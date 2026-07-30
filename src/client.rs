//! Blocking HTTP transport for SkillsMP search and repository snapshot fetching.

use crate::config::ProviderCredential;
use crate::core::{
    GitHubFileBlobData, GitHubRepositorySnapshotData, GitHubSkillLocationData,
    GitHubSnapshotFetcher, MAX_ARCHIVE_CUMULATIVE_SIZE, MAX_ARCHIVE_ENTRIES,
    MAX_ARCHIVE_RESOURCE_SIZE, MAX_ARCHIVE_SIZE, RepositoryFileBlobData, RepositoryLocationData,
    RepositoryProvider, RepositorySnapshotData, RepositorySnapshotFetcher,
};
use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::io::Read;
use std::time::Duration;
use tar::Archive;
use url::Url;

const SKILLSMP_API_KEY_ENV_VAR: &str = "SKILLSMP_API_KEY";
const SKILLY_GITHUB_TOKEN_ENV_VAR: &str = "SKILLY_GITHUB_TOKEN";
const GITHUB_TOKEN_ENV_VARS: [&str; 3] = [SKILLY_GITHUB_TOKEN_ENV_VAR, "GITHUB_TOKEN", "GH_TOKEN"];
const DEFAULT_SKILLSMP_API_BASE_URL: &str = "https://skillsmp.com/api/v1";
const DEFAULT_GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_API_BASE_URL_ENV_VAR: &str = "SKILLY_GITHUB_API_BASE_URL";
const BITBUCKET_CLOUD_API_BASE_URL_ENV_VAR: &str = "SKILLY_BITBUCKET_CLOUD_API_BASE_URL";
const DEFAULT_BITBUCKET_CLOUD_API_BASE_URL: &str = "https://api.bitbucket.org/2.0";
const SKILLY_BITBUCKET_CLOUD_TOKEN_ENV_VAR: &str = "SKILLY_BITBUCKET_CLOUD_TOKEN";
const SKILLY_BITBUCKET_DATA_CENTER_TOKEN_ENV_VAR: &str = "SKILLY_BITBUCKET_DATA_CENTER_TOKEN";
const SKILLY_USER_AGENT: &str = concat!("skilly/", env!("CARGO_PKG_VERSION"));
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub github_token: Option<String>,
    pub repository_token: Option<String>,
    repository_credentials: Vec<ProviderCredential>,
    pub proxy: Option<String>,
}

impl ClientConfig {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        github_token: Option<String>,
        proxy: Option<String>,
    ) -> Self {
        Self {
            base_url,
            api_key,
            github_token,
            repository_token: None,
            repository_credentials: Vec::new(),
            proxy,
        }
    }

    pub fn base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_SKILLSMP_API_BASE_URL.to_string())
    }

    pub fn github_api_base_url(&self) -> String {
        env::var(GITHUB_API_BASE_URL_ENV_VAR)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_GITHUB_API_BASE_URL.to_string())
    }

    pub fn api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| env::var(SKILLSMP_API_KEY_ENV_VAR).ok())
    }

    pub fn github_token(&self) -> Option<String> {
        if let Some(token) = self.github_token.clone() {
            return Some(token);
        }
        self.saved_repository_token(RepositoryProvider::GitHub, "https://github.com")
            .or_else(|| {
                GITHUB_TOKEN_ENV_VARS
                    .iter()
                    .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
            })
    }

    #[must_use]
    pub fn with_repository_token(mut self, token: Option<String>) -> Self {
        self.repository_token = token;
        self
    }

    #[must_use]
    pub(crate) fn with_repository_credentials(
        mut self,
        credentials: Vec<ProviderCredential>,
    ) -> Self {
        self.repository_credentials = credentials;
        self
    }

    pub fn repository_token(&self, provider: RepositoryProvider, base_url: &str) -> Option<String> {
        self.repository_token
            .clone()
            .or_else(|| self.saved_repository_token(provider, base_url))
            .or_else(|| match provider {
                RepositoryProvider::GitHub => self.github_token(),
                RepositoryProvider::BitbucketCloud => {
                    env::var(SKILLY_BITBUCKET_CLOUD_TOKEN_ENV_VAR)
                        .ok()
                        .filter(|value| !value.is_empty())
                }
                RepositoryProvider::BitbucketDataCenter => {
                    env::var(SKILLY_BITBUCKET_DATA_CENTER_TOKEN_ENV_VAR)
                        .ok()
                        .filter(|value| !value.is_empty())
                }
            })
    }

    fn saved_repository_token(
        &self,
        provider: RepositoryProvider,
        base_url: &str,
    ) -> Option<String> {
        self.repository_credentials
            .iter()
            .rev()
            .find(|credential| {
                credential.provider == provider && credential.url == base_url.trim_end_matches('/')
            })
            .map(|credential| credential.token.clone())
    }

    pub fn bitbucket_cloud_api_base_url(&self) -> String {
        env::var(BITBUCKET_CLOUD_API_BASE_URL_ENV_VAR)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_BITBUCKET_CLOUD_API_BASE_URL.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsMpSearchQuery {
    pub q: String,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort_by: Option<String>,
    pub category: Option<String>,
    pub occupation: Option<String>,
}

impl SkillsMpSearchQuery {
    pub fn new(q: impl Into<String>) -> Self {
        Self {
            q: q.into(),
            ..Self::default()
        }
    }

    fn params(&self) -> Vec<(String, String)> {
        let mut params = vec![("q".to_string(), self.q.clone())];
        if let Some(page) = self.page {
            params.push(("page".to_string(), page.to_string()));
        }
        if let Some(limit) = self.limit {
            params.push(("limit".to_string(), limit.to_string()));
        }
        if let Some(sort_by) = self.sort_by.as_ref() {
            params.push(("sortBy".to_string(), sort_by.clone()));
        }
        if let Some(category) = self.category.as_ref() {
            params.push(("category".to_string(), category.clone()));
        }
        if let Some(occupation) = self.occupation.as_ref() {
            params.push(("occupation".to_string(), occupation.clone()));
        }
        params
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpSkill {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    #[serde(rename = "githubUrl")]
    pub repository_url: String,
    #[serde(rename = "skillUrl")]
    pub skill_url: String,
    #[serde(rename = "stars")]
    pub stars: Option<i64>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpPagination {
    pub page: i64,
    pub limit: i64,
    pub total: i64,
    #[serde(rename = "totalPages")]
    pub total_pages: i64,
    #[serde(rename = "hasNext")]
    pub has_next: bool,
    #[serde(rename = "hasPrev")]
    pub has_prev: bool,
    #[serde(rename = "totalIsExact")]
    pub total_is_exact: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsMpFilters {
    pub search: Option<String>,
    #[serde(rename = "sortBy")]
    pub sort_by: Option<String>,
    pub category: Option<String>,
    pub occupation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpSearchData {
    pub skills: Vec<SkillsMpSkill>,
    pub pagination: SkillsMpPagination,
    pub filters: SkillsMpFilters,
}

#[cfg(feature = "python-bindings")]
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsMpAiSearchData {
    #[serde(default)]
    pub skills: Vec<SkillsMpSkill>,
    #[serde(default)]
    pub results: Vec<SkillsMpSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsMpMeta {
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    #[serde(rename = "responseTimeMs")]
    pub response_time_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpSearchApiResponse {
    pub success: bool,
    pub data: SkillsMpSearchData,
    pub meta: Option<SkillsMpMeta>,
}

#[cfg(feature = "python-bindings")]
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpAiSearchApiResponse {
    pub success: bool,
    pub data: SkillsMpAiSearchData,
    pub meta: Option<SkillsMpMeta>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepositoryInfo {
    default_branch: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubCommitInfo {
    sha: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketCloudRepositoryInfo {
    mainbranch: Option<BitbucketCloudMainBranch>,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketCloudMainBranch {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketCloudCommit {
    hash: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketCloudSourcePage {
    values: Vec<BitbucketCloudSourceEntry>,
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketCloudSourceEntry {
    r#type: String,
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketDataCenterRef {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BitbucketDataCenterCommit {
    id: String,
}

pub struct SkillsMpClient {
    config: ClientConfig,
    client: Client,
}

/// Internal request boundary for repository discovery.
///
/// Provider discovery constructs each request once and delegates its execution
/// through this boundary. Public Python callers supply the transport that owns
/// network policy and response bounds.
pub(crate) trait RepositoryHttpTransport {
    fn get(&self, url: &str, headers: HeaderMap, params: &[(String, String)]) -> Result<Vec<u8>>;
}

struct ReqwestRepositoryHttpTransport<'a> {
    client: &'a Client,
}

impl RepositoryHttpTransport for ReqwestRepositoryHttpTransport<'_> {
    fn get(&self, url: &str, headers: HeaderMap, params: &[(String, String)]) -> Result<Vec<u8>> {
        Ok(self
            .client
            .get(url)
            .headers(headers)
            .query(params)
            .send()?
            .error_for_status()?
            .bytes()?
            .to_vec())
    }
}

fn repository_get_json<T: for<'de> Deserialize<'de>>(
    transport: &dyn RepositoryHttpTransport,
    url: &str,
    headers: HeaderMap,
    params: &[(String, String)],
) -> Result<T> {
    let body = transport.get(url, headers, params)?;
    serde_json::from_slice(&body).context("repository response is not valid JSON")
}

impl SkillsMpClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent(SKILLY_USER_AGENT)
            .timeout(DEFAULT_REQUEST_TIMEOUT);
        if let Some(proxy) = config.proxy.as_ref() {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(Self {
            config,
            client: builder.build()?,
        })
    }

    fn api_headers(&self, require_api_key: bool) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        if let Some(api_key) = self.config.api_key() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))?,
            );
        } else if require_api_key {
            bail!(
                "API key is required; set it via environment variable {SKILLSMP_API_KEY_ENV_VAR} or pass it to the client"
            );
        }
        Ok(headers)
    }

    fn github_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(SKILLY_USER_AGENT));
        if let Some(token) = self.config.github_token() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))?,
            );
        }
        Ok(headers)
    }

    fn repository_headers(
        &self,
        provider: RepositoryProvider,
        base_url: &str,
    ) -> Result<HeaderMap> {
        if provider == RepositoryProvider::GitHub {
            return self.github_headers();
        }
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(SKILLY_USER_AGENT));
        if let Some(token) = self.config.repository_token(provider, base_url) {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))?,
            );
        }
        Ok(headers)
    }

    fn skillsmp_url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url().trim_end_matches('/'), path)
    }

    fn github_repo_url(&self, location: &GitHubSkillLocationData, suffix: &str) -> String {
        let base = format!(
            "{}/repos/{}/{}",
            self.config.github_api_base_url().trim_end_matches('/'),
            location.owner,
            location.repo
        );
        if suffix.is_empty() {
            base
        } else {
            format!("{base}/{}", suffix.trim_start_matches('/'))
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        headers: HeaderMap,
        params: &[(String, String)],
    ) -> Result<T> {
        Ok(self
            .client
            .get(url)
            .headers(headers)
            .query(params)
            .send()?
            .error_for_status()?
            .json::<T>()?)
    }

    pub fn search(&self, query: &SkillsMpSearchQuery) -> Result<SkillsMpSearchApiResponse> {
        self.get_json(
            &self.skillsmp_url("/skills/search"),
            self.api_headers(false)?,
            &query.params(),
        )
    }

    #[cfg(feature = "python-bindings")]
    #[allow(dead_code)]
    pub fn ai_search(&self, q: &str) -> Result<SkillsMpAiSearchApiResponse> {
        self.get_json(
            &self.skillsmp_url("/skills/ai-search"),
            self.api_headers(true)?,
            &[("q".to_string(), q.to_string())],
        )
    }

    fn resolve_github_ref_and_commit_sha_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &GitHubSkillLocationData,
    ) -> Result<(String, String)> {
        if let Some(reference) = location.r#ref.as_ref()
            && looks_like_commit_sha(reference)
        {
            return Ok((reference.clone(), reference.clone()));
        }

        let reference = match location.r#ref.as_ref() {
            Some(reference) => reference.clone(),
            None => {
                repository_get_json::<GitHubRepositoryInfo>(
                    transport,
                    &self.github_repo_url(location, ""),
                    self.github_headers()?,
                    &[],
                )?
                .default_branch
            }
        };

        let commit = repository_get_json::<GitHubCommitInfo>(
            transport,
            &self.github_repo_url(location, &format!("commits/{reference}")),
            self.github_headers()?,
            &[],
        )?;
        Ok((reference, commit.sha))
    }

    fn fetch_github_snapshot_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData> {
        let (reference, commit_sha) =
            self.resolve_github_ref_and_commit_sha_with(transport, location)?;
        let archive_bytes = transport.get(
            &self.github_repo_url(location, &format!("tarball/{commit_sha}")),
            self.github_headers()?,
            &[],
        )?;
        Ok(GitHubRepositorySnapshotData {
            ref_name: reference,
            commit_sha: commit_sha.clone(),
            files: extract_github_archive_files(&archive_bytes, &commit_sha)?,
        })
    }

    pub(crate) fn fetch_repository_snapshot_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData> {
        match location.provider {
            RepositoryProvider::GitHub => {
                self.fetch_github_repository_snapshot_with(transport, location)
            }
            RepositoryProvider::BitbucketCloud => {
                self.fetch_bitbucket_cloud_snapshot_with(transport, location)
            }
            RepositoryProvider::BitbucketDataCenter => {
                self.fetch_bitbucket_data_center_snapshot_with(transport, location)
            }
        }
    }

    fn fetch_github_repository_snapshot_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData> {
        let snapshot = self.fetch_github_snapshot_with(
            transport,
            &GitHubSkillLocationData {
                owner: location.namespace.clone(),
                repo: location.repo.clone(),
                r#ref: location.r#ref.clone(),
                path: location.path.clone(),
                url: location.url.clone(),
            },
        )?;
        Ok(RepositorySnapshotData {
            ref_name: snapshot.ref_name,
            commit_sha: snapshot.commit_sha,
            files: snapshot
                .files
                .into_iter()
                .map(|(path, blob)| {
                    (
                        path,
                        RepositoryFileBlobData {
                            path: blob.path,
                            content: blob.content.into_bytes(),
                            commit_sha: blob.commit_sha,
                        },
                    )
                })
                .collect(),
        })
    }

    fn fetch_bitbucket_cloud_snapshot_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData> {
        let repository_url = self.bitbucket_cloud_repository_url(location, "");
        let reference = match location.r#ref.as_ref() {
            Some(reference) => reference.clone(),
            None => repository_get_json::<BitbucketCloudRepositoryInfo>(
                transport,
                &repository_url,
                self.repository_headers(RepositoryProvider::BitbucketCloud, &location.base_url)?,
                &[],
            )?
            .mainbranch
            .map(|branch| branch.name)
            .context("Bitbucket Cloud repository does not define a main branch")?,
        };
        let commit = repository_get_json::<BitbucketCloudCommit>(
            transport,
            &self.bitbucket_cloud_repository_url(location, &format!("commit/{reference}")),
            self.repository_headers(RepositoryProvider::BitbucketCloud, &location.base_url)?,
            &[],
        )?;
        let files = self.fetch_bitbucket_cloud_files_with(transport, location, &commit.hash)?;
        Ok(RepositorySnapshotData {
            ref_name: reference,
            commit_sha: commit.hash,
            files,
        })
    }

    fn fetch_bitbucket_cloud_files_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &RepositoryLocationData,
        commit_sha: &str,
    ) -> Result<BTreeMap<String, RepositoryFileBlobData>> {
        let mut files = BTreeMap::new();
        let mut cumulative_size = 0_u64;
        let root = normalize_repository_path(&location.path)?;
        let mut directories = vec![root.clone()];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(directory) = directories.pop() {
            if !visited.insert(directory.clone()) {
                continue;
            }
            let mut page_url = self.bitbucket_cloud_source_url(location, commit_sha, &directory);
            loop {
                let page = repository_get_json::<BitbucketCloudSourcePage>(
                    transport,
                    &page_url,
                    self.repository_headers(
                        RepositoryProvider::BitbucketCloud,
                        &location.base_url,
                    )?,
                    &[],
                )?;
                for entry in page.values {
                    let path = normalize_repository_path(&entry.path)?;
                    if !repository_path_is_within_root(&path, &root) {
                        bail!(
                            "Bitbucket Cloud returned a path outside the requested repository root: {path}"
                        );
                    }
                    match entry.r#type.as_str() {
                        "commit_file" => {
                            if files.len() >= MAX_ARCHIVE_ENTRIES {
                                bail!(
                                    "Bitbucket Cloud source tree exceeds maximum {MAX_ARCHIVE_ENTRIES} files"
                                );
                            }
                            let bytes = transport.get(
                                &self.bitbucket_cloud_source_url(location, commit_sha, &path),
                                self.repository_headers(
                                    RepositoryProvider::BitbucketCloud,
                                    &location.base_url,
                                )?,
                                &[],
                            )?;
                            if bytes.len() as u64 > MAX_ARCHIVE_RESOURCE_SIZE {
                                bail!(
                                    "Bitbucket Cloud file {path} exceeds maximum {MAX_ARCHIVE_RESOURCE_SIZE} bytes"
                                );
                            }
                            cumulative_size = cumulative_size
                                .checked_add(bytes.len() as u64)
                                .context("Bitbucket Cloud source tree size overflow")?;
                            if cumulative_size > MAX_ARCHIVE_CUMULATIVE_SIZE {
                                bail!(
                                    "Bitbucket Cloud source tree exceeds maximum {MAX_ARCHIVE_CUMULATIVE_SIZE} bytes"
                                );
                            }
                            files.insert(
                                path.clone(),
                                RepositoryFileBlobData {
                                    path,
                                    content: bytes,
                                    commit_sha: Some(commit_sha.to_string()),
                                },
                            );
                        }
                        "commit_directory" => directories.push(path),
                        other => bail!("unsupported Bitbucket Cloud source entry type: {other}"),
                    }
                }
                let Some(next) = page.next else {
                    break;
                };
                if !same_url_origin(&next, &self.config.bitbucket_cloud_api_base_url()) {
                    bail!("Bitbucket Cloud returned a pagination URL outside its API origin");
                }
                page_url = next;
            }
        }
        Ok(files)
    }

    fn fetch_bitbucket_data_center_snapshot_with(
        &self,
        transport: &dyn RepositoryHttpTransport,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData> {
        let repository_url = self.bitbucket_data_center_repository_url(location, "");
        let reference = match location.r#ref.as_ref() {
            Some(reference) => reference.clone(),
            None => {
                repository_get_json::<BitbucketDataCenterRef>(
                    transport,
                    &format!("{repository_url}/default-branch"),
                    self.repository_headers(
                        RepositoryProvider::BitbucketDataCenter,
                        &location.base_url,
                    )?,
                    &[],
                )?
                .id
            }
        };
        let commit = repository_get_json::<BitbucketDataCenterCommit>(
            transport,
            &format!("{repository_url}/commits/{reference}"),
            self.repository_headers(RepositoryProvider::BitbucketDataCenter, &location.base_url)?,
            &[],
        )?;
        let archive_bytes = transport.get(
            &format!("{repository_url}/archive"),
            self.repository_headers(RepositoryProvider::BitbucketDataCenter, &location.base_url)?,
            &[
                ("at".to_string(), commit.id.clone()),
                ("format".to_string(), "tar.gz".to_string()),
            ],
        )?;
        if archive_bytes.len() as u64 > MAX_ARCHIVE_SIZE {
            bail!("Bitbucket Data Center archive exceeds maximum {MAX_ARCHIVE_SIZE} bytes");
        }
        Ok(RepositorySnapshotData {
            ref_name: reference,
            commit_sha: commit.id.clone(),
            files: extract_repository_archive_files(&archive_bytes, &commit.id)?,
        })
    }

    fn bitbucket_cloud_repository_url(
        &self,
        location: &RepositoryLocationData,
        suffix: &str,
    ) -> String {
        let base = format!(
            "{}/repositories/{}/{}",
            self.config
                .bitbucket_cloud_api_base_url()
                .trim_end_matches('/'),
            location.namespace,
            location.repo
        );
        if suffix.is_empty() {
            base
        } else {
            format!("{base}/{}", suffix.trim_start_matches('/'))
        }
    }

    fn bitbucket_cloud_source_url(
        &self,
        location: &RepositoryLocationData,
        commit_sha: &str,
        path: &str,
    ) -> String {
        let suffix = if path == "." || path.is_empty() {
            format!("src/{commit_sha}/")
        } else {
            format!("src/{commit_sha}/{path}")
        };
        self.bitbucket_cloud_repository_url(location, &suffix)
    }

    fn bitbucket_data_center_repository_url(
        &self,
        location: &RepositoryLocationData,
        suffix: &str,
    ) -> String {
        let base = format!(
            "{}/rest/api/latest/projects/{}/repos/{}",
            location.base_url.trim_end_matches('/'),
            location.namespace,
            location.repo
        );
        if suffix.is_empty() {
            base
        } else {
            format!("{base}/{}", suffix.trim_start_matches('/'))
        }
    }
}

impl RepositoryHttpTransport for SkillsMpClient {
    fn get(&self, url: &str, headers: HeaderMap, params: &[(String, String)]) -> Result<Vec<u8>> {
        ReqwestRepositoryHttpTransport {
            client: &self.client,
        }
        .get(url, headers, params)
    }
}

impl GitHubSnapshotFetcher for SkillsMpClient {
    fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData> {
        self.fetch_github_snapshot_with(self, location)
    }
}

impl RepositorySnapshotFetcher for SkillsMpClient {
    fn fetch_repository_snapshot(
        &self,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData> {
        self.fetch_repository_snapshot_with(self, location)
    }
}

fn looks_like_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn normalize_repository_path(path: &str) -> Result<String> {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() || normalized == "." {
        return Ok(".".to_string());
    }
    if normalized
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("repository response contains an unsafe path: {path}");
    }
    Ok(normalized.to_string())
}

fn repository_path_is_within_root(path: &str, root: &str) -> bool {
    root == "." || path == root || path.starts_with(&format!("{root}/"))
}

fn same_url_origin(candidate: &str, base: &str) -> bool {
    let Ok(candidate) = Url::parse(candidate) else {
        return false;
    };
    let Ok(base) = Url::parse(base) else {
        return false;
    };
    candidate.scheme() == base.scheme()
        && candidate.host_str() == base.host_str()
        && candidate.port_or_known_default() == base.port_or_known_default()
}

fn extract_repository_archive_files(
    archive_bytes: &[u8],
    commit_sha: &str,
) -> Result<BTreeMap<String, RepositoryFileBlobData>> {
    if archive_bytes.len() as u64 > MAX_ARCHIVE_SIZE {
        bail!("repository archive exceeds maximum {MAX_ARCHIVE_SIZE} bytes");
    }
    let mut files = BTreeMap::new();
    let mut entry_count = 0_usize;
    let mut cumulative_size = 0_u64;
    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(decoder);
    for entry in archive
        .entries()
        .context("invalid repository archive response")?
    {
        let mut entry = entry?;
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            bail!("repository archive exceeds maximum {MAX_ARCHIVE_ENTRIES} entries");
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().to_string();
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }
        let relative_path = normalize_repository_path(&parts[1..].join("/"))?;
        let size = entry.size();
        if size > MAX_ARCHIVE_RESOURCE_SIZE {
            bail!(
                "repository archive entry {relative_path} exceeds maximum {MAX_ARCHIVE_RESOURCE_SIZE} bytes"
            );
        }
        cumulative_size = cumulative_size
            .checked_add(size)
            .context("repository archive size overflow")?;
        if cumulative_size > MAX_ARCHIVE_CUMULATIVE_SIZE {
            bail!("repository archive exceeds maximum {MAX_ARCHIVE_CUMULATIVE_SIZE} bytes");
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        files.insert(
            relative_path.clone(),
            RepositoryFileBlobData {
                path: relative_path,
                content,
                commit_sha: Some(commit_sha.to_string()),
            },
        );
    }
    Ok(files)
}

fn extract_github_archive_files(
    archive_bytes: &[u8],
    commit_sha: &str,
) -> Result<BTreeMap<String, GitHubFileBlobData>> {
    let mut files = BTreeMap::new();
    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(decoder);
    for entry in archive
        .entries()
        .context("invalid GitHub archive response")?
    {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().to_string();
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }
        let relative_path = parts[1..].join("/");
        let mut content_bytes = Vec::new();
        entry.read_to_end(&mut content_bytes)?;
        let Ok(content) = String::from_utf8(content_bytes.clone()) else {
            continue;
        };
        files.insert(
            relative_path.clone(),
            GitHubFileBlobData {
                path: relative_path,
                content,
                size: content_bytes.len(),
                commit_sha: Some(commit_sha.to_string()),
            },
        );
    }
    Ok(files)
}
