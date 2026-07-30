//! Domain models, validation, filesystem-independent operations, and core business logic
//! for skill discovery, installation, scanning, and update management.

use anyhow::{Context, Result, anyhow, bail};
use csv::ReaderBuilder;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

pub const DEFAULT_SKILLS_PATH: &str = ".agents/skills";
pub const CLAUDE_SKILLS_PATH: &str = ".claude/skills";
pub const CODEX_SKILLS_PATH: &str = ".codex/skills";
pub const COPILOT_LOCAL_SKILLS_PATH: &str = ".github/skills";
pub const COPILOT_GLOBAL_SKILLS_PATH: &str = ".copilot/skills";
pub const SKILLY_DEFAULT_DIRECTORY_ENV_VAR: &str = "SKILLY_DEFAULT_DIRECTORY";
pub const RESOURCE_KIND_SCRIPT: &str = "script";
pub const RESOURCE_KIND_REFERENCE: &str = "reference";
pub const RESOURCE_KIND_ASSET: &str = "asset";
pub const RESOURCE_KIND_OTHER: &str = "other";

pub const SKILLY_SOURCE_METADATA_KEY: &str = "skilly-source";
pub const SKILLY_SOURCE_DEPENDENCY: &str = "dependency";
pub const SKILLY_SOURCE_REPOSITORY: &str = "repository";
pub const SKILLY_UNKNOWN_SOURCE: &str = "unknown";
pub const SKILLY_REPOSITORY_PROVIDER_METADATA_KEY: &str = "skilly-repository-provider";
pub const SKILLY_REPOSITORY_URL_METADATA_KEY: &str = "skilly-repository-url";
pub const SKILLY_REPOSITORY_COMMIT_SHA_METADATA_KEY: &str = "skilly-repository-commit-sha";
pub const SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY: &str = "skilly-package-name";
pub const SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY: &str = "skilly-package-version";
pub const SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY: &str = "skilly-package-ecosystem";
pub const PACKAGE_ECOSYSTEM_PYTHON: &str = "python";
pub const PACKAGE_ECOSYSTEM_NODE: &str = "node";
pub const PACKAGE_ECOSYSTEM_MAVEN: &str = "maven";

pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_INSTALLABLE: &str = "installable";
pub const STATUS_UPDATABLE: &str = "updatable";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PackageEcosystem(pub String);

impl PackageEcosystem {
    #[must_use]
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub const MAX_DESCRIPTION_LENGTH: usize = 1024;
pub const MAX_COMPATIBILITY_LENGTH: usize = 500;

static TEMPORARY_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Target agent directory convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkillDirectoryFlavor {
    #[default]
    Agents,
    Claude,
    Codex,
    Copilot,
}

/// Resolve the skills directory path for a given agent flavor and scope.
pub fn skills_directory(flavor: SkillDirectoryFlavor, global: bool) -> Result<PathBuf> {
    let relative = match (flavor, global) {
        (SkillDirectoryFlavor::Agents, _) => DEFAULT_SKILLS_PATH,
        (SkillDirectoryFlavor::Claude, _) => CLAUDE_SKILLS_PATH,
        (SkillDirectoryFlavor::Codex, _) => CODEX_SKILLS_PATH,
        (SkillDirectoryFlavor::Copilot, false) => COPILOT_LOCAL_SKILLS_PATH,
        (SkillDirectoryFlavor::Copilot, true) => COPILOT_GLOBAL_SKILLS_PATH,
    };
    if !global {
        return Ok(PathBuf::from(relative));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("could not determine the user home directory"))?;
    Ok(home.join(relative))
}

/// Resolve the default skills directory:
/// 1. `SKILLY_DEFAULT_DIRECTORY` env var (highest priority).
/// 2. Falls back to `.agents/skills`.
pub fn default_skills_directory() -> Result<PathBuf> {
    if let Some(directory) = env::var_os(SKILLY_DEFAULT_DIRECTORY_ENV_VAR) {
        return absolute_path(Path::new(&directory));
    }
    Ok(PathBuf::from(DEFAULT_SKILLS_PATH))
}

/// Resolve the effective default directory path from environment, config,
/// and application default, in priority order.
///
/// 1. `SKILLY_DEFAULT_DIRECTORY` env var.
/// 2. `config.default_directory`.
/// 3. Application default (`.agents/skills`).
pub fn resolve_default_directory(config: Option<&str>) -> String {
    if let Ok(directory) = env::var(SKILLY_DEFAULT_DIRECTORY_ENV_VAR) {
        return directory;
    }
    if let Some(cfg_default) = config
        && !cfg_default.is_empty()
    {
        return cfg_default.to_string();
    }
    DEFAULT_SKILLS_PATH.to_string()
}

pub(crate) fn absolute_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_home_path(path)?;
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    Ok(env::current_dir()?.join(expanded))
}

fn expand_home_path(path: &Path) -> Result<PathBuf> {
    let value = path.to_string_lossy();
    if value == "~" || value.starts_with("~/") {
        let home = env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .ok_or_else(|| anyhow!("could not determine the user home directory"))?;
        let home = PathBuf::from(home);
        if value == "~" {
            return Ok(home);
        }
        return Ok(home.join(value.trim_start_matches("~/")));
    }
    Ok(path.to_path_buf())
}

/// Bundled resource file (script, reference, or asset) within a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillResourceData {
    pub relative_path: String,
    pub kind: String,
    #[serde(default, with = "serde_bytes")]
    pub raw: Vec<u8>,
}

/// A stable machine-readable reason why an in-memory skill bundle is invalid.
#[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleValidationCode {
    InvalidUtf8,
    InvalidFrontmatter,
    InvalidField,
    InvalidResourcePath,
    DuplicateResourcePath,
}

#[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
impl BundleValidationCode {
    /// Stable identifier exposed by the Python `SkillBundleError.code` attribute.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUtf8 => "invalid_utf8",
            Self::InvalidFrontmatter => "invalid_frontmatter",
            Self::InvalidField => "invalid_field",
            Self::InvalidResourcePath => "invalid_resource_path",
            Self::DuplicateResourcePath => "duplicate_resource_path",
        }
    }
}

/// A validation failure returned when loading a skill bundle from memory.
#[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleValidationError {
    pub code: BundleValidationCode,
    pub path: String,
    pub field: Option<String>,
    pub message: String,
}

#[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
impl BundleValidationError {
    fn new(
        code: BundleValidationCode,
        path: impl Into<String>,
        field: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            field: field.map(str::to_string),
            message: message.into(),
        }
    }
}

impl fmt::Display for BundleValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BundleValidationError {}

/// A Git hosting provider supported by repository-backed skill discovery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum RepositoryProvider {
    #[serde(rename = "github")]
    GitHub,
    BitbucketCloud,
    BitbucketDataCenter,
}

impl RepositoryProvider {
    /// Stable identifier used by CLI, Python, and persisted provenance.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::BitbucketCloud => "bitbucket-cloud",
            Self::BitbucketDataCenter => "bitbucket-data-center",
        }
    }
}

impl std::str::FromStr for RepositoryProvider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "github" => Ok(Self::GitHub),
            "bitbucket-cloud" => Ok(Self::BitbucketCloud),
            "bitbucket-data-center" => Ok(Self::BitbucketDataCenter),
            _ => bail!(
                "unsupported repository provider {value}; expected github, bitbucket-cloud, or bitbucket-data-center"
            ),
        }
    }
}

/// A validated repository location, independent of its Git hosting provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryLocationData {
    pub provider: RepositoryProvider,
    /// Web origin and any reverse-proxy path before a Data Center `projects` route.
    pub base_url: String,
    /// GitHub owner, Bitbucket Cloud workspace, or Bitbucket Data Center project.
    pub namespace: String,
    pub repo: String,
    pub r#ref: Option<String>,
    pub path: String,
    pub url: String,
}

/// A binary file retrieved from a provider snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryFileBlobData {
    pub path: String,
    #[serde(with = "serde_bytes")]
    pub content: Vec<u8>,
    pub commit_sha: Option<String>,
}

/// A repository tree pinned to an immutable commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySnapshotData {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub commit_sha: String,
    pub files: BTreeMap<String, RepositoryFileBlobData>,
}

/// Parsed GitHub skill location: owner, repo, ref, path, and the original URL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubSkillLocationData {
    pub owner: String,
    pub repo: String,
    pub r#ref: Option<String>,
    pub path: String,
    pub url: String,
}

/// A single file blob retrieved from a GitHub repository snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubFileBlobData {
    pub path: String,
    pub content: String,
    pub size: usize,
    pub commit_sha: Option<String>,
}

/// Full snapshot of a GitHub repository: ref, commit SHA, and file tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubRepositorySnapshotData {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub commit_sha: String,
    pub files: BTreeMap<String, GitHubFileBlobData>,
}

/// Complete skill model: frontmatter, body, resources, and source provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillData {
    pub name: String,
    pub description: String,
    pub path: Option<String>,
    #[serde(default)]
    pub body: String,
    /// Exact source bytes of the root `SKILL.md` document.
    #[serde(default, with = "serde_bytes")]
    pub raw: Vec<u8>,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    pub allowed_tools: Option<String>,
    #[serde(default)]
    pub resources: Vec<SkillResourceData>,
    #[serde(default)]
    pub resource_warnings: Vec<String>,
    #[serde(default = "default_unknown_source")]
    pub source: String,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
    pub repository_provider: Option<RepositoryProvider>,
    pub repository_url: Option<String>,
    pub repository_commit_sha: Option<String>,
    pub package_ecosystem: Option<PackageEcosystem>,
}

/// Pairing of an available skill, its installed counterpart, and dependency origins.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMatchData {
    pub available: SkillData,
    pub installed: Option<SkillData>,
    #[serde(default)]
    pub dependency_origins: Vec<ProjectDependencyOrigin>,
}

/// Source provenance for a skill: which channel installed it and tracking metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkillSourceMetadata {
    pub source: Option<String>,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
    pub repository_provider: Option<RepositoryProvider>,
    pub repository_url: Option<String>,
    pub repository_commit_sha: Option<String>,
    pub package_ecosystem: Option<PackageEcosystem>,
}

/// Which part of a project produced a given dependency requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectDependencyOrigin {
    pub ecosystem: String,
    pub scope: String,
}

impl ProjectDependencyOrigin {
    #[must_use]
    pub fn python_project() -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_PYTHON.to_string(),
            scope: "project".to_string(),
        }
    }

    #[must_use]
    pub fn python_dependency_group(group: impl Into<String>) -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_PYTHON.to_string(),
            scope: format!("group:{}", group.into()),
        }
    }

    #[must_use]
    pub fn python_optional_dependency(extra: impl Into<String>) -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_PYTHON.to_string(),
            scope: format!("extra:{}", extra.into()),
        }
    }

    #[must_use]
    pub fn node_dependencies() -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_NODE.to_string(),
            scope: "dependencies".to_string(),
        }
    }

    #[must_use]
    pub fn node_dev_dependencies() -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_NODE.to_string(),
            scope: "devDependencies".to_string(),
        }
    }

    #[must_use]
    pub fn node_optional_dependencies() -> Self {
        Self {
            ecosystem: PACKAGE_ECOSYSTEM_NODE.to_string(),
            scope: "optionalDependencies".to_string(),
        }
    }

    #[must_use]
    pub fn scan_label(&self) -> String {
        format!("{}:{}", self.ecosystem, self.scope)
    }

    #[must_use]
    pub fn detail_label(&self) -> String {
        match (self.ecosystem.as_str(), self.scope.as_str()) {
            ("python", "project") => "python project dependency".to_string(),
            ("python", scope) if let Some(group) = scope.strip_prefix("group:") => {
                format!("python dependency group: {group}")
            }
            ("python", scope) if let Some(extra) = scope.strip_prefix("extra:") => {
                format!("python optional dependency: {extra}")
            }
            ("node", "dependencies") => "node runtime dependency".to_string(),
            ("node", "devDependencies") => "node development dependency".to_string(),
            ("node", "optionalDependencies") => "node optional dependency".to_string(),
            (ecosystem, scope) => format!("{ecosystem} {scope} dependency"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NamedSelection {
    #[default]
    All,
    Include(BTreeSet<String>),
    Exclude(BTreeSet<String>),
}

impl NamedSelection {
    pub fn new(include: Option<Vec<String>>, exclude: Option<Vec<String>>) -> Result<Self> {
        match (include, exclude) {
            (Some(_), Some(_)) => bail!("Include and exclude filters cannot be combined"),
            (Some(include), None) => Ok(Self::Include(
                include
                    .into_iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect(),
            )),
            (None, Some(exclude)) => Ok(Self::Exclude(
                exclude
                    .into_iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect(),
            )),
            (None, None) => Ok(Self::All),
        }
    }

    fn includes(&self, name: &str) -> bool {
        match self {
            Self::All => true,
            Self::Include(include) => include.contains(name),
            Self::Exclude(exclude) => !exclude.contains(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRequirementData {
    pub spec: String,
    pub origin: ProjectDependencyOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanDependencySelection {
    pub include_project_dependencies: bool,
    pub dependency_groups: NamedSelection,
    pub optional_dependencies: NamedSelection,
    pub include_node_dependencies: bool,
    pub include_node_dev_dependencies: bool,
    pub include_node_optional_dependencies: bool,
}

impl Default for ScanDependencySelection {
    fn default() -> Self {
        Self {
            include_project_dependencies: true,
            dependency_groups: NamedSelection::All,
            optional_dependencies: NamedSelection::All,
            include_node_dependencies: true,
            include_node_dev_dependencies: true,
            include_node_optional_dependencies: true,
        }
    }
}

impl ScanDependencySelection {
    fn includes(&self, origin: &ProjectDependencyOrigin) -> bool {
        match origin.ecosystem.as_str() {
            "python" => match origin.scope.as_str() {
                "project" => self.include_project_dependencies,
                scope if let Some(group) = scope.strip_prefix("group:") => {
                    self.dependency_groups.includes(group)
                }
                scope if let Some(extra) = scope.strip_prefix("extra:") => {
                    self.optional_dependencies.includes(extra)
                }
                _ => false,
            },
            "node" => match origin.scope.as_str() {
                "dependencies" => self.include_node_dependencies,
                "devDependencies" => self.include_node_dev_dependencies,
                "optionalDependencies" => self.include_node_optional_dependencies,
                _ => false,
            },
            _ => false,
        }
    }
}

impl SkillSourceMetadata {
    pub fn new(
        source: Option<&str>,
        package_name: Option<&str>,
        package_version: Option<&str>,
        package_ecosystem: Option<PackageEcosystem>,
    ) -> Self {
        Self {
            source: source.map(str::to_string),
            package_name: package_name.map(str::to_string),
            package_version: package_version.map(str::to_string),
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem,
        }
    }

    fn apply_missing_from_metadata(&mut self, metadata: &BTreeMap<String, String>) {
        if self.package_name.is_none() {
            self.package_name = metadata
                .get(SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY)
                .cloned();
        }
        if self.package_version.is_none() {
            self.package_version = metadata
                .get(SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY)
                .cloned();
        }
        if self.repository_provider.is_none() {
            self.repository_provider = metadata
                .get(SKILLY_REPOSITORY_PROVIDER_METADATA_KEY)
                .and_then(|value| match value.as_str() {
                    "github" => Some(RepositoryProvider::GitHub),
                    "bitbucket-cloud" => Some(RepositoryProvider::BitbucketCloud),
                    "bitbucket-data-center" => Some(RepositoryProvider::BitbucketDataCenter),
                    _ => None,
                });
        }
        if self.repository_url.is_none() {
            self.repository_url = metadata.get(SKILLY_REPOSITORY_URL_METADATA_KEY).cloned();
        }
        if self.repository_commit_sha.is_none() {
            self.repository_commit_sha = metadata
                .get(SKILLY_REPOSITORY_COMMIT_SHA_METADATA_KEY)
                .cloned();
        }
        if self.package_ecosystem.is_none() {
            self.package_ecosystem = infer_package_ecosystem(metadata);
        }
    }

    fn resolved_source(&self, metadata: &BTreeMap<String, String>) -> String {
        self.source
            .clone()
            .unwrap_or_else(|| infer_source(metadata))
    }

    fn insert_source_metadata(&self, metadata: &mut BTreeMap<String, String>) {
        if let Some(source) = self.source.as_deref().filter(|source| {
            matches!(
                *source,
                SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_REPOSITORY | SKILLY_UNKNOWN_SOURCE
            )
        }) {
            metadata.insert(SKILLY_SOURCE_METADATA_KEY.to_string(), source.to_string());
        }
        if let Some(package_name) = self.package_name.as_ref() {
            metadata.insert(
                SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY.to_string(),
                package_name.clone(),
            );
        }
        if let Some(package_version) = self.package_version.as_ref() {
            metadata.insert(
                SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY.to_string(),
                package_version.clone(),
            );
        }
        if let Some(package_ecosystem) = self.package_ecosystem.as_ref() {
            metadata.insert(
                SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY.to_string(),
                package_ecosystem.as_str().to_string(),
            );
        }
        if let Some(provider) = self.repository_provider {
            metadata.insert(
                SKILLY_REPOSITORY_PROVIDER_METADATA_KEY.to_string(),
                provider.as_str().to_string(),
            );
        }
        if let Some(repository_url) = self.repository_url.as_ref() {
            metadata.insert(
                SKILLY_REPOSITORY_URL_METADATA_KEY.to_string(),
                repository_url.clone(),
            );
        }
        if let Some(repository_commit_sha) = self.repository_commit_sha.as_ref() {
            metadata.insert(
                SKILLY_REPOSITORY_COMMIT_SHA_METADATA_KEY.to_string(),
                repository_commit_sha.clone(),
            );
        }
    }
}

/// Environment paths and dependency selection for a project scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEnvironment {
    pub directory: PathBuf,
    pub sources: Vec<ProjectSource>,
}

impl Default for ProjectEnvironment {
    fn default() -> Self {
        Self {
            directory: PathBuf::from(DEFAULT_SKILLS_PATH),
            sources: vec![
                ProjectSource::Python(PythonSourceSettings::default()),
                ProjectSource::Node(NodeSourceSettings::default()),
                ProjectSource::Maven(MavenSourceSettings::default()),
            ],
        }
    }
}

impl ProjectEnvironment {}

/// Built-in source of project dependencies that may provide skills.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectSource {
    Python(PythonSourceSettings),
    Node(NodeSourceSettings),
    Maven(MavenSourceSettings),
}

/// Configuration for scanning a Python project for dependency-provided skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonSourceSettings {
    pub pyproject_toml_path: PathBuf,
    pub venv_path: PathBuf,
    pub include_project_dependencies: bool,
    pub dependency_groups: NamedSelection,
    pub optional_dependencies: NamedSelection,
}

impl Default for PythonSourceSettings {
    fn default() -> Self {
        Self {
            pyproject_toml_path: PathBuf::from("pyproject.toml"),
            venv_path: PathBuf::from(".venv"),
            include_project_dependencies: true,
            dependency_groups: NamedSelection::All,
            optional_dependencies: NamedSelection::All,
        }
    }
}

/// Configuration for scanning a Node.js project for dependency-provided skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSourceSettings {
    pub package_json_path: PathBuf,
    pub node_modules_path: PathBuf,
    pub include_dependencies: bool,
    pub include_dev_dependencies: bool,
    pub include_optional_dependencies: bool,
}

impl Default for NodeSourceSettings {
    fn default() -> Self {
        Self {
            package_json_path: PathBuf::from("package.json"),
            node_modules_path: PathBuf::from("node_modules"),
            include_dependencies: true,
            include_dev_dependencies: true,
            include_optional_dependencies: true,
        }
    }
}

/// Configuration for scanning a Maven project for dependency-provided skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenSourceSettings {
    pub pom_xml_path: PathBuf,
    pub repository_path: PathBuf,
    pub include_compile_scope: bool,
    pub include_runtime_scope: bool,
    pub include_provided_scope: bool,
    pub include_test_scope: bool,
    pub include_system_scope: bool,
}

impl Default for MavenSourceSettings {
    fn default() -> Self {
        Self {
            pom_xml_path: PathBuf::from("pom.xml"),
            repository_path: expand_home_path(Path::new("~/.m2/repository"))
                .unwrap_or_else(|_| PathBuf::from("~/.m2/repository")),
            include_compile_scope: true,
            include_runtime_scope: true,
            include_provided_scope: false,
            include_test_scope: true,
            include_system_scope: false,
        }
    }
}

/// Distribution metadata (name + optional version) read from a `.dist-info` directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionInfo {
    pub name: String,
    pub version: Option<String>,
}

/// Fetch a full snapshot (files + metadata) from a GitHub repository location.
#[allow(dead_code)]
pub trait GitHubSnapshotFetcher {
    fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData>;
}

/// Fetch a repository snapshot for a validated provider location.
pub trait RepositorySnapshotFetcher {
    /// Return all files beneath the requested repository revision.
    fn fetch_repository_snapshot(
        &self,
        location: &RepositoryLocationData,
    ) -> Result<RepositorySnapshotData>;
}

/// Maximum file size for binary reads (100 MB).
pub const MAX_BINARY_READ_SIZE: u64 = 100 * 1024 * 1024;

/// Abstract filesystem for pluggable backends (native, in-memory, remote).
///
/// Byte-level primitives. Use [`read_text`](FileSystem::read_text) and
/// [`write_text`](FileSystem::write_text) when working with UTF-8 content
/// such as SKILL.md, POM files, JSON, and TOML.
pub trait FileSystem {
    fn read_bytes(&self, path: &Path, max_size: Option<u64>) -> Result<Vec<u8>>;
    fn write_bytes(&self, path: &Path, content: &[u8]) -> Result<()>;
    fn list_files(&self, path: &Path) -> Result<Vec<String>>;
    fn exists(&self, path: &Path) -> Result<bool>;
    fn is_dir(&self, path: &Path) -> Result<bool>;
    fn make_dir(&self, path: &Path, parents: bool, exist_ok: bool) -> Result<()>;
    fn remove_tree(&self, path: &Path) -> Result<()>;
    fn replace_tree(&self, path: &Path, replacement: &Path) -> Result<()>;
    fn resolve(&self, path: &Path) -> Result<PathBuf>;

    /// Read a file as UTF-8 text, bounded by [`MAX_BINARY_READ_SIZE`].
    fn read_text(&self, path: &Path) -> Result<String> {
        let bytes = self.read_bytes(path, Some(MAX_BINARY_READ_SIZE))?;
        String::from_utf8(bytes)
            .map_err(|error| anyhow!("invalid UTF-8 in {}: {error}", path.display()))
    }

    /// Write UTF-8 text to a file.
    #[allow(dead_code)]
    fn write_text(&self, path: &Path, content: &str) -> Result<()> {
        self.write_bytes(path, content.as_bytes())
    }
}

/// Native (std::fs) filesystem implementation.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeFileSystem;

const NATIVE_FILE_SYSTEM: NativeFileSystem = NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn read_bytes(&self, path: &Path, max_size: Option<u64>) -> Result<Vec<u8>> {
        let limit = max_size.unwrap_or(MAX_BINARY_READ_SIZE);
        let metadata = fs::metadata(path)?;
        let file_size = metadata.len();
        if file_size > limit {
            bail!(
                "file size {file_size} exceeds maximum {limit} bytes: {}",
                path.display()
            );
        }
        fs::read(path).map_err(Into::into)
    }

    fn write_bytes(&self, path: &Path, content: &[u8]) -> Result<()> {
        fs::write(path, content)?;
        Ok(())
    }

    fn list_files(&self, path: &Path) -> Result<Vec<String>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(path)? {
            match entry {
                Ok(entry) => {
                    files.push(entry.file_name().to_string_lossy().to_string());
                }
                Err(_) => continue,
            }
        }
        Ok(files)
    }

    fn exists(&self, path: &Path) -> Result<bool> {
        Ok(path.exists())
    }

    fn is_dir(&self, path: &Path) -> Result<bool> {
        Ok(path.is_dir())
    }

    fn make_dir(&self, path: &Path, parents: bool, exist_ok: bool) -> Result<()> {
        if parents {
            if exist_ok {
                fs::create_dir_all(path)?;
                return Ok(());
            }
            if path.exists() {
                bail!("file exists: {}", path.display());
            }
            fs::create_dir_all(path)?;
            return Ok(());
        }

        match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if exist_ok && error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn remove_tree(&self, path: &Path) -> Result<()> {
        fs::remove_dir_all(path)?;
        Ok(())
    }

    fn replace_tree(&self, path: &Path, replacement: &Path) -> Result<()> {
        replace_tree(path, replacement)
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf> {
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        Ok(env::current_dir()?.join(path))
    }
}

fn default_unknown_source() -> String {
    SKILLY_UNKNOWN_SOURCE.to_string()
}

fn skill_directory_from_resolved_path(resolved: PathBuf) -> Option<PathBuf> {
    let Some(name) = resolved.file_name().and_then(|value| value.to_str()) else {
        return Some(resolved);
    };
    if name.eq_ignore_ascii_case("SKILL.md") {
        return resolved.parent().map(Path::to_path_buf);
    }
    Some(resolved)
}

fn resolve_path_in(file_system: &dyn FileSystem, path: &Path) -> Result<PathBuf> {
    file_system.resolve(path)
}

fn normalize_skill_directory_in(
    file_system: &dyn FileSystem,
    path: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let Some(path) = path else {
        return Ok(None);
    };
    Ok(skill_directory_from_resolved_path(resolve_path_in(
        file_system,
        path,
    )?))
}

fn split_frontmatter(text: &str) -> Result<(Vec<String>, String)> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|line| line.trim()) != Some("---") {
        bail!("missing YAML frontmatter");
    }

    for index in 1..lines.len() {
        if lines[index].trim() == "---" {
            let frontmatter = lines[1..index]
                .iter()
                .map(|line| (*line).to_string())
                .collect::<Vec<_>>();
            let body = lines[index + 1..].join("\n");
            return Ok((frontmatter, body));
        }
    }

    bail!("unterminated YAML frontmatter");
}

fn parse_yaml_frontmatter(text: &str) -> Result<Mapping> {
    let parsed: YamlValue =
        serde_yaml::from_str(text).map_err(|error| anyhow!("invalid YAML frontmatter: {error}"))?;
    match parsed {
        YamlValue::Null => Ok(Mapping::new()),
        YamlValue::Mapping(mapping) => Ok(mapping),
        _ => bail!("frontmatter must be a mapping"),
    }
}

fn should_quote_relaxed_yaml_scalar(value: &str) -> bool {
    !matches!(
        value.chars().next(),
        None | Some('|') | Some('>') | Some('"') | Some('\'') | Some('[') | Some('{')
    )
}

fn relax_frontmatter_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| {
            let Some((prefix, suffix)) = line.split_once(':') else {
                return line.clone();
            };
            let trimmed_value = suffix.trim_start();
            if trimmed_value.is_empty() || !should_quote_relaxed_yaml_scalar(trimmed_value) {
                return line.clone();
            }
            let quoted = format_scalar(trimmed_value);
            if quoted == trimmed_value {
                return line.clone();
            }
            let indentation = suffix.len() - trimmed_value.len();
            format!("{prefix}:{}{}", " ".repeat(indentation), quoted)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_frontmatter(lines: &[String]) -> Result<Mapping> {
    let frontmatter = lines.join("\n");
    match parse_yaml_frontmatter(&frontmatter) {
        Ok(mapping) => Ok(mapping),
        Err(error) => {
            let relaxed_frontmatter = relax_frontmatter_lines(lines);
            if relaxed_frontmatter == frontmatter {
                return Err(error);
            }
            parse_yaml_frontmatter(&relaxed_frontmatter).map_err(|_| error)
        }
    }
}

fn yaml_scalar_to_string(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::String(text) => Some(text.trim_end_matches('\n').to_string()),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        YamlValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn mapping_get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a YamlValue> {
    mapping.get(YamlValue::String(key.to_string()))
}

fn required_string_field(mapping: &Mapping, key: &str) -> Result<String> {
    yaml_scalar_to_string(
        mapping_get(mapping, key).ok_or_else(|| anyhow!("{key} must be a string"))?,
    )
    .ok_or_else(|| anyhow!("{key} must be a string"))
}

#[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
fn required_bundle_string_field(
    mapping: &Mapping,
    key: &'static str,
) -> std::result::Result<String, BundleValidationError> {
    let value = mapping_get(mapping, key).and_then(yaml_scalar_to_string);
    value.ok_or_else(|| {
        BundleValidationError::new(
            BundleValidationCode::InvalidField,
            "SKILL.md",
            Some(key),
            format!("{key} must be a string"),
        )
    })
}

fn optional_string_field(mapping: &Mapping, key: &str) -> Option<String> {
    mapping_get(mapping, key).and_then(yaml_scalar_to_string)
}

fn frontmatter_metadata(parsed: &Mapping) -> BTreeMap<String, String> {
    match mapping_get(parsed, "metadata") {
        Some(YamlValue::Mapping(mapping)) => mapping
            .iter()
            .filter_map(|(key, value)| {
                Some((yaml_scalar_to_string(key)?, yaml_scalar_to_string(value)?))
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn infer_source(metadata: &BTreeMap<String, String>) -> String {
    if let Some(source) = metadata.get(SKILLY_SOURCE_METADATA_KEY)
        && matches!(
            source.as_str(),
            SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_REPOSITORY | SKILLY_UNKNOWN_SOURCE
        )
    {
        return source.clone();
    }
    SKILLY_UNKNOWN_SOURCE.to_string()
}

fn has_managed_metadata(metadata: &BTreeMap<String, String>) -> bool {
    metadata.contains_key(SKILLY_SOURCE_METADATA_KEY)
        || metadata.contains_key(SKILLY_REPOSITORY_PROVIDER_METADATA_KEY)
        || metadata.contains_key(SKILLY_REPOSITORY_URL_METADATA_KEY)
        || metadata.contains_key(SKILLY_REPOSITORY_COMMIT_SHA_METADATA_KEY)
        || metadata.contains_key(SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY)
        || metadata.contains_key(SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY)
        || metadata.contains_key(SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY)
}

fn infer_package_ecosystem(metadata: &BTreeMap<String, String>) -> Option<PackageEcosystem> {
    if let Some(eco) = metadata.get(SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY) {
        return Some(PackageEcosystem::new(eco.clone()));
    }
    None
}

#[must_use]
#[inline]
pub fn classify_resource_kind(relative_path: &str) -> String {
    match relative_path.split('/').next() {
        Some("scripts") => RESOURCE_KIND_SCRIPT.to_string(),
        Some("references") => RESOURCE_KIND_REFERENCE.to_string(),
        Some("assets") => RESOURCE_KIND_ASSET.to_string(),
        _ => RESOURCE_KIND_OTHER.to_string(),
    }
}

fn collect_resource_files_in(
    file_system: &dyn FileSystem,
    skill_directory: &Path,
    current_directory: &Path,
    resources: &mut Vec<SkillResourceData>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut children = file_system.list_files(current_directory)?;
    children.sort();
    for child_name in children {
        let child_path = current_directory.join(&child_name);
        if file_system.is_dir(&child_path)? {
            collect_resource_files_in(
                file_system,
                skill_directory,
                &child_path,
                resources,
                warnings,
            )?;
            continue;
        }
        let Ok(relative_path) = child_path.strip_prefix(skill_directory) else {
            warnings.push(format!(
                "path {} is not inside skill directory",
                child_path.display()
            ));
            continue;
        };
        let relative_path = relative_path
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if relative_path.eq_ignore_ascii_case("SKILL.md") {
            continue;
        }
        match file_system.read_bytes(&child_path, Some(MAX_BINARY_READ_SIZE)) {
            Ok(content) => {
                let kind = classify_resource_kind(&relative_path);
                resources.push(SkillResourceData {
                    relative_path,
                    kind,
                    raw: content,
                });
            }
            Err(error) => warnings.push(format!(
                "{}: could not read bundled resource ({error})",
                child_path.display()
            )),
        }
    }
    Ok(())
}

fn load_resource_files_in(
    file_system: &dyn FileSystem,
    skill_directory: &Path,
) -> (Vec<SkillResourceData>, Vec<String>) {
    if !matches!(file_system.is_dir(skill_directory), Ok(true)) {
        return (Vec::new(), Vec::new());
    }

    let mut resources = Vec::new();
    let mut warnings = Vec::new();
    if let Err(error) = collect_resource_files_in(
        file_system,
        skill_directory,
        skill_directory,
        &mut resources,
        &mut warnings,
    ) {
        warnings.push(format!(
            "{}: could not enumerate bundled resources ({error})",
            skill_directory.display()
        ));
    }
    resources.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    warnings.sort();
    (resources, warnings)
}

fn format_scalar(value: &str) -> String {
    if value.is_empty()
        || value.trim() != value
        || value.contains(": ")
        || value.contains('#')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('"')
        || value.starts_with('@')
    {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    } else {
        value.to_string()
    }
}

fn write_file_in(
    file_system: &dyn FileSystem,
    path: &Path,
    content: &[u8],
    overwrite: bool,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        file_system.make_dir(parent, true, true)?;
    }
    if file_system.exists(path)? && !overwrite {
        bail!("refusing to overwrite existing file: {}", path.display());
    }
    file_system.write_bytes(path, content)?;
    Ok(())
}

fn write_text_file_in(
    file_system: &dyn FileSystem,
    path: &Path,
    content: &str,
    overwrite: bool,
) -> Result<()> {
    write_file_in(file_system, path, content.as_bytes(), overwrite)
}

fn validate_skill_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-');
    if valid {
        Ok(())
    } else {
        bail!(
            "invalid skill name {name:?}: use 1-64 lowercase letters, numbers, and single hyphens"
        )
    }
}

fn validate_resource_path(path: &str) -> Result<()> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !path.eq_ignore_ascii_case("SKILL.md");
    if valid {
        Ok(())
    } else {
        bail!("invalid relative resource path: {path}")
    }
}

fn validate_install_paths(skill: &SkillData, skill_name: Option<&str>) -> Result<()> {
    skill.validate()?;
    validate_skill_name(skill_name.unwrap_or(&skill.name))?;
    let mut seen = BTreeSet::new();
    for resource in &skill.resources {
        validate_resource_path(&resource.relative_path)?;
        if !seen.insert(resource.relative_path.to_ascii_lowercase()) {
            bail!("duplicate resource path: {}", resource.relative_path);
        }
    }
    Ok(())
}

fn temporary_sibling(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("skill destination has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("skill destination has no valid name: {}", path.display()))?;
    let id = TEMPORARY_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(".{name}.skilly-tmp-{}-{id}", std::process::id())))
}

fn replace_tree(path: &Path, replacement: &Path) -> Result<()> {
    if !path.exists() {
        fs::rename(replacement, path)?;
        return Ok(());
    }

    let backup = temporary_sibling(path)?;
    fs::rename(path, &backup)?;
    if let Err(error) = fs::rename(replacement, path) {
        if let Err(backup_error) = fs::rename(&backup, path) {
            eprintln!(
                "skilly: failed to restore backup during replacement (original at {}): {backup_error}",
                backup.display()
            );
        }
        return Err(error.into());
    }
    fs::remove_dir_all(backup)?;
    Ok(())
}

fn find_skill_markdown_path_in(file_system: &dyn FileSystem, path: &Path) -> Result<PathBuf> {
    let directory = resolve_path_in(file_system, path)?;
    if !file_system.is_dir(&directory)? {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
    }
    let mut children = file_system.list_files(&directory)?;
    children.sort();
    for child_name in children {
        if !child_name.eq_ignore_ascii_case("SKILL.md") {
            continue;
        }
        let child_path = directory.join(&child_name);
        if !file_system.is_dir(&child_path)? {
            return Ok(child_path);
        }
    }
    Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
}

impl SkillData {
    /// Validate skill name, description length, and compatibility length.
    pub fn validate(&self) -> Result<()> {
        validate_skill_name(&self.name)?;
        if self.description.is_empty() || self.description.len() > MAX_DESCRIPTION_LENGTH {
            bail!("skill description must contain 1-{MAX_DESCRIPTION_LENGTH} characters");
        }
        if self
            .compatibility
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > MAX_COMPATIBILITY_LENGTH)
        {
            bail!(
                "skill compatibility must contain 1-{MAX_COMPATIBILITY_LENGTH} characters when provided"
            );
        }
        Ok(())
    }

    fn write_to_root_in(
        &self,
        file_system: &dyn FileSystem,
        root: &Path,
        overwrite: bool,
    ) -> Result<Self> {
        file_system.make_dir(root, true, true)?;
        write_text_file_in(
            file_system,
            &root.join("SKILL.md"),
            &self.render(Some(&self.managed_metadata())),
            overwrite,
        )?;
        for resource in &self.resources {
            let destination = root.join(PathBuf::from(&resource.relative_path));
            write_file_in(file_system, &destination, &resource.raw, overwrite)?;
        }
        Self::from_dir_with_source_metadata_in(file_system, root, &SkillSourceMetadata::default())
    }

    fn from_text_parts(
        text: &str,
        skill_directory: Option<PathBuf>,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let (frontmatter, body) = split_frontmatter(text)?;
        let parsed = parse_frontmatter(&frontmatter)?;
        let name = required_string_field(&parsed, "name")?;
        let description = required_string_field(&parsed, "description")?;
        Ok(Self::from_parsed_parts(
            &parsed,
            name,
            description,
            body,
            text.as_bytes().to_vec(),
            skill_directory,
            source_metadata,
        ))
    }

    fn from_parsed_parts(
        parsed: &Mapping,
        name: String,
        description: String,
        body: String,
        raw: Vec<u8>,
        skill_directory: Option<PathBuf>,
        source_metadata: &SkillSourceMetadata,
    ) -> Self {
        let metadata = frontmatter_metadata(parsed);
        let mut source_metadata = source_metadata.clone();
        source_metadata.apply_missing_from_metadata(&metadata);
        let source = source_metadata.resolved_source(&metadata);
        let package_ecosystem = source_metadata.package_ecosystem.or_else(|| {
            if source == SKILLY_SOURCE_DEPENDENCY {
                Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON))
            } else {
                None
            }
        });

        Self {
            name,
            description,
            path: skill_directory
                .as_ref()
                .map(|value| value.to_string_lossy().to_string()),
            body,
            raw,
            license: optional_string_field(parsed, "license"),
            compatibility: optional_string_field(parsed, "compatibility"),
            metadata,
            allowed_tools: optional_string_field(parsed, "allowed-tools"),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source,
            package_name: source_metadata.package_name.clone(),
            package_version: source_metadata.package_version.clone(),
            repository_provider: source_metadata.repository_provider,
            repository_url: source_metadata.repository_url.clone(),
            repository_commit_sha: source_metadata.repository_commit_sha.clone(),
            package_ecosystem,
        }
    }

    /// Parse a skill from text content, with optional filesystem for resource discovery.
    pub fn from_text_in(
        file_system: &dyn FileSystem,
        text: &str,
        path: Option<&Path>,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let skill_directory = normalize_skill_directory_in(file_system, path)?;
        let mut skill = Self::from_text_parts(text, skill_directory.clone(), source_metadata)?;

        if let Some(directory) = skill_directory.as_ref() {
            let (resources, warnings) = load_resource_files_in(file_system, directory);
            skill.resources = resources;
            skill.resource_warnings = warnings;
        }

        Ok(skill)
    }

    /// Parse a skill from text content via the native filesystem.
    pub fn from_text(
        text: &str,
        path: Option<&Path>,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_text_in(&NATIVE_FILE_SYSTEM, text, path, source_metadata)
    }

    /// Load a complete skill bundle from its in-memory files without filesystem access.
    ///
    /// # Errors
    ///
    /// Returns [`BundleValidationError`] when `SKILL.md` is not valid UTF-8,
    /// its frontmatter does not satisfy the supported skill rules, or a bundled
    /// resource path is unsafe or duplicates another resource path.
    #[cfg_attr(not(any(test, feature = "python-bindings")), allow(dead_code))]
    pub fn from_bundle(
        skill_markdown: &[u8],
        resources: Vec<SkillResourceData>,
    ) -> std::result::Result<Self, BundleValidationError> {
        let text = std::str::from_utf8(skill_markdown).map_err(|error| {
            BundleValidationError::new(
                BundleValidationCode::InvalidUtf8,
                "SKILL.md",
                None,
                format!("SKILL.md must be valid UTF-8: {error}"),
            )
        })?;
        let (frontmatter, body) = split_frontmatter(text).map_err(|error| {
            BundleValidationError::new(
                BundleValidationCode::InvalidFrontmatter,
                "SKILL.md",
                None,
                error.to_string(),
            )
        })?;
        let parsed = parse_frontmatter(&frontmatter).map_err(|error| {
            BundleValidationError::new(
                BundleValidationCode::InvalidFrontmatter,
                "SKILL.md",
                None,
                error.to_string(),
            )
        })?;
        let name = required_bundle_string_field(&parsed, "name")?;
        let description = required_bundle_string_field(&parsed, "description")?;
        let mut skill = Self::from_parsed_parts(
            &parsed,
            name,
            description,
            body,
            skill_markdown.to_vec(),
            None,
            &SkillSourceMetadata::default(),
        );

        if let Err(error) = validate_skill_name(&skill.name) {
            return Err(BundleValidationError::new(
                BundleValidationCode::InvalidField,
                "SKILL.md",
                Some("name"),
                error.to_string(),
            ));
        }
        if skill.description.is_empty() || skill.description.len() > MAX_DESCRIPTION_LENGTH {
            return Err(BundleValidationError::new(
                BundleValidationCode::InvalidField,
                "SKILL.md",
                Some("description"),
                format!("skill description must contain 1-{MAX_DESCRIPTION_LENGTH} characters"),
            ));
        }
        if skill
            .compatibility
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > MAX_COMPATIBILITY_LENGTH)
        {
            return Err(BundleValidationError::new(
                BundleValidationCode::InvalidField,
                "SKILL.md",
                Some("compatibility"),
                format!(
                    "skill compatibility must contain 1-{MAX_COMPATIBILITY_LENGTH} characters when provided"
                ),
            ));
        }

        let mut paths = BTreeSet::new();
        for resource in &resources {
            if let Err(error) = validate_resource_path(&resource.relative_path) {
                return Err(BundleValidationError::new(
                    BundleValidationCode::InvalidResourcePath,
                    &resource.relative_path,
                    None,
                    error.to_string(),
                ));
            }
            if !paths.insert(resource.relative_path.to_ascii_lowercase()) {
                return Err(BundleValidationError::new(
                    BundleValidationCode::DuplicateResourcePath,
                    &resource.relative_path,
                    None,
                    format!("duplicate resource path: {}", resource.relative_path),
                ));
            }
        }

        skill.resources = resources;
        Ok(skill)
    }

    /// Load a skill from a SKILL.md file via the native filesystem.
    #[cfg(feature = "python-bindings")]
    #[allow(dead_code)]
    pub fn from_file_with_source_metadata(
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_file_with_source_metadata_in(&NATIVE_FILE_SYSTEM, path, source_metadata)
    }

    /// Load a skill from a SKILL.md file, discovering bundled resources.
    pub fn from_file_with_source_metadata_in(
        file_system: &dyn FileSystem,
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let text = file_system.read_text(path)?;
        Self::from_text_in(file_system, &text, Some(path), source_metadata)
    }

    /// Discover a SKILL.md inside a directory and load the skill.
    pub fn from_dir_with_source_metadata(
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_dir_with_source_metadata_in(&NATIVE_FILE_SYSTEM, path, source_metadata)
    }

    /// Discover a SKILL.md inside a directory and load the skill via a custom filesystem.
    pub fn from_dir_with_source_metadata_in(
        file_system: &dyn FileSystem,
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let skill_path = find_skill_markdown_path_in(file_system, path)?;
        Self::from_file_with_source_metadata_in(file_system, &skill_path, source_metadata)
    }

    /// Render the skill as a SKILL.md string with frontmatter and body.
    #[must_use]
    pub fn render(&self, metadata_override: Option<&BTreeMap<String, String>>) -> String {
        let combined_metadata = match metadata_override {
            Some(overrides) if !overrides.is_empty() => {
                let mut combined = self.metadata.clone();
                for (key, value) in overrides {
                    combined.insert(key.clone(), value.clone());
                }
                combined
            }
            _ => self.metadata.clone(),
        };

        let frontmatter = self.build_frontmatter_lines(&combined_metadata);
        let total_estimate =
            frontmatter.iter().map(|s| s.len() + 1).sum::<usize>() + self.body.len() + 8;
        let mut output = String::with_capacity(total_estimate);
        output.push_str("---\n");
        for line in &frontmatter {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("---\n");
        if !self.body.is_empty() {
            output.push_str(&self.body);
        }
        output
    }

    fn build_frontmatter_lines(&self, combined_metadata: &BTreeMap<String, String>) -> Vec<String> {
        let mut lines = vec![
            format!("name: {}", format_scalar(&self.name)),
            format!("description: {}", format_scalar(&self.description)),
        ];
        if let Some(license) = self.license.as_ref() {
            lines.push(format!("license: {}", format_scalar(license)));
        }
        if let Some(compatibility) = self.compatibility.as_ref() {
            lines.push(format!("compatibility: {}", format_scalar(compatibility)));
        }
        if let Some(allowed_tools) = self.allowed_tools.as_ref() {
            lines.push(format!("allowed-tools: {}", format_scalar(allowed_tools)));
        }
        if !combined_metadata.is_empty() {
            lines.push("metadata:".to_string());
            for (key, value) in combined_metadata {
                lines.push(format!("  {key}: {}", format_scalar(value)));
            }
        }
        lines
    }

    /// Write the skill to disk (native filesystem), returning the installed skill.
    pub fn install_to(
        &self,
        directory: &Path,
        skill_name: Option<&str>,
        overwrite: bool,
    ) -> Result<Self> {
        self.install_to_in(&NATIVE_FILE_SYSTEM, directory, skill_name, overwrite)
    }

    /// Atomically replace an existing skill on disk, via the native filesystem.
    pub fn replace_to(&self, directory: &Path, skill_name: Option<&str>) -> Result<Self> {
        self.replace_to_in(&NATIVE_FILE_SYSTEM, directory, skill_name)
    }

    /// Write the skill to disk via a custom filesystem, returning the installed skill.
    pub fn install_to_in(
        &self,
        file_system: &dyn FileSystem,
        directory: &Path,
        skill_name: Option<&str>,
        overwrite: bool,
    ) -> Result<Self> {
        validate_install_paths(self, skill_name)?;
        let root = resolve_path_in(
            file_system,
            &directory.join(skill_name.unwrap_or(&self.name)),
        )?;
        self.write_to_root_in(file_system, &root, overwrite)
    }

    /// Atomically replace an existing skill on disk via a custom filesystem.
    pub fn replace_to_in(
        &self,
        file_system: &dyn FileSystem,
        directory: &Path,
        skill_name: Option<&str>,
    ) -> Result<Self> {
        validate_install_paths(self, skill_name)?;
        let root = resolve_path_in(
            file_system,
            &directory.join(skill_name.unwrap_or(&self.name)),
        )?;
        let replacement = temporary_sibling(&root)?;
        if let Err(error) = self.write_to_root_in(file_system, &replacement, false) {
            if matches!(file_system.exists(&replacement), Ok(true))
                && let Err(cleanup_error) = file_system.remove_tree(&replacement)
            {
                eprintln!(
                    "skilly: failed to clean up temporary replacement {}: {cleanup_error}",
                    replacement.display()
                );
            }
            return Err(error);
        }
        file_system.replace_tree(&root, &replacement)?;
        Self::from_dir_with_source_metadata_in(file_system, &root, &SkillSourceMetadata::default())
    }

    /// Build the full metadata map including persisted source tracking.
    #[must_use]
    pub fn managed_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = self.metadata.clone();
        self.source_metadata().insert_source_metadata(&mut metadata);
        metadata
    }

    /// Extract source provenance as a [`SkillSourceMetadata`] struct.
    #[must_use]
    pub fn source_metadata(&self) -> SkillSourceMetadata {
        SkillSourceMetadata {
            source: Some(self.source.clone()),
            package_name: self.package_name.clone(),
            package_version: self.package_version.clone(),
            repository_provider: self.repository_provider,
            repository_url: self.repository_url.clone(),
            repository_commit_sha: self.repository_commit_sha.clone(),
            package_ecosystem: self.package_ecosystem.clone(),
        }
    }

    /// Return the on-disk directory name for this skill.
    #[must_use]
    pub fn directory_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(|path| Path::new(path).file_name().and_then(|value| value.to_str()))
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.name)
            .to_string()
    }

    /// Return `name==version` if both package fields are set, otherwise just the name.
    #[must_use]
    pub fn package_reference(&self) -> Option<String> {
        match (
            &self.package_name,
            &self.package_version,
            self.package_ecosystem.as_ref(),
        ) {
            (Some(name), Some(version), Some(eco))
                if !version.is_empty() && eco.as_str() == PACKAGE_ECOSYSTEM_NODE =>
            {
                Some(format!("{name}@{version}"))
            }
            (Some(name), Some(version), Some(eco))
                if !version.is_empty() && eco.as_str() == PACKAGE_ECOSYSTEM_MAVEN =>
            {
                Some(format!("{name}:{version}"))
            }
            (Some(name), Some(version), _) if !version.is_empty() => {
                Some(format!("{name}=={version}"))
            }
            (Some(name), _, _) => Some(name.clone()),
            _ => None,
        }
    }

    /// Check whether two skills refer to the same logical skill, matching by
    /// package name, repository provenance, or name.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        if let (Some(package_name), Some(other_package_name)) =
            (&self.package_name, &other.package_name)
        {
            return (self.package_ecosystem.as_ref(), package_name, &self.name)
                == (
                    other.package_ecosystem.as_ref(),
                    other_package_name,
                    &other.name,
                );
        }
        if let (Some(provider), Some(url), Some(other_provider), Some(other_url)) = (
            self.repository_provider,
            self.repository_url.as_ref(),
            other.repository_provider,
            other.repository_url.as_ref(),
        ) {
            return (provider, url) == (other_provider, other_url);
        }
        self.name == other.name
    }

    /// Returns `true` when the skill carries persisted skilly provenance metadata.
    #[must_use]
    #[inline]
    pub fn is_installed(&self) -> bool {
        has_managed_metadata(&self.metadata)
    }

    /// Returns `true` when the skill source is a dependency install.
    #[must_use]
    #[inline]
    pub fn is_dependency(&self) -> bool {
        self.source == SKILLY_SOURCE_DEPENDENCY
    }

    /// Returns `true` when the skill has a known update source.
    #[cfg(feature = "python-bindings")]
    #[allow(dead_code)]
    #[must_use]
    #[inline]
    pub fn can_update(&self) -> bool {
        self.is_dependency() || self.repository_url.is_some()
    }
}

/// Return a status string for a scan match: `installed`, `installable`, or `updatable`.
#[must_use]
#[inline]
pub fn scan_match_status(available: &SkillData, installed: Option<&SkillData>) -> &'static str {
    match installed {
        None => STATUS_INSTALLABLE,
        Some(installed) if installed.package_version == available.package_version => {
            STATUS_INSTALLED
        }
        Some(_) => STATUS_UPDATABLE,
    }
}

#[cfg(feature = "python-bindings")]
#[allow(dead_code)]
pub fn discover_installed_skills(directory: &Path) -> Result<Vec<SkillData>> {
    discover_installed_skills_in(&NATIVE_FILE_SYSTEM, directory)
}

pub fn discover_installed_skills_in(
    file_system: &dyn FileSystem,
    directory: &Path,
) -> Result<Vec<SkillData>> {
    let root = resolve_path_in(file_system, directory)?;
    if !file_system.exists(&root)? {
        return Ok(Vec::new());
    }
    if !file_system.is_dir(&root)? {
        bail!("{}", root.display());
    }
    let mut skills = Vec::new();
    let mut children = file_system.list_files(&root)?;
    children.sort();
    for child_name in children {
        let child_path = root.join(&child_name);
        if !file_system.is_dir(&child_path)? {
            continue;
        }
        let skill = SkillData::from_dir_with_source_metadata_in(
            file_system,
            &child_path,
            &SkillSourceMetadata::default(),
        )
        .with_context(|| format!("Invalid installed skill: {}", child_path.display()))?;
        skills.push(skill);
    }
    Ok(skills)
}

pub fn remove_skill(name: &str, directory: &Path) -> Result<SkillData> {
    remove_skill_in(&NATIVE_FILE_SYSTEM, name, directory)
}

pub fn remove_skill_in(
    file_system: &dyn FileSystem,
    name: &str,
    directory: &Path,
) -> Result<SkillData> {
    let skill = require_installed_skill_in(file_system, name, directory)?;
    let skill_directory = skill
        .path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("installed skill has no directory: {name}"))?;
    file_system.remove_tree(&skill_directory)?;
    Ok(skill)
}

pub fn require_installed_skill_in(
    file_system: &dyn FileSystem,
    name: &str,
    directory: &Path,
) -> Result<SkillData> {
    let skills = discover_installed_skills_in(file_system, directory)?;
    for skill in &skills {
        if skill.directory_name() == name {
            return Ok(skill.clone());
        }
    }
    let matches = skills
        .into_iter()
        .filter(|skill| skill.name == name)
        .take(2)
        .collect::<Vec<_>>();
    match matches.len() {
        0 => bail!("installed skill not found: {name}"),
        1 => Ok(matches[0].clone()),
        _ => bail!("multiple installed skills match name: {name}"),
    }
}

pub fn find_site_packages_dir_in(
    file_system: &dyn FileSystem,
    venv_path: &Path,
) -> Result<Option<PathBuf>> {
    let windows_path = venv_path.join("Lib").join("site-packages");
    if file_system.is_dir(&windows_path)? {
        return Ok(Some(windows_path));
    }
    for lib_name in ["lib", "lib64"] {
        let lib_dir = venv_path.join(lib_name);
        if !file_system.is_dir(&lib_dir)? {
            continue;
        }
        let mut children = file_system.list_files(&lib_dir)?;
        children.sort();
        children.reverse();
        for child_name in children {
            let child_path = lib_dir.join(&child_name);
            let site_packages = child_path.join("site-packages");
            if file_system.is_dir(&child_path)?
                && child_name.starts_with("python")
                && file_system.is_dir(&site_packages)?
            {
                return Ok(Some(site_packages));
            }
        }
    }
    Ok(None)
}

pub fn list_dist_info_dirs_in(
    file_system: &dyn FileSystem,
    site_packages: &Path,
) -> Result<Vec<PathBuf>> {
    if !file_system.is_dir(site_packages)? {
        return Ok(Vec::new());
    }
    let mut dirs = file_system
        .list_files(site_packages)?
        .into_iter()
        .filter(|name| name.ends_with(".dist-info"))
        .map(|name| site_packages.join(name))
        .filter(|path| matches!(file_system.is_dir(path), Ok(true)))
        .collect::<Vec<_>>();
    dirs.sort();
    Ok(dirs)
}

pub fn read_distribution_info_in(
    file_system: &dyn FileSystem,
    dist_info: &Path,
) -> Result<Option<DistributionInfo>> {
    let text = match file_system.read_text(&dist_info.join("METADATA")) {
        Ok(text) => text,
        Err(_) => return Ok(None),
    };
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Version:") {
            version = Some(rest.trim().to_string());
        }
    }
    Ok(name.map(|name| DistributionInfo { name, version }))
}

/// Check whether an installed path (from a RECORD file) points to a skilly skill directory.
#[must_use]
#[inline]
pub fn is_skill_record(installed_path: &str) -> bool {
    let normalized = installed_path.replace('\\', "/");
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.last() != Some(&"SKILL.md") {
        return false;
    }
    // Reject paths that would escape the package root.
    if parts.contains(&".") || parts.contains(&"..") || parts.contains(&"") {
        return false;
    }
    for (index, part) in parts.iter().enumerate() {
        if *part == ".agents" && parts.len() > index + 3 && parts[index + 1] == "skills" {
            return true;
        }
        if *part == "skills" && parts.len() == index + 3 {
            return true;
        }
    }
    false
}

pub fn resolve_record_path(site_packages: &Path, installed_path: &str) -> PathBuf {
    installed_path
        .replace('\\', "/")
        .split('/')
        .fold(site_packages.to_path_buf(), |path, part| path.join(part))
}

fn sort_dependency_skills(skills: &mut [SkillData]) {
    skills.sort_by(|left, right| {
        (
            left.package_ecosystem.as_ref(),
            left.package_name.as_deref().unwrap_or(""),
            left.package_version.as_deref().unwrap_or(""),
            left.name.as_str(),
        )
            .cmp(&(
                right.package_ecosystem.as_ref(),
                right.package_name.as_deref().unwrap_or(""),
                right.package_version.as_deref().unwrap_or(""),
                right.name.as_str(),
            ))
    });
}

pub fn discover_venv_skills(path: &Path) -> Result<Vec<SkillData>> {
    discover_venv_skills_in(&NATIVE_FILE_SYSTEM, path)
}

pub fn discover_venv_skills_in(
    file_system: &dyn FileSystem,
    path: &Path,
) -> Result<Vec<SkillData>> {
    let venv_path = resolve_path_in(file_system, path)?;
    let Some(site_packages) = find_site_packages_dir_in(file_system, &venv_path)? else {
        return Ok(Vec::new());
    };

    let mut skills = Vec::new();
    let mut seen_directories = BTreeSet::new();
    for dist_info in list_dist_info_dirs_in(file_system, &site_packages)? {
        let Some(distribution) = read_distribution_info_in(file_system, &dist_info)? else {
            continue;
        };
        let Ok(record_text) = file_system.read_text(&dist_info.join("RECORD")) else {
            continue;
        };
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .from_reader(record_text.as_bytes());
        for row in reader.records().flatten() {
            let Some(installed_path) = row.get(0) else {
                continue;
            };
            if !is_skill_record(installed_path) {
                continue;
            }
            let skill_path = resolve_record_path(&site_packages, installed_path);
            let Some(directory) = skill_path.parent() else {
                continue;
            };
            if !seen_directories.insert(directory.to_path_buf()) {
                continue;
            }
            if let Ok(skill) = SkillData::from_file_with_source_metadata_in(
                file_system,
                &skill_path,
                &SkillSourceMetadata::new(
                    Some(SKILLY_SOURCE_DEPENDENCY),
                    Some(&distribution.name),
                    distribution.version.as_deref(),
                    Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
                ),
            ) {
                skills.push(skill);
            }
        }
    }

    sort_dependency_skills(&mut skills);
    Ok(skills)
}

fn collect_project_requirement_values(
    values: Option<&toml::Value>,
    origin: ProjectDependencyOrigin,
) -> Vec<ProjectRequirementData> {
    values
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    value.as_str().map(|spec| ProjectRequirementData {
                        spec: spec.to_string(),
                        origin: origin.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_project_requirement_entries(text: &str) -> Result<Vec<ProjectRequirementData>> {
    let parsed: toml::Value = text.parse()?;
    let mut dependencies = collect_project_requirement_values(
        parsed
            .get("project")
            .and_then(|value| value.get("dependencies")),
        ProjectDependencyOrigin::python_project(),
    );

    if let Some(groups) = parsed
        .get("dependency-groups")
        .and_then(|value| value.as_table())
    {
        for (group_name, values) in groups {
            dependencies.extend(collect_project_requirement_values(
                Some(values),
                ProjectDependencyOrigin::python_dependency_group(group_name),
            ));
        }
    }

    if let Some(extras) = parsed
        .get("project")
        .and_then(|value| value.get("optional-dependencies"))
        .and_then(|value| value.as_table())
    {
        for (extra_name, values) in extras {
            dependencies.extend(collect_project_requirement_values(
                Some(values),
                ProjectDependencyOrigin::python_optional_dependency(extra_name),
            ));
        }
    }

    Ok(dependencies)
}

fn select_project_requirement_entries(
    requirements: Vec<ProjectRequirementData>,
    include_dev: bool,
    include_extras: &[String],
) -> Vec<ProjectRequirementData> {
    let mut selected = include_extras.iter().cloned().collect::<BTreeSet<_>>();
    if include_dev {
        selected.insert("dev".to_string());
    }

    requirements
        .into_iter()
        .filter(|requirement| match &requirement.origin {
            origin if origin.ecosystem == PACKAGE_ECOSYSTEM_PYTHON && origin.scope == "project" => {
                true
            }
            origin
                if origin.ecosystem == PACKAGE_ECOSYSTEM_PYTHON
                    && let Some(group) = origin.scope.strip_prefix("group:") =>
            {
                selected.contains(group)
            }
            origin
                if origin.ecosystem == PACKAGE_ECOSYSTEM_PYTHON
                    && let Some(extra) = origin.scope.strip_prefix("extra:") =>
            {
                selected.contains(extra)
            }
            _ => false,
        })
        .collect()
}

fn filter_scan_requirement_entries(
    requirements: Vec<ProjectRequirementData>,
    selection: &ScanDependencySelection,
) -> Vec<ProjectRequirementData> {
    requirements
        .into_iter()
        .filter(|requirement| selection.includes(&requirement.origin))
        .collect()
}

fn package_dependency_origins(
    requirements: &[ProjectRequirementData],
) -> BTreeMap<String, Vec<ProjectDependencyOrigin>> {
    let mut origins_by_package = BTreeMap::<String, BTreeSet<ProjectDependencyOrigin>>::new();
    for requirement in requirements {
        let Some(package_name) = requirement_name(&requirement.spec) else {
            continue;
        };
        origins_by_package
            .entry(package_name)
            .or_default()
            .insert(requirement.origin.clone());
    }

    origins_by_package
        .into_iter()
        .map(|(package_name, origins)| (package_name, origins.into_iter().collect()))
        .collect()
}

fn parse_project_requirements(
    text: &str,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<String>> {
    Ok(select_project_requirement_entries(
        parse_project_requirement_entries(text)?,
        include_dev,
        include_extras,
    )
    .into_iter()
    .map(|requirement| requirement.spec)
    .collect())
}

fn scan_project_requirements_in(
    file_system: &dyn FileSystem,
    pyproject_toml_path: &Path,
    settings: &PythonSourceSettings,
) -> Result<Vec<ProjectRequirementData>> {
    let text = file_system.read_text(pyproject_toml_path)?;
    let selection = ScanDependencySelection {
        include_project_dependencies: settings.include_project_dependencies,
        dependency_groups: settings.dependency_groups.clone(),
        optional_dependencies: settings.optional_dependencies.clone(),
        include_node_dependencies: false,
        include_node_dev_dependencies: false,
        include_node_optional_dependencies: false,
    };
    Ok(filter_scan_requirement_entries(
        parse_project_requirement_entries(&text)?,
        &selection,
    ))
}

fn scan_node_requirements_in(
    file_system: &dyn FileSystem,
    package_json_path: &Path,
    settings: &NodeSourceSettings,
) -> Result<Vec<ProjectRequirementData>> {
    if !file_system.exists(package_json_path)? {
        return Ok(Vec::new());
    }
    let text = file_system.read_text(package_json_path)?;
    let selection = ScanDependencySelection {
        include_project_dependencies: false,
        dependency_groups: NamedSelection::All,
        optional_dependencies: NamedSelection::All,
        include_node_dependencies: settings.include_dependencies,
        include_node_dev_dependencies: settings.include_dev_dependencies,
        include_node_optional_dependencies: settings.include_optional_dependencies,
    };
    Ok(filter_scan_requirement_entries(
        parse_node_dependency_entries(&text)?,
        &selection,
    ))
}

pub fn project_requirements(
    pyproject_toml_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<String>> {
    project_requirements_in(
        &NATIVE_FILE_SYSTEM,
        pyproject_toml_path,
        include_dev,
        include_extras,
    )
}

pub fn project_requirements_in(
    file_system: &dyn FileSystem,
    pyproject_toml_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<String>> {
    let text = file_system.read_text(pyproject_toml_path)?;
    parse_project_requirements(&text, include_dev, include_extras)
}

/// Extract the package name from a pip or npm requirement spec.
#[must_use]
#[inline]
pub fn requirement_name(spec: &str) -> Option<String> {
    let trimmed = spec.trim_start();
    let mut name = String::new();
    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '@' | '/') {
            name.push(character);
        } else {
            break;
        }
    }
    if name.is_empty() { None } else { Some(name) }
}

fn validate_node_package_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains("..") || name.contains('\\') {
        bail!("invalid node package name: {name:?}");
    }
    let valid = name
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'));
    if !valid {
        bail!("invalid node package name: {name:?}");
    }
    Ok(())
}

fn parse_node_dependency_entries(text: &str) -> Result<Vec<ProjectRequirementData>> {
    let parsed: serde_json::Value =
        serde_json::from_str(text).map_err(|error| anyhow!("invalid package.json: {error}"))?;

    let mut dependencies = Vec::new();

    if let Some(deps) = parsed.get("dependencies").and_then(|v| v.as_object()) {
        for (name, _version) in deps {
            validate_node_package_name(name)?;
            dependencies.push(ProjectRequirementData {
                spec: name.to_string(),
                origin: ProjectDependencyOrigin::node_dependencies(),
            });
        }
    }

    if let Some(deps) = parsed.get("devDependencies").and_then(|v| v.as_object()) {
        for (name, _version) in deps {
            validate_node_package_name(name)?;
            dependencies.push(ProjectRequirementData {
                spec: name.to_string(),
                origin: ProjectDependencyOrigin::node_dev_dependencies(),
            });
        }
    }

    if let Some(deps) = parsed
        .get("optionalDependencies")
        .and_then(|v| v.as_object())
    {
        for (name, _version) in deps {
            validate_node_package_name(name)?;
            dependencies.push(ProjectRequirementData {
                spec: name.to_string(),
                origin: ProjectDependencyOrigin::node_optional_dependencies(),
            });
        }
    }

    Ok(dependencies)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodePackageInfo {
    name: String,
    version: String,
}

#[cfg(feature = "python-bindings")]
#[allow(dead_code)]
pub fn discover_node_modules_skills(node_modules_path: &Path) -> Result<Vec<SkillData>> {
    discover_node_modules_skills_in(&NATIVE_FILE_SYSTEM, node_modules_path)
}

pub fn discover_node_modules_skills_in(
    file_system: &dyn FileSystem,
    node_modules_path: &Path,
) -> Result<Vec<SkillData>> {
    let resolved = resolve_path_in(file_system, node_modules_path)?;
    if !file_system.exists(&resolved)? {
        return Ok(Vec::new());
    }
    if !file_system.is_dir(&resolved)? {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    let packages = file_system.list_files(&resolved)?;
    for package_dir in &packages {
        // Handle scoped packages: @scope directory contains child packages
        if package_dir.starts_with('@') {
            let scope_dir = resolved.join(package_dir);
            if !file_system.is_dir(&scope_dir).unwrap_or(false) {
                continue;
            }
            let scoped_packages = file_system.list_files(&scope_dir)?;
            for scoped_pkg in &scoped_packages {
                let package_path = scope_dir.join(scoped_pkg);
                if !file_system.is_dir(&package_path).unwrap_or(false) {
                    continue;
                }
                let full_name = format!("{package_dir}/{scoped_pkg}");
                if let Ok(Some(skills_found)) =
                    load_node_package_skills_in(file_system, &package_path, &full_name)
                {
                    skills.extend(skills_found);
                }
            }
            continue;
        }

        let package_path = resolved.join(package_dir);
        if !file_system.is_dir(&package_path).unwrap_or(false) {
            continue;
        }
        if let Ok(Some(skills_found)) =
            load_node_package_skills_in(file_system, &package_path, package_dir)
        {
            skills.extend(skills_found);
        }
    }

    sort_dependency_skills(&mut skills);
    Ok(skills)
}

fn load_node_package_skills_in(
    file_system: &dyn FileSystem,
    package_path: &Path,
    _full_name: &str,
) -> Result<Option<Vec<SkillData>>> {
    let package_json_path = package_path.join("package.json");
    let Some(package_info) = read_node_package_json_in(file_system, &package_json_path)? else {
        return Ok(None);
    };

    let source_metadata = SkillSourceMetadata::new(
        Some(SKILLY_SOURCE_DEPENDENCY),
        Some(&package_info.name),
        Some(&package_info.version),
        Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE)),
    );

    let (skills, warnings) =
        discover_package_skills_from_dir_in(file_system, package_path, &source_metadata);

    for warning in warnings {
        eprintln!("skilly: {warning}");
    }

    Ok(if skills.is_empty() {
        None
    } else {
        Some(skills)
    })
}

fn read_node_package_json_in(
    file_system: &dyn FileSystem,
    package_json_path: &Path,
) -> Result<Option<NodePackageInfo>> {
    if !file_system.exists(package_json_path)? {
        return Ok(None);
    }
    let text = file_system.read_text(package_json_path)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| anyhow!("invalid package.json: {error}"))?;
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");
    Ok(Some(NodePackageInfo {
        name: name.to_string(),
        version: version.to_string(),
    }))
}

fn discover_package_skills_from_dir_in(
    file_system: &dyn FileSystem,
    package_root: &Path,
    source_metadata: &SkillSourceMetadata,
) -> (Vec<SkillData>, Vec<String>) {
    let skill_layouts = [".agents/skills", "skills"];
    let mut warnings = Vec::new();
    let mut seen_names: BTreeMap<String, SkillData> = BTreeMap::new();
    let mut duplicate_names = BTreeSet::new();

    for layout_dir in skill_layouts {
        let layout_path = package_root.join(layout_dir);
        if !matches!(file_system.is_dir(&layout_path), Ok(true)) {
            continue;
        }

        let Ok(child_names) = file_system.list_files(&layout_path) else {
            continue;
        };

        for child_name in child_names {
            let skill_dir = layout_path.join(&child_name);
            if !matches!(file_system.is_dir(&skill_dir), Ok(true)) {
                continue;
            }

            let normalized_name = child_name.to_ascii_lowercase();

            match SkillData::from_dir_with_source_metadata_in(
                file_system,
                &skill_dir,
                source_metadata,
            ) {
                Ok(skill) => {
                    if let Some(_existing) = seen_names.get(&normalized_name) {
                        duplicate_names.insert(normalized_name.clone());
                    }
                    seen_names.insert(normalized_name, skill);
                }
                Err(error) => {
                    warnings.push(format!(
                        "{}: could not load skill ({error})",
                        skill_dir.display()
                    ));
                }
            }
        }
    }

    for dup in &duplicate_names {
        seen_names.remove(dup);
        warnings.push(format!(
            "ambiguous duplicate skill directory name {dup:?} found in package {:?}; both copies ignored",
            package_root.display()
        ));
    }

    let mut skills: Vec<_> = seen_names.into_values().collect();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    (skills, warnings)
}

/// Discover skills from Maven JAR artifacts in the local repository.
///
/// Reads the POM, resolves direct dependencies, locates each JAR, and
/// extracts skills from the archive. Returns discovered skills and any
/// non-fatal warnings.
pub fn discover_maven_skills_in(
    file_system: &dyn FileSystem,
    settings: &MavenSourceSettings,
) -> Result<(Vec<SkillData>, Vec<String>)> {
    let mut all_skills = Vec::new();
    let mut all_warnings = Vec::new();

    if !file_system.exists(&settings.pom_xml_path).unwrap_or(false) {
        return Ok((all_skills, all_warnings));
    }

    let pom_text = file_system.read_text(&settings.pom_xml_path)?;
    let (coordinates, pom_warnings) = parse_maven_dependencies(&pom_text)?;
    all_warnings.extend(pom_warnings);

    let selected: Vec<_> = coordinates
        .into_iter()
        .filter(|coord| {
            let scope = coord.scope.as_deref().unwrap_or("compile");
            match scope {
                "compile" => settings.include_compile_scope,
                "runtime" => settings.include_runtime_scope,
                "provided" => settings.include_provided_scope,
                "test" => settings.include_test_scope,
                "system" => settings.include_system_scope,
                _ => false,
            }
        })
        .collect();

    let mut seen_names = BTreeSet::new();
    for coord in &selected {
        let jar_path = maven_jar_path(&settings.repository_path, coord);
        if !file_system.exists(&jar_path).unwrap_or(false) {
            all_warnings.push(format!(
                "Maven artifact not found: {} (expected at {})",
                format_maven_ref(coord),
                jar_path.display()
            ));
            continue;
        }

        let jar_bytes = match file_system.read_bytes(&jar_path, Some(MAX_ARCHIVE_SIZE)) {
            Ok(bytes) => bytes,
            Err(error) => {
                all_warnings.push(format!(
                    "could not read Maven artifact {} ({error})",
                    jar_path.display()
                ));
                continue;
            }
        };

        let source_metadata = SkillSourceMetadata::new(
            Some(SKILLY_SOURCE_DEPENDENCY),
            Some(&format_maven_package_name(coord)),
            Some(&coord.version),
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_MAVEN)),
        );

        match load_skills_from_archive(&jar_bytes, &source_metadata) {
            Ok((skills, archive_warnings)) => {
                all_warnings.extend(archive_warnings);
                for skill in skills {
                    let key = (
                        skill.name.clone(),
                        skill.package_name.clone(),
                        skill.package_version.clone(),
                    );
                    if !seen_names.insert(key) {
                        continue;
                    }
                    all_skills.push(skill);
                }
            }
            Err(error) => {
                all_warnings.push(format!(
                    "could not load skills from Maven artifact {} ({error})",
                    jar_path.display()
                ));
            }
        }
    }

    sort_dependency_skills(&mut all_skills);
    Ok((all_skills, all_warnings))
}

fn project_skill_matches_in(
    file_system: &dyn FileSystem,
    environment: &ProjectEnvironment,
) -> Result<Vec<SkillMatchData>> {
    let installed = discover_installed_skills_in(file_system, &environment.directory)?;
    let mut matches = Vec::new();

    for source in &environment.sources {
        match source {
            ProjectSource::Python(settings) => {
                if file_system
                    .exists(&settings.pyproject_toml_path)
                    .unwrap_or(false)
                {
                    let requirements = scan_project_requirements_in(
                        file_system,
                        &settings.pyproject_toml_path,
                        settings,
                    )?;
                    let origins_by_package = package_dependency_origins(&requirements);

                    for skill in discover_venv_skills_in(file_system, &settings.venv_path)? {
                        let package_name = match skill.package_name.as_ref() {
                            Some(name) => name,
                            None => continue,
                        };
                        let dependency_origins = match origins_by_package.get(package_name) {
                            Some(origins) => origins.clone(),
                            None => continue,
                        };
                        matches.push(SkillMatchData {
                            installed: match_installed(&installed, &skill),
                            available: skill,
                            dependency_origins,
                        });
                    }
                }
            }
            ProjectSource::Node(settings) => {
                if file_system
                    .exists(&settings.package_json_path)
                    .unwrap_or(false)
                {
                    let node_requirements = scan_node_requirements_in(
                        file_system,
                        &settings.package_json_path,
                        settings,
                    )?;
                    let node_origins_by_package = package_dependency_origins(&node_requirements);

                    for skill in
                        discover_node_modules_skills_in(file_system, &settings.node_modules_path)?
                    {
                        let package_name = match skill.package_name.as_ref() {
                            Some(name) => name,
                            None => continue,
                        };
                        let dependency_origins = match node_origins_by_package.get(package_name) {
                            Some(origins) => origins.clone(),
                            None => continue,
                        };
                        matches.push(SkillMatchData {
                            installed: match_installed(&installed, &skill),
                            available: skill,
                            dependency_origins,
                        });
                    }
                }
            }
            ProjectSource::Maven(settings) => {
                let (skills, warnings) = discover_maven_skills_in(file_system, settings)?;
                for warning in warnings {
                    eprintln!("skilly: {warning}");
                }
                let dependency_origins = vec![ProjectDependencyOrigin {
                    ecosystem: PACKAGE_ECOSYSTEM_MAVEN.to_string(),
                    scope: "compile".to_string(),
                }];
                for skill in skills {
                    matches.push(SkillMatchData {
                        installed: match_installed(&installed, &skill),
                        available: skill,
                        dependency_origins: dependency_origins.clone(),
                    });
                }
            }
        }
    }

    Ok(matches)
}

/// Find an installed skill that matches the available skill.
#[must_use]
pub fn match_installed(
    installed_skills: &[SkillData],
    available_skill: &SkillData,
) -> Option<SkillData> {
    installed_skills
        .iter()
        .find(|installed_skill| available_skill.matches(installed_skill))
        .cloned()
}

pub fn scan_project_in(environment: &ProjectEnvironment) -> Result<Vec<SkillMatchData>> {
    scan_project_with_file_system(&NATIVE_FILE_SYSTEM, environment)
}

pub fn scan_project_with_file_system(
    file_system: &dyn FileSystem,
    environment: &ProjectEnvironment,
) -> Result<Vec<SkillMatchData>> {
    let mut matches = project_skill_matches_in(file_system, environment)?;
    matches.sort_by(|left, right| {
        (
            left.available.package_ecosystem.as_ref(),
            left.available.package_name.as_deref().unwrap_or(""),
            left.available.name.as_str(),
            left.available.package_version.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.available.package_ecosystem.as_ref(),
                right.available.package_name.as_deref().unwrap_or(""),
                right.available.name.as_str(),
                right.available.package_version.as_deref().unwrap_or(""),
            ))
    });
    Ok(matches)
}

pub fn dependency_updates_in(environment: &ProjectEnvironment) -> Result<Vec<SkillMatchData>> {
    dependency_updates_with_file_system(&NATIVE_FILE_SYSTEM, environment)
}

pub fn dependency_updates_with_file_system(
    file_system: &dyn FileSystem,
    environment: &ProjectEnvironment,
) -> Result<Vec<SkillMatchData>> {
    Ok(scan_project_with_file_system(file_system, environment)?
        .into_iter()
        .filter(|item| {
            scan_match_status(&item.available, item.installed.as_ref()) == STATUS_UPDATABLE
        })
        .collect())
}

pub fn available_dependency_skill_in(
    installed_skill: &SkillData,
    environment: &ProjectEnvironment,
) -> Result<Option<SkillData>> {
    available_dependency_skill_with_file_system(&NATIVE_FILE_SYSTEM, installed_skill, environment)
}

pub fn available_dependency_skill_with_file_system(
    file_system: &dyn FileSystem,
    installed_skill: &SkillData,
    environment: &ProjectEnvironment,
) -> Result<Option<SkillData>> {
    Ok(project_skill_matches_in(file_system, environment)?
        .into_iter()
        .map(|item| item.available)
        .find(|skill| skill.matches(installed_skill)))
}

/// Parse a GitHub URL into a structured skill location.
///
/// Supports `https://github.com/<owner>/<repo>` and
/// `https://github.com/<owner>/<repo>/tree/<ref>/<path>`.
pub fn parse_github_skill_url(github_url: &str) -> Result<GitHubSkillLocationData> {
    let parsed = Url::parse(github_url)?;
    if parsed.host_str() != Some("github.com") {
        bail!(
            "unsupported GitHub URL host: {}",
            parsed.host_str().unwrap_or_default()
        );
    }
    let parts = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .map(|segment| percent_decode_str(segment).decode_utf8_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if parts.len() < 2 {
        bail!(
            "GitHub skill URLs must look like https://github.com/<owner>/<repo> or https://github.com/<owner>/<repo>/tree/<ref>/<path>"
        );
    }

    let mut ref_name = None;
    let mut path = ".".to_string();
    if parts.len() >= 3 {
        if parts[2] != "tree" {
            bail!(
                "github skill URLs must look like https://github.com/<owner>/<repo> or https://github.com/<owner>/<repo>/tree/<ref>/<path>"
            );
        }
        if parts.len() < 4 {
            bail!(
                "github tree URLs must include a ref like https://github.com/<owner>/<repo>/tree/<ref>"
            );
        }
        ref_name = Some(parts[3].clone());
        if parts.len() > 4 {
            path = parts[4..].join("/");
        }
    }

    Ok(GitHubSkillLocationData {
        owner: parts[0].clone(),
        repo: parts[1].clone(),
        r#ref: ref_name,
        path,
        url: github_url.to_string(),
    })
}

/// Parse a repository URL into a provider-neutral, validated location.
///
/// GitHub and Bitbucket Cloud are auto-detected from their public hosts.
/// Bitbucket Data Center must be selected explicitly because a self-hosted URL
/// cannot be identified safely from its host name alone.
pub fn parse_repository_location(
    repository_url: &str,
    provider: Option<RepositoryProvider>,
) -> Result<RepositoryLocationData> {
    let parsed = Url::parse(repository_url)?;
    let detected_provider = match parsed.host_str() {
        Some("github.com") => Some(RepositoryProvider::GitHub),
        Some("bitbucket.org") => Some(RepositoryProvider::BitbucketCloud),
        _ => None,
    };
    let provider = match (provider, detected_provider) {
        (Some(explicit), Some(detected)) if explicit != detected => bail!(
            "repository URL host does not match explicit provider {}",
            explicit.as_str()
        ),
        (Some(provider), _) => provider,
        (None, Some(provider)) => provider,
        (None, None) => bail!(
            "could not detect repository provider; pass --provider bitbucket-data-center for a self-hosted Bitbucket URL"
        ),
    };

    match provider {
        RepositoryProvider::GitHub => parse_github_repository_location(repository_url),
        RepositoryProvider::BitbucketCloud => parse_bitbucket_cloud_location(repository_url),
        RepositoryProvider::BitbucketDataCenter => {
            parse_bitbucket_data_center_location(repository_url)
        }
    }
}

fn parse_github_repository_location(repository_url: &str) -> Result<RepositoryLocationData> {
    let location = parse_github_skill_url(repository_url)?;
    Ok(RepositoryLocationData {
        provider: RepositoryProvider::GitHub,
        base_url: "https://github.com".to_string(),
        namespace: location.owner,
        repo: location.repo,
        r#ref: location.r#ref,
        path: location.path,
        url: location.url,
    })
}

fn parse_bitbucket_cloud_location(repository_url: &str) -> Result<RepositoryLocationData> {
    let parsed = Url::parse(repository_url)?;
    if parsed.host_str() != Some("bitbucket.org") {
        bail!(
            "unsupported Bitbucket Cloud URL host: {}",
            parsed.host_str().unwrap_or_default()
        );
    }
    let parts = decoded_path_segments(&parsed);
    if parts.len() < 2 {
        bail!(
            "Bitbucket Cloud skill URLs must look like https://bitbucket.org/<workspace>/<repo> or https://bitbucket.org/<workspace>/<repo>/src/<ref>/<path>"
        );
    }

    let (reference, path) = match parts.get(2).map(String::as_str) {
        None => (None, ".".to_string()),
        Some("src") if parts.len() >= 4 => {
            let path = if parts.len() > 4 {
                parts[4..].join("/")
            } else {
                ".".to_string()
            };
            (Some(parts[3].clone()), path)
        }
        Some("src") => bail!(
            "Bitbucket Cloud source URLs must include a ref like https://bitbucket.org/<workspace>/<repo>/src/<ref>"
        ),
        Some(_) => bail!(
            "Bitbucket Cloud skill URLs must look like https://bitbucket.org/<workspace>/<repo> or https://bitbucket.org/<workspace>/<repo>/src/<ref>/<path>"
        ),
    };

    Ok(RepositoryLocationData {
        provider: RepositoryProvider::BitbucketCloud,
        base_url: "https://bitbucket.org".to_string(),
        namespace: parts[0].clone(),
        repo: parts[1].clone(),
        r#ref: reference,
        path,
        url: repository_url.to_string(),
    })
}

fn parse_bitbucket_data_center_location(repository_url: &str) -> Result<RepositoryLocationData> {
    let parsed = Url::parse(repository_url)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("Bitbucket Data Center URLs must use http or https");
    }
    let parts = decoded_path_segments(&parsed);
    let Some(project_index) = parts.iter().position(|part| part == "projects") else {
        bail!("Bitbucket Data Center URLs must include /projects/<project>/repos/<repo>");
    };
    if parts.get(project_index + 2).map(String::as_str) != Some("repos")
        || parts.len() < project_index + 4
    {
        bail!("Bitbucket Data Center URLs must include /projects/<project>/repos/<repo>");
    }
    let trailing = &parts[project_index + 4..];
    let path_from_url = match trailing {
        [] => ".".to_string(),
        [segment] if segment == "browse" => ".".to_string(),
        [segment, path @ ..] if segment == "browse" && !path.is_empty() => path.join("/"),
        _ => bail!("Bitbucket Data Center skill URLs must end at the repository or /browse/<path>"),
    };
    let query_path = parsed
        .query_pairs()
        .find(|(key, _)| key == "path")
        .map(|(_, value)| value.to_string());
    if query_path.is_some() && path_from_url != "." {
        bail!("Bitbucket Data Center skill URL cannot specify both a browse path and path query");
    }
    let reference = parsed
        .query_pairs()
        .find(|(key, _)| key == "at")
        .map(|(_, value)| value.to_string());
    let authority = parsed
        .host_str()
        .ok_or_else(|| anyhow!("Bitbucket Data Center URL is missing a host"))?;
    let authority = parsed
        .port()
        .map(|port| format!("{authority}:{port}"))
        .unwrap_or_else(|| authority.to_string());
    let base_prefix = parts[..project_index].join("/");
    let base_url = if base_prefix.is_empty() {
        format!("{}://{authority}", parsed.scheme())
    } else {
        format!("{}://{authority}/{base_prefix}", parsed.scheme())
    };

    Ok(RepositoryLocationData {
        provider: RepositoryProvider::BitbucketDataCenter,
        base_url,
        namespace: parts[project_index + 1].clone(),
        repo: parts[project_index + 3].clone(),
        r#ref: reference,
        path: query_path.unwrap_or(path_from_url),
        url: repository_url.to_string(),
    })
}

fn decoded_path_segments(parsed: &Url) -> Vec<String> {
    parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .map(|segment| percent_decode_str(segment).decode_utf8_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Build a GitHub tree URL from a location, path, and optional ref.
#[must_use]
#[allow(dead_code)]
pub fn build_github_skill_url(
    location: &GitHubSkillLocationData,
    path: &str,
    ref_name: Option<&str>,
) -> Option<String> {
    let base_url = format!("https://github.com/{}/{}", location.owner, location.repo);
    let resolved_ref = ref_name.or(location.r#ref.as_deref());
    if path == "." || path.is_empty() {
        return resolved_ref
            .map(|value| format!("{base_url}/tree/{value}"))
            .or(Some(base_url));
    }
    resolved_ref.map(|value| format!("{base_url}/tree/{value}/{path}"))
}

/// Find all skill directories (containing SKILL.md) in a set of GitHub files.
#[must_use]
#[allow(dead_code)]
pub fn find_github_skill_dirs(
    files: &BTreeMap<String, GitHubFileBlobData>,
    root: &str,
) -> Vec<String> {
    let normalized_root = if root == "." { "" } else { root };
    let mut dirs = files
        .keys()
        .filter(|path| path.ends_with("/SKILL.md") || *path == "SKILL.md")
        .filter_map(|path| {
            let directory = Path::new(path)
                .parent()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| ".".to_string());
            let matches_root = normalized_root.is_empty()
                || directory == normalized_root
                || directory.starts_with(&format!("{normalized_root}/"));
            matches_root.then_some(directory)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

/// Discover skills from any supported Git repository provider.
pub fn discover_repository_skills<F: RepositorySnapshotFetcher>(
    fetcher: &F,
    repository_url: &str,
    provider: Option<RepositoryProvider>,
) -> Result<Vec<SkillData>> {
    let location = parse_repository_location(repository_url, provider)?;
    let snapshot = fetcher.fetch_repository_snapshot(&location)?;
    let skill_dirs = find_repository_skill_dirs(&snapshot.files, &location.path);
    if skill_dirs.is_empty() {
        bail!("no SKILL.md found at {repository_url}");
    }

    skill_dirs
        .into_iter()
        .map(|skill_dir| {
            let skill_url = build_repository_skill_url(&location, &skill_dir, &snapshot.ref_name);
            build_skill_from_repository_files(
                &snapshot.files,
                &skill_dir,
                &location.provider,
                skill_url,
                &snapshot.commit_sha,
            )
        })
        .collect()
}

fn find_repository_skill_dirs(
    files: &BTreeMap<String, RepositoryFileBlobData>,
    root: &str,
) -> Vec<String> {
    let normalized_root = if root == "." { "" } else { root };
    let dirs = files
        .keys()
        .filter(|path| path.ends_with("/SKILL.md") || *path == "SKILL.md")
        .filter_map(|path| {
            let directory = Path::new(path)
                .parent()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| ".".to_string());
            (normalized_root.is_empty()
                || directory == normalized_root
                || directory.starts_with(&format!("{normalized_root}/")))
            .then_some(directory)
        })
        .collect::<BTreeSet<_>>();
    dirs.into_iter().collect()
}

fn build_skill_from_repository_files(
    files: &BTreeMap<String, RepositoryFileBlobData>,
    skill_dir: &str,
    provider: &RepositoryProvider,
    repository_url: String,
    repository_commit_sha: &str,
) -> Result<SkillData> {
    let skill_md_path = if skill_dir == "." {
        "SKILL.md".to_string()
    } else {
        format!("{skill_dir}/SKILL.md")
    };
    let skill_blob = files
        .get(&skill_md_path)
        .ok_or_else(|| anyhow!("SKILL.md not found at {skill_dir}"))?;
    let skill_text = std::str::from_utf8(&skill_blob.content)
        .with_context(|| format!("SKILL.md at {skill_dir} is not valid UTF-8"))?;
    let mut skill = SkillData::from_text(
        skill_text,
        None,
        &SkillSourceMetadata {
            source: Some(SKILLY_SOURCE_REPOSITORY.to_string()),
            package_name: None,
            package_version: None,
            repository_provider: Some(*provider),
            repository_url: Some(repository_url),
            repository_commit_sha: Some(repository_commit_sha.to_string()),
            package_ecosystem: None,
        },
    )?;
    let prefix = if skill_dir == "." {
        String::new()
    } else {
        format!("{skill_dir}/")
    };
    skill.resources = files
        .iter()
        .filter_map(|(path, blob)| {
            if path == &skill_md_path || (!prefix.is_empty() && !path.starts_with(&prefix)) {
                return None;
            }
            let relative_path = if prefix.is_empty() {
                path.clone()
            } else {
                path[prefix.len()..].to_string()
            };
            Some(SkillResourceData {
                relative_path: relative_path.clone(),
                kind: classify_resource_kind(&relative_path),
                raw: blob.content.clone(),
            })
        })
        .collect();
    skill
        .resources
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(skill)
}

fn build_repository_skill_url(
    location: &RepositoryLocationData,
    skill_dir: &str,
    ref_name: &str,
) -> String {
    let path = (skill_dir != ".").then_some(skill_dir);
    match location.provider {
        RepositoryProvider::GitHub => path.map_or_else(
            || {
                format!(
                    "{}/{}/{}",
                    location.base_url, location.namespace, location.repo
                )
            },
            |path| {
                format!(
                    "{}/{}/{}/tree/{ref_name}/{path}",
                    location.base_url, location.namespace, location.repo
                )
            },
        ),
        RepositoryProvider::BitbucketCloud => path.map_or_else(
            || {
                format!(
                    "{}/{}/{}",
                    location.base_url, location.namespace, location.repo
                )
            },
            |path| {
                format!(
                    "{}/{}/{}/src/{ref_name}/{path}",
                    location.base_url, location.namespace, location.repo
                )
            },
        ),
        RepositoryProvider::BitbucketDataCenter => path.map_or_else(
            || {
                format!(
                    "{}/projects/{}/repos/{}?at={ref_name}",
                    location.base_url, location.namespace, location.repo
                )
            },
            |path| {
                format!(
                    "{}/projects/{}/repos/{}/browse/{path}?at={ref_name}",
                    location.base_url, location.namespace, location.repo
                )
            },
        ),
    }
}

#[allow(dead_code)]
fn skill_dirs_len_eq_location(location_path: &str, skill_dir: &str) -> bool {
    (location_path == "." && skill_dir == ".") || location_path == skill_dir
}

/// Maximum total archive size for skill loading (200 MB).
pub const MAX_ARCHIVE_SIZE: u64 = 200 * 1024 * 1024;
/// Maximum number of entries in an archive.
pub const MAX_ARCHIVE_ENTRIES: usize = 100_000;
/// Maximum uncompressed size for a single resource within an archive (10 MB).
pub const MAX_ARCHIVE_RESOURCE_SIZE: u64 = 10 * 1024 * 1024;
/// Maximum cumulative uncompressed size across all extracted entries (500 MB).
pub const MAX_ARCHIVE_CUMULATIVE_SIZE: u64 = 500 * 1024 * 1024;

/// Discover skills from a raw ZIP/JAR archive without extracting to disk.
///
/// Scans entries under both `skills/<name>/SKILL.md` and `.agents/skills/<name>/SKILL.md`
/// layouts, enforces size and entry limits, and returns complete [`SkillData`] records
/// including bundled resources read from the same archive.
pub fn load_skills_from_archive(
    archive_bytes: &[u8],
    source_metadata: &SkillSourceMetadata,
) -> Result<(Vec<SkillData>, Vec<String>)> {
    let total_size = archive_bytes.len() as u64;
    if total_size > MAX_ARCHIVE_SIZE {
        bail!("archive size {total_size} exceeds maximum {MAX_ARCHIVE_SIZE} bytes");
    }

    let cursor = std::io::Cursor::new(archive_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|error| anyhow!("invalid ZIP archive: {error}"))?;

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!(
            "archive entry count {} exceeds maximum {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        );
    }

    let mut warnings = Vec::new();
    let mut skill_entries: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    let skill_layout_prefixes = [".agents/skills/", "skills/"];
    let mut seen_entry_paths = BTreeSet::new();
    let mut cumulative_size: u64 = 0;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| anyhow!("could not read archive entry {index}: {error}"))?;

        let name = entry.name().to_string();
        let normalized = name.replace('\\', "/");

        if !validate_archive_entry_path(&normalized) {
            warnings.push(format!("skipping unsafe archive entry path: {name:?}"));
            continue;
        }

        // Reject duplicate entry paths
        if !seen_entry_paths.insert(normalized.clone()) {
            bail!("duplicate archive entry path: {name:?}");
        }

        if entry.is_dir() {
            continue;
        }

        // Only decompress entries that belong to a skill layout directory
        let mut matched_layout = false;
        for prefix in skill_layout_prefixes {
            if let Some(remainder) = normalized.strip_prefix(prefix) {
                if remainder.is_empty() || !remainder.contains('/') {
                    break;
                }
                matched_layout = true;
                break;
            }
        }
        if !matched_layout {
            continue;
        }

        let uncompressed_size = entry.size();
        if uncompressed_size > MAX_ARCHIVE_RESOURCE_SIZE {
            warnings.push(format!(
                "skipping oversized archive entry {name:?} ({uncompressed_size} bytes)"
            ));
            continue;
        }

        cumulative_size += uncompressed_size;
        if cumulative_size > MAX_ARCHIVE_CUMULATIVE_SIZE {
            bail!(
                "cumulative uncompressed size {cumulative_size} exceeds maximum {MAX_ARCHIVE_CUMULATIVE_SIZE} bytes"
            );
        }

        let mut buf = Vec::with_capacity(uncompressed_size as usize);
        std::io::copy(&mut entry, &mut buf)
            .map_err(|error| anyhow!("could not read archive entry {name:?}: {error}"))?;

        for prefix in skill_layout_prefixes {
            if let Some(remainder) = normalized.strip_prefix(prefix) {
                if let Some((skill_name, rest)) = remainder.split_once('/') {
                    if rest.is_empty() {
                        continue;
                    }
                    let key = format!("{prefix}{skill_name}");
                    skill_entries
                        .entry(key)
                        .or_default()
                        .push((rest.to_string(), buf.clone()));
                }
                break;
            }
        }
    }

    let mut seen_dirs = BTreeSet::new();
    let mut duplicate_names = BTreeSet::new();
    let mut pending: Vec<(String, SkillData)> = Vec::new();

    for (dir_path, resources) in skill_entries {
        let Some(skill_dir_name) = dir_path.rsplit('/').next() else {
            continue;
        };
        let normalized_name = skill_dir_name.to_ascii_lowercase();
        if !seen_dirs.insert(normalized_name.clone()) {
            duplicate_names.insert(normalized_name.clone());
            continue;
        }

        let skill_md_entry = resources.iter().find(|(path, _)| path == "SKILL.md");
        let Some((_, skill_md_content)) = skill_md_entry else {
            warnings.push(format!(
                "no SKILL.md found in archive skill directory {dir_path:?}; skipping"
            ));
            continue;
        };

        let skill_md_text = String::from_utf8(skill_md_content.clone())
            .map_err(|error| anyhow!("SKILL.md in {dir_path:?} is not valid UTF-8: {error}"))?;

        let mut skill = SkillData::from_text_parts(&skill_md_text, None, source_metadata)?;

        let mut skill_resources = Vec::new();
        for (rel_path, content_bytes) in &resources {
            if rel_path == "SKILL.md" {
                continue;
            }
            skill_resources.push(SkillResourceData {
                relative_path: rel_path.clone(),
                kind: classify_resource_kind(rel_path),
                raw: content_bytes.clone(),
            });
        }
        skill_resources.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        skill.resources = skill_resources;

        pending.push((normalized_name, skill));
    }

    // Remove ambiguous dual-layout skills (same name in both layouts)
    for dup in &duplicate_names {
        warnings.push(format!(
            "ambiguous duplicate skill directory name {dup:?} found in archive; both copies ignored"
        ));
    }
    pending.retain(|(name, _)| !duplicate_names.contains(name));

    let mut skills: Vec<_> = pending.into_iter().map(|(_, skill)| skill).collect();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((skills, warnings))
}

/// Validate that an archive entry path is safe: no traversal, no absolute roots,
/// no backslashes, no empty segments.
fn validate_archive_entry_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

/// A Maven coordinate parsed from a `pom.xml` dependency entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    pub scope: Option<String>,
}

/// Parse direct Maven dependencies from a `pom.xml` string using a
/// structured XML reader.
///
/// Supports concrete versions and `${property}` values defined in the same
/// POM's `<properties>` block. Returns coordinates and any warnings for
/// unresolved or unsupported constructs.
pub fn parse_maven_dependencies(pom_xml: &str) -> Result<(Vec<MavenCoordinate>, Vec<String>)> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(pom_xml);
    reader.config_mut().trim_text(true);

    let mut warnings = Vec::new();
    let mut properties = BTreeMap::new();
    let mut coordinates = Vec::new();

    // Depth tracking so we only process direct-child <dependencies> of <project>.
    let mut depth = 0u32;
    let mut in_project = false;
    let mut in_dependencies = false;
    let mut in_dependency = false;
    // Skip sections whose dependencies we ignore.
    let mut skip_depth = None::<u32>;
    // Current dependency fields
    let mut current_group_id: Option<String> = None;
    let mut current_artifact_id: Option<String> = None;
    let mut current_version: Option<String> = None;
    let mut current_scope: Option<String> = None;
    // Track current text content
    let mut current_text = String::new();
    let mut in_properties = false;
    let mut prop_name: Option<String> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if skip_depth.is_none()
                    && matches!(
                        tag_name.as_str(),
                        "dependencyManagement" | "profiles" | "build" | "plugin" | "plugins"
                    )
                {
                    skip_depth = Some(depth);
                }

                if tag_name == "project" && depth == 1 {
                    in_project = true;
                }

                if in_project && tag_name == "properties" && skip_depth.is_none() {
                    in_properties = true;
                }

                if in_properties && tag_name != "properties" {
                    prop_name = Some(tag_name.clone());
                }

                if in_project && tag_name == "dependencies" && skip_depth.is_none() {
                    in_dependencies = true;
                }

                if in_dependencies && tag_name == "dependency" {
                    in_dependency = true;
                    current_group_id = None;
                    current_artifact_id = None;
                    current_version = None;
                    current_scope = None;
                }

                current_text.clear();
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if skip_depth == Some(depth) {
                    skip_depth = None;
                }

                if in_properties && tag_name == "properties" {
                    in_properties = false;
                    prop_name = None;
                }

                if in_properties {
                    if let Some(ref name) = prop_name {
                        let text = current_text.trim().to_string();
                        if !text.is_empty() {
                            properties.insert(name.clone(), text);
                        }
                    }
                    prop_name = None;
                }

                if in_dependency {
                    match tag_name.as_str() {
                        "groupId" => {
                            current_group_id = Some(std::mem::take(&mut current_text));
                        }
                        "artifactId" => {
                            current_artifact_id = Some(std::mem::take(&mut current_text));
                        }
                        "version" => {
                            current_version = Some(std::mem::take(&mut current_text));
                        }
                        "scope" => {
                            let text = std::mem::take(&mut current_text);
                            current_scope = if text.trim().is_empty() {
                                None
                            } else {
                                Some(text.trim().to_string())
                            };
                        }
                        "dependency" => {
                            in_dependency = false;
                            match (
                                current_group_id.take(),
                                current_artifact_id.take(),
                                current_version.take(),
                            ) {
                                (Some(g), Some(a), Some(v)) => {
                                    let resolved = if v.starts_with("${") && v.ends_with('}') {
                                        let key = &v[2..v.len() - 1];
                                        match properties.get(key) {
                                            Some(val) => val.clone(),
                                            None => {
                                                warnings.push(format!(
                                                    "skipping Maven dependency {g}:{a}: could not resolve version {v:?}"
                                                ));
                                                current_text.clear();
                                                continue;
                                            }
                                        }
                                    } else {
                                        v
                                    };

                                    match validate_maven_coordinate(&g, &a, &resolved) {
                                        Ok(()) => {}
                                        Err(error) => {
                                            warnings.push(format!(
                                                "rejecting unsafe Maven dependency {g}:{a}:{resolved}: {error}"
                                            ));
                                            current_text.clear();
                                            continue;
                                        }
                                    }

                                    coordinates.push(MavenCoordinate {
                                        group_id: g,
                                        artifact_id: a,
                                        version: resolved,
                                        scope: current_scope.take(),
                                    });
                                }
                                (Some(g), Some(a), None) => {
                                    warnings.push(format!(
                                        "skipping Maven dependency {g}:{a}: missing version"
                                    ));
                                }
                                _ => {
                                    warnings.push(
                                        "skipping malformed Maven dependency entry".to_string(),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if in_project && tag_name == "project" {
                    in_project = false;
                }

                if in_dependencies && tag_name == "dependencies" && skip_depth.is_none() {
                    in_dependencies = false;
                }

                current_text.clear();
                depth -= 1;
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                current_text.push_str(&text);
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(anyhow!(
                    "malformed pom.xml at position {}: {error}",
                    reader.buffer_position()
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    // Treat unclosed elements as malformed XML.
    if depth != 0 || in_project {
        bail!("malformed pom.xml: unclosed elements detected (depth {depth})");
    }

    Ok((coordinates, warnings))
}

/// Validate a Maven coordinate component for path traversal safety.
fn validate_maven_coordinate_component(component: &str, label: &str) -> Result<()> {
    if component.is_empty() {
        bail!("{label} must not be empty");
    }
    if component.contains("..") {
        bail!("{label} must not contain \"..\"");
    }
    if component.contains('/') || component.contains('\\') {
        bail!("{label} must not contain path separators");
    }
    if component.starts_with('.') {
        bail!("{label} must not start with \".\"");
    }
    if component.contains(':') {
        bail!("{label} must not contain \":\"");
    }
    Ok(())
}

/// Validate that groupId, artifactId, and version are safe for
/// filesystem path construction.
fn validate_maven_coordinate(group_id: &str, artifact_id: &str, version: &str) -> Result<()> {
    validate_maven_coordinate_component(group_id, "groupId")?;
    // Allow dots in groupId (they map to path separators)
    for part in group_id.split('.') {
        if part.is_empty() {
            bail!("groupId must not contain empty segments");
        }
    }
    validate_maven_coordinate_component(artifact_id, "artifactId")?;
    validate_maven_coordinate_component(version, "version")?;
    if version.contains('.') {
        // Dots in version are fine (they're version separators, not path)
        for part in version.split('.') {
            if part.is_empty() {
                bail!("version must not contain empty segments");
            }
        }
    }
    Ok(())
}

/// Format a Maven coordinate as `groupId:artifactId:version`.
#[must_use]
pub fn format_maven_ref(coordinate: &MavenCoordinate) -> String {
    format!(
        "{}:{}:{}",
        coordinate.group_id, coordinate.artifact_id, coordinate.version
    )
}

/// Format a Maven package name as `groupId:artifactId` (without version).
#[must_use]
pub fn format_maven_package_name(coordinate: &MavenCoordinate) -> String {
    format!("{}:{}", coordinate.group_id, coordinate.artifact_id)
}

/// Resolve the local repository path for a Maven coordinate.
///
/// Maps `groupId:artifactId:version` to the standard Maven repository layout:
/// `{repo}/{groupId with . as /}/{artifactId}/{version}/{artifactId}-{version}.jar`
#[must_use]
pub fn maven_jar_path(repository: &Path, coordinate: &MavenCoordinate) -> PathBuf {
    let group_path = coordinate.group_id.replace('.', "/");
    repository
        .join(group_path)
        .join(&coordinate.artifact_id)
        .join(&coordinate.version)
        .join(format!(
            "{}-{}.jar",
            coordinate.artifact_id, coordinate.version
        ))
}

/// Check whether installed and available repository skills share a commit.
#[must_use]
#[inline]
pub fn repository_versions_match(installed: &SkillData, available: &SkillData) -> bool {
    installed.repository_provider == available.repository_provider
        && installed.repository_url == available.repository_url
        && installed.repository_commit_sha.is_some()
        && installed.repository_commit_sha == available.repository_commit_sha
}

#[cfg(test)]
mod tests {
    use super::{
        BundleValidationCode, NamedSelection, PACKAGE_ECOSYSTEM_NODE, PACKAGE_ECOSYSTEM_PYTHON,
        PackageEcosystem, ProjectDependencyOrigin, RepositoryFileBlobData, RepositoryLocationData,
        RepositorySnapshotData, RepositorySnapshotFetcher, ScanDependencySelection, SkillData,
        SkillResourceData, SkillSourceMetadata, discover_repository_skills,
        parse_node_dependency_entries, parse_project_requirement_entries,
        parse_project_requirements, validate_node_package_name,
    };
    use std::collections::{BTreeMap, BTreeSet};

    const PYPROJECT: &str = r#"
[project]
name = "demo"
version = "0.1.0"
dependencies = ["base-pkg>=1", "shared-pkg>=1"]

[project.optional-dependencies]
docs = ["docs-pkg>=1", "shared-pkg>=1"]

[dependency-groups]
dev = ["dev-pkg>=1", "shared-pkg>=1"]
"#;

    #[test]
    fn project_requirements_include_selected_groups_and_optional_dependencies() {
        let requirements = parse_project_requirements(
            PYPROJECT,
            true,
            &["docs".to_string(), "missing".to_string()],
        )
        .expect("requirements should parse");

        assert_eq!(
            requirements,
            vec![
                "base-pkg>=1".to_string(),
                "shared-pkg>=1".to_string(),
                "dev-pkg>=1".to_string(),
                "shared-pkg>=1".to_string(),
                "docs-pkg>=1".to_string(),
                "shared-pkg>=1".to_string(),
            ]
        );
    }

    #[test]
    fn scan_dependency_selection_filters_categories() {
        let requirements =
            parse_project_requirement_entries(PYPROJECT).expect("requirements should parse");

        let selected = requirements
            .into_iter()
            .filter(|requirement| {
                ScanDependencySelection {
                    include_project_dependencies: false,
                    dependency_groups: NamedSelection::All,
                    optional_dependencies: NamedSelection::Include(BTreeSet::new()),
                    include_node_dependencies: false,
                    include_node_dev_dependencies: false,
                    include_node_optional_dependencies: false,
                }
                .includes(&requirement.origin)
            })
            .collect::<Vec<_>>();

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|requirement| {
            requirement.origin == ProjectDependencyOrigin::python_dependency_group("dev")
        }));
    }

    #[test]
    fn scan_dependency_selection_can_include_named_groups_and_exclude_named_extras() {
        let requirements =
            parse_project_requirement_entries(PYPROJECT).expect("requirements should parse");

        let selected = requirements
            .into_iter()
            .filter(|requirement| {
                ScanDependencySelection {
                    include_project_dependencies: true,
                    dependency_groups: NamedSelection::Include(
                        ["dev".to_string()].into_iter().collect(),
                    ),
                    optional_dependencies: NamedSelection::Exclude(
                        ["docs".to_string()].into_iter().collect(),
                    ),
                    include_node_dependencies: false,
                    include_node_dev_dependencies: false,
                    include_node_optional_dependencies: false,
                }
                .includes(&requirement.origin)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            selected
                .into_iter()
                .map(|requirement| requirement.spec)
                .collect::<Vec<_>>(),
            vec![
                "base-pkg>=1".to_string(),
                "shared-pkg>=1".to_string(),
                "dev-pkg>=1".to_string(),
                "shared-pkg>=1".to_string(),
            ]
        );
    }

    // ── Node dependency parsing ──────────────────────────────────────

    #[test]
    fn node_package_name_validation_rejects_unsafe_input() {
        assert!(validate_node_package_name("").is_err());
        assert!(validate_node_package_name("..").is_err());
        assert!(validate_node_package_name("foo/..").is_err());
        assert!(validate_node_package_name("../escape").is_err());
        assert!(validate_node_package_name("foo/../bar").is_err());
        assert!(validate_node_package_name("pkg\\traversal").is_err());
        assert!(validate_node_package_name("@scope/..").is_err());
        assert!(validate_node_package_name("./local").is_err());
    }

    #[test]
    fn node_package_name_validation_accepts_valid_names() {
        assert!(validate_node_package_name("simple-pkg").is_ok());
        assert!(validate_node_package_name("@scope/my-pkg").is_ok());
        assert!(validate_node_package_name("@babel/core").is_ok());
        assert!(validate_node_package_name("typescript").is_ok());
        assert!(validate_node_package_name("under_score").is_ok());
    }

    #[test]
    fn parse_node_dependencies_extracts_all_sections() {
        let package_json = r#"{
            "dependencies": {
                "react": "^18.0.0",
                "lodash": "4.17.21"
            },
            "devDependencies": {
                "typescript": "5.0.0",
                "eslint": "8.0.0"
            },
            "optionalDependencies": {
                "sharp": "0.32.0"
            }
        }"#;

        let entries =
            parse_node_dependency_entries(package_json).expect("package.json should parse");

        let specs: BTreeSet<_> = entries.iter().map(|e| e.spec.clone()).collect();
        assert!(specs.contains("react"));
        assert!(specs.contains("lodash"));
        assert!(specs.contains("typescript"));
        assert!(specs.contains("eslint"));
        assert!(specs.contains("sharp"));
        assert_eq!(specs.len(), 5);
    }

    #[test]
    fn parse_node_dependencies_handles_scoped_packages() {
        let package_json = r#"{
            "dependencies": {
                "@scope/package-a": "1.0.0",
                "@babel/core": "7.0.0"
            },
            "devDependencies": {
                "@types/node": "20.0.0"
            }
        }"#;

        let entries =
            parse_node_dependency_entries(package_json).expect("package.json should parse");

        let specs: BTreeSet<_> = entries.iter().map(|e| e.spec.clone()).collect();
        assert!(specs.contains("@scope/package-a"));
        assert!(specs.contains("@babel/core"));
        assert!(specs.contains("@types/node"));
    }

    #[test]
    fn parse_node_dependencies_handles_duplicate_packages_across_sections() {
        let package_json = r#"{
            "dependencies": {
                "shared-lib": "1.0.0"
            },
            "devDependencies": {
                "shared-lib": "2.0.0"
            }
        }"#;

        let entries =
            parse_node_dependency_entries(package_json).expect("package.json should parse");

        assert_eq!(entries.len(), 2);
        let specs: Vec<_> = entries
            .iter()
            .map(|e| (e.spec.as_str(), e.origin.clone()))
            .collect();
        assert_eq!(
            specs[0],
            ("shared-lib", ProjectDependencyOrigin::node_dependencies())
        );
        assert_eq!(
            specs[1],
            (
                "shared-lib",
                ProjectDependencyOrigin::node_dev_dependencies()
            )
        );
    }

    #[test]
    fn parse_node_dependencies_handles_empty_sections() {
        let package_json = r#"{
            "dependencies": {}
        }"#;

        let entries =
            parse_node_dependency_entries(package_json).expect("package.json should parse");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_node_dependencies_handles_missing_sections() {
        let package_json = r#"{"name": "my-pkg"}"#;

        let entries =
            parse_node_dependency_entries(package_json).expect("package.json should parse");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_node_dependencies_rejects_malformed_json() {
        assert!(parse_node_dependency_entries("not json").is_err());
        assert!(parse_node_dependency_entries("{invalid").is_err());
    }

    #[test]
    fn parse_node_dependencies_rejects_traversal_package_names() {
        let package_json = r#"{
            "dependencies": {
                "../escape": "1.0.0"
            }
        }"#;

        assert!(parse_node_dependency_entries(package_json).is_err());
    }

    #[test]
    fn node_dependency_origin_labels_include_ecosystem() {
        assert_eq!(
            ProjectDependencyOrigin::node_dependencies().scan_label(),
            "node:dependencies"
        );
        assert_eq!(
            ProjectDependencyOrigin::node_dev_dependencies().detail_label(),
            "node development dependency"
        );
        assert_eq!(
            ProjectDependencyOrigin::node_optional_dependencies().scan_label(),
            "node:optionalDependencies"
        );
    }

    // ── Ecosystem matching and provenance ────────────────────────────

    fn make_skill(name: &str, pkg: &str, ver: &str, eco: Option<PackageEcosystem>) -> SkillData {
        SkillData {
            name: name.to_string(),
            description: String::new(),
            path: None,
            body: String::new(),
            raw: Vec::new(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools: None,
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: "dependency".to_string(),
            package_name: Some(pkg.to_string()),
            package_version: Some(ver.to_string()),
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem: eco,
        }
    }

    #[test]
    fn ecosystem_prevents_cross_ecosystem_matches() {
        let python_skill = make_skill(
            "lint",
            "ruff",
            "1.0.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
        );
        let node_skill = make_skill(
            "lint",
            "ruff",
            "1.0.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE)),
        );

        assert!(!python_skill.matches(&node_skill));
        assert!(!node_skill.matches(&python_skill));
    }

    #[test]
    fn ecosystem_same_type_allows_match() {
        let skill_a = make_skill(
            "lint",
            "ruff",
            "1.0.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
        );
        let skill_b = make_skill(
            "lint",
            "ruff",
            "2.0.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
        );

        assert!(skill_a.matches(&skill_b));
    }

    #[test]
    fn legacy_dependency_skill_without_explicit_ecosystem_infers_python() {
        // Skills parsed from SKILL.md without ecosystem metadata but with
        // source="dependency" get ecosystem=Python via the fallback in from_parsed.
        let mut metadata = BTreeMap::new();
        metadata.insert("skilly-source".to_string(), "dependency".to_string());

        let mut source_md = SkillSourceMetadata::new(
            None, // source comes from metadata
            Some("ruff"),
            Some("1.0.0"),
            None, // no explicit ecosystem
        );
        source_md.apply_missing_from_metadata(&metadata);

        // apply_missing_from_metadata does not set ecosystem for legacy (no key present)
        assert_eq!(source_md.package_ecosystem, None);

        // The or_else fallback in from_parsed would set Python:
        let source = source_md.resolved_source(&metadata);
        let inferred = source_md.package_ecosystem.or_else(|| {
            if source == "dependency" {
                Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON))
            } else {
                None
            }
        });
        assert_eq!(
            inferred,
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON))
        );
    }

    #[test]
    fn package_reference_formats_node_with_at_sign() {
        let node = make_skill(
            "lint",
            "@scope/my-pkg",
            "2.1.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE)),
        );
        assert_eq!(
            node.package_reference(),
            Some("@scope/my-pkg@2.1.0".to_string())
        );

        let unscoped = make_skill(
            "lint",
            "typescript",
            "5.0.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE)),
        );
        assert_eq!(
            unscoped.package_reference(),
            Some("typescript@5.0.0".to_string())
        );
    }

    #[test]
    fn package_reference_formats_python_with_double_equals() {
        let python = make_skill(
            "lint",
            "ruff",
            "0.12.0",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
        );
        assert_eq!(python.package_reference(), Some("ruff==0.12.0".to_string()));
    }

    #[test]
    fn package_reference_omits_version_when_empty() {
        let no_version = make_skill(
            "lint",
            "ruff",
            "",
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_PYTHON)),
        );
        assert_eq!(no_version.package_reference(), Some("ruff".to_string()));
    }

    #[test]
    fn source_metadata_persists_ecosystem_in_metadata_map() {
        let mut metadata = BTreeMap::new();
        let source_metadata = SkillSourceMetadata::new(
            Some("dependency"),
            Some("my-pkg"),
            Some("1.0.0"),
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE)),
        );

        source_metadata.insert_source_metadata(&mut metadata);

        assert_eq!(
            metadata.get("skilly-package-ecosystem"),
            Some(&"node".to_string())
        );
        assert_eq!(
            metadata.get("skilly-package-name"),
            Some(&"my-pkg".to_string())
        );
        assert_eq!(
            metadata.get("skilly-package-version"),
            Some(&"1.0.0".to_string())
        );
    }

    #[test]
    fn managed_metadata_marks_local_install_with_unknown_source() {
        let skill = SkillData {
            name: "local-skill".to_string(),
            description: "Local skill".to_string(),
            path: None,
            body: String::new(),
            raw: Vec::new(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools: None,
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: super::SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem: None,
        };

        let managed = skill.managed_metadata();
        assert_eq!(
            managed.get(super::SKILLY_SOURCE_METADATA_KEY),
            Some(&super::SKILLY_UNKNOWN_SOURCE.to_string())
        );
        assert!(super::has_managed_metadata(&managed));
    }

    #[test]
    fn ecosystem_metadata_round_trips_through_source_metadata() {
        let mut metadata = BTreeMap::new();
        metadata.insert("skilly-package-ecosystem".to_string(), "node".to_string());

        let mut source_md = SkillSourceMetadata::new(Some("dependency"), None, None, None);
        source_md.apply_missing_from_metadata(&metadata);

        assert_eq!(
            source_md.package_ecosystem,
            Some(PackageEcosystem::new(PACKAGE_ECOSYSTEM_NODE))
        );
    }

    #[test]
    fn scan_dependency_selection_includes_node_sections() {
        let selection = ScanDependencySelection {
            include_node_dependencies: true,
            include_node_dev_dependencies: false,
            include_node_optional_dependencies: true,
            ..Default::default()
        };

        assert!(selection.includes(&ProjectDependencyOrigin::node_dependencies()));
        assert!(!selection.includes(&ProjectDependencyOrigin::node_dev_dependencies()));
        assert!(selection.includes(&ProjectDependencyOrigin::node_optional_dependencies()));
    }

    #[test]
    fn scan_dependency_selection_controls_python_sections() {
        let selection = ScanDependencySelection {
            include_project_dependencies: true,
            dependency_groups: NamedSelection::Include(["dev".to_string()].into_iter().collect()),
            optional_dependencies: NamedSelection::Exclude(
                ["docs".to_string()].into_iter().collect(),
            ),
            include_node_dependencies: false,
            include_node_dev_dependencies: false,
            include_node_optional_dependencies: false,
        };

        assert!(selection.includes(&ProjectDependencyOrigin::python_project()));
        assert!(selection.includes(&ProjectDependencyOrigin::python_dependency_group("dev")));
        assert!(!selection.includes(&ProjectDependencyOrigin::python_dependency_group("test")));
        assert!(selection.includes(&ProjectDependencyOrigin::python_optional_dependency("lint")));
        assert!(!selection.includes(&ProjectDependencyOrigin::python_optional_dependency("docs")));
    }

    // --- Maven POM parsing tests ---

    use super::{
        MavenCoordinate, load_skills_from_archive, maven_jar_path, parse_maven_dependencies,
        validate_maven_coordinate,
    };

    const MINIMAL_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <groupId>com.example</groupId>
    <artifactId>demo</artifactId>
    <version>1.0.0</version>
    <dependencies>
        <dependency>
            <groupId>com.google.guava</groupId>
            <artifactId>guava</artifactId>
            <version>33.0.0</version>
        </dependency>
        <dependency>
            <groupId>org.slf4j</groupId>
            <artifactId>slf4j-api</artifactId>
            <version>2.0.0</version>
            <scope>compile</scope>
        </dependency>
    </dependencies>
</project>"#;

    #[test]
    fn parse_maven_dependencies_reads_direct_project_dependencies() {
        let (coords, warnings) = parse_maven_dependencies(MINIMAL_POM).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0].group_id, "com.google.guava");
        assert_eq!(coords[0].artifact_id, "guava");
        assert_eq!(coords[0].version, "33.0.0");
        assert_eq!(coords[0].scope, None);
        assert_eq!(coords[1].group_id, "org.slf4j");
        assert_eq!(coords[1].scope.as_deref(), Some("compile"));
    }

    #[test]
    fn parse_maven_dependencies_resolves_property_versions() {
        let pom = r#"<project>
            <properties>
                <guava.version>33.0.0</guava.version>
            </properties>
            <dependencies>
                <dependency>
                    <groupId>com.google.guava</groupId>
                    <artifactId>guava</artifactId>
                    <version>${guava.version}</version>
                </dependency>
            </dependencies>
        </project>"#;
        let (coords, warnings) = parse_maven_dependencies(pom).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(coords.len(), 1);
        assert_eq!(coords[0].version, "33.0.0");
    }

    #[test]
    fn parse_maven_dependencies_skips_dependency_management() {
        let pom = r#"<project>
            <dependencyManagement>
                <dependencies>
                    <dependency>
                        <groupId>managed</groupId>
                        <artifactId>lib</artifactId>
                        <version>1.0</version>
                    </dependency>
                </dependencies>
            </dependencyManagement>
            <dependencies>
                <dependency>
                    <groupId>direct</groupId>
                    <artifactId>lib</artifactId>
                    <version>2.0</version>
                </dependency>
            </dependencies>
        </project>"#;
        let (coords, _) = parse_maven_dependencies(pom).unwrap();
        assert_eq!(coords.len(), 1);
        assert_eq!(coords[0].group_id, "direct");
    }

    #[test]
    fn parse_maven_dependencies_rejects_malformed_xml() {
        // Truncated POM: unclosed <dependencies>
        let truncated = r#"<project><dependencies><dependency><groupId>g</groupId>"#;
        let result = parse_maven_dependencies(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn parse_maven_dependencies_skips_missing_version() {
        let pom = r#"<project>
            <dependencies>
                <dependency>
                    <groupId>com.example</groupId>
                    <artifactId>lib</artifactId>
                </dependency>
            </dependencies>
        </project>"#;
        let (coords, warnings) = parse_maven_dependencies(pom).unwrap();
        assert!(coords.is_empty());
        assert!(!warnings.is_empty());
    }

    // --- Maven coordinate validation ---

    #[test]
    fn validate_maven_coordinate_rejects_path_traversal() {
        assert!(validate_maven_coordinate("com.example", "lib", "../1.0").is_err());
        assert!(validate_maven_coordinate("com.example", "lib", "1.0/evil").is_err());
        assert!(validate_maven_coordinate("com..example", "lib", "1.0").is_err());
        assert!(validate_maven_coordinate("com.example", "lib..", "1.0").is_err());
    }

    #[test]
    fn validate_maven_coordinate_rejects_colons() {
        assert!(validate_maven_coordinate("com:example", "lib", "1.0").is_err());
        assert!(validate_maven_coordinate("com.example", "lib", "1:0").is_err());
    }

    #[test]
    fn validate_maven_coordinate_accepts_valid_ids() {
        assert!(validate_maven_coordinate("com.example.app", "my-lib", "1.0").is_ok());
        assert!(validate_maven_coordinate("org.slf4j", "slf4j-api", "2.0.0").is_ok());
        assert!(validate_maven_coordinate("com.google.code.gson", "gson", "2.11.0").is_ok());
    }

    #[test]
    fn validate_maven_coordinate_allows_dots_in_artifact_id() {
        assert!(validate_maven_coordinate("com.example", "my.lib", "1.0").is_ok());
        assert!(validate_maven_coordinate("com.example", "commons.lang3", "3.17.0").is_ok());
    }

    #[test]
    fn maven_jar_path_constructs_standard_layout() {
        let repo = std::path::Path::new("/home/user/.m2/repository");
        let coord = MavenCoordinate {
            group_id: "com.google.guava".to_string(),
            artifact_id: "guava".to_string(),
            version: "33.0.0".to_string(),
            scope: None,
        };
        let path = maven_jar_path(repo, &coord);
        assert_eq!(
            path,
            std::path::Path::new(
                "/home/user/.m2/repository/com/google/guava/guava/33.0.0/guava-33.0.0.jar"
            )
        );
    }

    // --- Archive loading tests ---

    fn make_zip_with_skill(name: &str) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let skill_md = format!("---\nname: {name}\ndescription: Test skill.\n---\nBody\n");
            zip.start_file(format!("skills/{name}/SKILL.md"), options)
                .unwrap();
            zip.write_all(skill_md.as_bytes()).unwrap();
            zip.start_file(format!("skills/{name}/scripts/run.py"), options)
                .unwrap();
            zip.write_all(b"print('hello')\n").unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn load_skills_from_archive_reads_valid_jar() {
        let archive = make_zip_with_skill("my-skill");
        let metadata = SkillSourceMetadata::default();
        let (skills, warnings) = load_skills_from_archive(&archive, &metadata).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].resources.len(), 1);
        assert_eq!(skills[0].resources[0].relative_path, "scripts/run.py");
        assert_eq!(skills[0].resources[0].raw, b"print('hello')\n");
    }

    #[test]
    fn load_skills_from_archive_rejects_path_traversal_entries() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("../escape/SKILL.md", options).unwrap();
            zip.write_all(b"---\nname: escape\ndescription: Test.\n---\nBody\n")
                .unwrap();
            zip.finish().unwrap();
        }
        let metadata = SkillSourceMetadata::default();
        let (skills, warnings) = load_skills_from_archive(&buf, &metadata).unwrap();
        assert!(skills.is_empty());
        assert!(!warnings.is_empty());
    }

    #[test]
    fn load_skills_from_archive_preserves_binary_resources() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("skills/bin-skill/SKILL.md", options)
                .unwrap();
            zip.write_all(b"---\nname: bin-skill\ndescription: Binary test.\n---\nBody\n")
                .unwrap();
            // Binary resource (non-UTF-8)
            zip.start_file("skills/bin-skill/assets/data.bin", options)
                .unwrap();
            zip.write_all(&[0x00, 0xFF, 0xFE, 0xFD]).unwrap();
            zip.finish().unwrap();
        }
        let metadata = SkillSourceMetadata::default();
        let (skills, warnings) = load_skills_from_archive(&buf, &metadata).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].resources.len(), 1);
        assert_eq!(skills[0].resources[0].raw, vec![0x00, 0xFF, 0xFE, 0xFD]);
    }

    #[test]
    fn from_bundle_preserves_input_bytes_without_filesystem_access() {
        let markdown =
            b"---\ndescription: In-memory bundle.\nname: in-memory\n---\nUse the bundle.\n";
        let resources = vec![SkillResourceData {
            relative_path: "scripts/run.py".to_string(),
            kind: "reference".to_string(),
            raw: b"print('not executed')\n".to_vec(),
        }];

        let skill = SkillData::from_bundle(markdown, resources).expect("valid bundle");

        assert_eq!(skill.raw, markdown);
        assert_eq!(skill.resources[0].kind, "reference");
        assert_eq!(skill.resources[0].raw, b"print('not executed')\n");
    }

    #[test]
    fn from_bundle_reports_structured_validation_errors() {
        let invalid_utf8 = SkillData::from_bundle(b"\xff", Vec::new()).expect_err("invalid UTF-8");
        assert_eq!(invalid_utf8.code, BundleValidationCode::InvalidUtf8);
        assert_eq!(invalid_utf8.code.as_str(), "invalid_utf8");
        assert_eq!(invalid_utf8.path, "SKILL.md");

        let unsafe_resource = SkillData::from_bundle(
            b"---\nname: safe-name\ndescription: Safe description.\n---\n",
            vec![SkillResourceData {
                relative_path: "references/../secret.txt".to_string(),
                kind: "other".to_string(),
                raw: Vec::new(),
            }],
        )
        .expect_err("unsafe resource path");
        assert_eq!(
            unsafe_resource.code,
            BundleValidationCode::InvalidResourcePath
        );
        assert_eq!(unsafe_resource.path, "references/../secret.txt");
    }

    #[test]
    fn load_skills_from_archive_removes_dual_layout_duplicates() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let skill_md = "---\nname: dup-skill\ndescription: Dual layout.\n---\nBody\n";
            // First layout
            zip.start_file(".agents/skills/dup-skill/SKILL.md", options)
                .unwrap();
            zip.write_all(skill_md.as_bytes()).unwrap();
            // Second layout (same skill name = ambiguous)
            zip.start_file("skills/dup-skill/SKILL.md", options)
                .unwrap();
            zip.write_all(skill_md.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let metadata = SkillSourceMetadata::default();
        let (skills, warnings) = load_skills_from_archive(&buf, &metadata).unwrap();
        assert_eq!(skills.len(), 0);
        assert!(!warnings.is_empty());
    }

    #[test]
    fn is_skill_record_rejects_traversal() {
        use super::is_skill_record;
        // Valid record
        assert!(is_skill_record(".agents/skills/test-skill/SKILL.md"));
        // Traversal attempt
        assert!(!is_skill_record("../outside/skills/escaped/SKILL.md"));
        // Empty segment
        assert!(!is_skill_record("skills//test/SKILL.md"));
        // Dot segment
        assert!(!is_skill_record("skills/./test/SKILL.md"));
    }

    #[test]
    fn repository_location_auto_detects_public_hosts() {
        let github = super::parse_repository_location(
            "https://github.com/example/skills/tree/main/review",
            None,
        )
        .expect("GitHub URL should parse");
        assert_eq!(github.provider, super::RepositoryProvider::GitHub);
        assert_eq!(github.namespace, "example");
        assert_eq!(github.path, "review");

        let cloud = super::parse_repository_location(
            "https://bitbucket.org/example/skills/src/main/review",
            None,
        )
        .expect("Bitbucket Cloud URL should parse");
        assert_eq!(cloud.provider, super::RepositoryProvider::BitbucketCloud);
        assert_eq!(cloud.namespace, "example");
        assert_eq!(cloud.r#ref.as_deref(), Some("main"));
        assert_eq!(cloud.path, "review");
    }

    struct StaticRepositorySnapshotFetcher {
        snapshot: RepositorySnapshotData,
    }

    impl RepositorySnapshotFetcher for StaticRepositorySnapshotFetcher {
        fn fetch_repository_snapshot(
            &self,
            _location: &RepositoryLocationData,
        ) -> anyhow::Result<RepositorySnapshotData> {
            Ok(self.snapshot.clone())
        }
    }

    #[test]
    fn repository_discovery_preserves_exact_skill_raw_bytes() {
        let skill_markdown = b"---\ndescription: Preserve source bytes exactly.\nname: remote-skill\n---\nSource instructions.\n";
        let mut files = BTreeMap::new();
        files.insert(
            "skills/remote-skill/SKILL.md".to_string(),
            RepositoryFileBlobData {
                path: "skills/remote-skill/SKILL.md".to_string(),
                content: skill_markdown.to_vec(),
                commit_sha: Some("1234567".to_string()),
            },
        );
        files.insert(
            "skills/remote-skill/scripts/run.py".to_string(),
            RepositoryFileBlobData {
                path: "skills/remote-skill/scripts/run.py".to_string(),
                content: b"print('ok')\n".to_vec(),
                commit_sha: Some("1234567".to_string()),
            },
        );
        let fetcher = StaticRepositorySnapshotFetcher {
            snapshot: RepositorySnapshotData {
                ref_name: "main".to_string(),
                commit_sha: "1234567".to_string(),
                files,
            },
        };

        let discovered = discover_repository_skills(
            &fetcher,
            "https://github.com/example/skills/tree/main/skills/remote-skill",
            None,
        )
        .expect("repository discovery should succeed");

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].raw, skill_markdown);
        assert_ne!(discovered[0].render(None).as_bytes(), skill_markdown);
        assert_eq!(discovered[0].resources[0].raw, b"print('ok')\n");
    }

    #[test]
    fn repository_location_requires_explicit_data_center_provider() {
        let error = super::parse_repository_location(
            "https://git.example.test/bitbucket/projects/SK/repos/skills/browse/review?at=refs/heads/main",
            None,
        )
        .expect_err("self-hosted Bitbucket must not be guessed");
        assert!(
            error
                .to_string()
                .contains("--provider bitbucket-data-center")
        );

        let location = super::parse_repository_location(
            "https://git.example.test/bitbucket/projects/SK/repos/skills/browse/review?at=refs/heads/main",
            Some(super::RepositoryProvider::BitbucketDataCenter),
        )
        .expect("explicit Data Center URL should parse");
        assert_eq!(location.base_url, "https://git.example.test/bitbucket");
        assert_eq!(location.namespace, "SK");
        assert_eq!(location.repo, "skills");
        assert_eq!(location.r#ref.as_deref(), Some("refs/heads/main"));
        assert_eq!(location.path, "review");
    }

    #[test]
    fn repository_location_rejects_provider_host_mismatch() {
        let error = super::parse_repository_location(
            "https://github.com/example/skills",
            Some(super::RepositoryProvider::BitbucketCloud),
        )
        .expect_err("mismatched provider must fail before transport");
        assert!(
            error
                .to_string()
                .contains("does not match explicit provider")
        );
    }
}
