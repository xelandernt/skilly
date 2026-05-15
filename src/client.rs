use crate::core::{
    GitHubContentItemData, GitHubFileBlobData, GitHubRepositorySnapshotData,
    GitHubSkillLocationData, GitHubSnapshotFetcher,
};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::io::Read;
use tar::Archive;

const SKILLSMP_API_KEY_ENV_VAR: &str = "SKILLSMP_API_KEY";
const SKILLY_GITHUB_TOKEN_ENV_VAR: &str = "SKILLY_GITHUB_TOKEN";
const GITHUB_TOKEN_ENV_VARS: [&str; 3] = [SKILLY_GITHUB_TOKEN_ENV_VAR, "GITHUB_TOKEN", "GH_TOKEN"];
const DEFAULT_SKILLSMP_API_BASE_URL: &str = "https://skillsmp.com/api/v1";
const DEFAULT_GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_API_BASE_URL_ENV_VAR: &str = "SKILLY_GITHUB_API_BASE_URL";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub github_token: Option<String>,
    pub proxy: Option<String>,
}

impl ClientConfig {
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
        GITHUB_TOKEN_ENV_VARS
            .iter()
            .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpSkill {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    #[serde(rename = "githubUrl")]
    pub github_url: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsMpAiSearchApiResponse {
    pub success: bool,
    pub data: SkillsMpAiSearchData,
    pub meta: Option<SkillsMpMeta>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubContentEntry {
    r#type: String,
    name: String,
    path: String,
    html_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubFileContent {
    path: String,
    html_url: Option<String>,
    size: Option<usize>,
    encoding: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepositoryInfo {
    default_branch: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubCommitInfo {
    sha: String,
}

pub struct SkillsMpClient {
    config: ClientConfig,
    client: Client,
}

impl SkillsMpClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let mut builder = Client::builder().user_agent("skilly/0.0.1");
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
                "API key is required. Set it via environment variable {SKILLSMP_API_KEY_ENV_VAR} or pass it to the client."
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
        headers.insert(USER_AGENT, HeaderValue::from_static("skilly/0.0.1"));
        if let Some(token) = self.config.github_token() {
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

    fn github_contents_url(&self, location: &GitHubSkillLocationData, path: &str) -> String {
        let base = format!(
            "{}/repos/{}/{}/contents",
            self.config.github_api_base_url().trim_end_matches('/'),
            location.owner,
            location.repo
        );
        if path == "." || path.is_empty() {
            base
        } else {
            format!("{base}/{path}")
        }
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

    fn get_bytes(
        &self,
        url: &str,
        headers: HeaderMap,
        params: &[(String, String)],
    ) -> Result<Vec<u8>> {
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

    pub fn search(
        &self,
        q: &str,
        page: Option<u32>,
        limit: Option<u32>,
        sort_by: Option<&str>,
        category: Option<&str>,
        occupation: Option<&str>,
    ) -> Result<SkillsMpSearchApiResponse> {
        let mut params = vec![("q".to_string(), q.to_string())];
        if let Some(page) = page {
            params.push(("page".to_string(), page.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit".to_string(), limit.to_string()));
        }
        if let Some(sort_by) = sort_by {
            params.push(("sortBy".to_string(), sort_by.to_string()));
        }
        if let Some(category) = category {
            params.push(("category".to_string(), category.to_string()));
        }
        if let Some(occupation) = occupation {
            params.push(("occupation".to_string(), occupation.to_string()));
        }
        self.get_json(
            &self.skillsmp_url("/skills/search"),
            self.api_headers(false)?,
            &params,
        )
    }

    pub fn ai_search(&self, q: &str) -> Result<SkillsMpAiSearchApiResponse> {
        self.get_json(
            &self.skillsmp_url("/skills/ai-search"),
            self.api_headers(true)?,
            &[("q".to_string(), q.to_string())],
        )
    }

    pub fn fetch_github_directory(
        &self,
        location: &GitHubSkillLocationData,
        current_path: &str,
    ) -> Result<Vec<GitHubContentItemData>> {
        let mut params = Vec::new();
        if let Some(reference) = location.r#ref.as_ref() {
            params.push(("ref".to_string(), reference.clone()));
        }
        let entries = self.get_json::<Vec<GitHubContentEntry>>(
            &self.github_contents_url(location, current_path),
            self.github_headers()?,
            &params,
        )?;
        Ok(entries
            .into_iter()
            .map(|entry| GitHubContentItemData {
                r#type: entry.r#type,
                name: entry.name,
                path: entry.path,
                commit_sha: extract_commit_sha_from_html_url(entry.html_url.as_deref()),
            })
            .collect())
    }

    pub fn fetch_github_file(
        &self,
        location: &GitHubSkillLocationData,
        path: &str,
    ) -> Result<GitHubFileBlobData> {
        let mut params = Vec::new();
        if let Some(reference) = location.r#ref.as_ref() {
            params.push(("ref".to_string(), reference.clone()));
        }
        let file = self.get_json::<GitHubFileContent>(
            &self.github_contents_url(location, path),
            self.github_headers()?,
            &params,
        )?;
        let content = decode_github_file_content(&file)?;
        Ok(GitHubFileBlobData {
            path: file.path,
            size: file.size.unwrap_or(content.len()),
            content,
            commit_sha: extract_commit_sha_from_html_url(file.html_url.as_deref()),
        })
    }

    pub fn resolve_github_ref_and_commit_sha(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<(String, String)> {
        if let Some(reference) = location.r#ref.as_ref() {
            if looks_like_commit_sha(reference) {
                return Ok((reference.clone(), reference.clone()));
            }
        }

        let reference = match location.r#ref.as_ref() {
            Some(reference) => reference.clone(),
            None => {
                self.get_json::<GitHubRepositoryInfo>(
                    &self.github_repo_url(location, ""),
                    self.github_headers()?,
                    &[],
                )?
                .default_branch
            }
        };

        let commit = self.get_json::<GitHubCommitInfo>(
            &self.github_repo_url(location, &format!("commits/{reference}")),
            self.github_headers()?,
            &[],
        )?;
        Ok((reference, commit.sha))
    }

    pub fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData> {
        let (reference, commit_sha) = self.resolve_github_ref_and_commit_sha(location)?;
        let archive_bytes = self.get_bytes(
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
}

impl GitHubSnapshotFetcher for SkillsMpClient {
    fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData> {
        SkillsMpClient::fetch_github_snapshot(self, location)
    }
}

fn decode_github_file_content(file: &GitHubFileContent) -> Result<String> {
    let Some(content) = file.content.as_ref() else {
        bail!("GitHub file response for {} is missing content", file.path);
    };
    if !matches!(file.encoding.as_deref(), None | Some("base64")) {
        bail!(
            "Unsupported GitHub file encoding for {}: {}",
            file.path,
            file.encoding.as_deref().unwrap_or_default()
        );
    }
    let normalized = content.replace('\n', "");
    let decoded = STANDARD.decode(normalized)?;
    String::from_utf8(decoded).map_err(Into::into)
}

fn extract_commit_sha_from_html_url(html_url: Option<&str>) -> Option<String> {
    let html_url = html_url?;
    let parts = html_url
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 6 || !matches!(parts[0], "http:" | "https:") {
        return None;
    }
    if parts[1] != "github.com" || !matches!(parts[4], "blob" | "tree") {
        return None;
    }
    Some(parts[5].to_string())
}

fn looks_like_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
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
        .context("Invalid GitHub archive response")?
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
