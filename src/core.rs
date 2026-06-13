//! Domain models, validation, filesystem-independent operations, and core business logic
//! for skill discovery, installation, scanning, and update management.

use anyhow::{Context, Result, anyhow, bail};
use csv::ReaderBuilder;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

pub const DEFAULT_SKILLS_PATH: &str = ".agents/skills";
pub const CLAUDE_SKILLS_PATH: &str = ".claude/skills";
pub const CODEX_SKILLS_PATH: &str = ".codex/skills";
pub const COPILOT_LOCAL_SKILLS_PATH: &str = ".github/skills";
pub const COPILOT_GLOBAL_SKILLS_PATH: &str = ".copilot/skills";
pub const SKILLY_DIRECTORY_ENV_VAR: &str = "SKILLY_DIRECTORY";
pub const RESOURCE_KIND_SCRIPT: &str = "script";
pub const RESOURCE_KIND_REFERENCE: &str = "reference";
pub const RESOURCE_KIND_ASSET: &str = "asset";
pub const RESOURCE_KIND_OTHER: &str = "other";

pub const SKILLY_MANAGED_METADATA_KEY: &str = "skilly-managed-by";
pub const SKILLY_MANAGED_METADATA_VALUE: &str = "skilly";
pub const SKILLY_SOURCE_METADATA_KEY: &str = "skilly-source";
pub const SKILLY_SOURCE_DEPENDENCY: &str = "dependency";
pub const SKILLY_SOURCE_GITHUB: &str = "github";
pub const SKILLY_SOURCE_SKILLSMP: &str = "skillsmp";
pub const SKILLY_UNKNOWN_SOURCE: &str = "unknown";
pub const SKILLY_GITHUB_URL_METADATA_KEY: &str = "skilly-github-url";
pub const SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY: &str = "skilly-github-commit-sha";
pub const SKILLY_SKILLSMP_ID_METADATA_KEY: &str = "skilly-skillsmp-id";
pub const SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY: &str = "skilly-package-name";
pub const SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY: &str = "skilly-package-version";
pub const SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY: &str = "skilly-package-ecosystem";
pub const PACKAGE_ECOSYSTEM_PYTHON: &str = "python";
pub const PACKAGE_ECOSYSTEM_NODE: &str = "node";

pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_INSTALLABLE: &str = "installable";
pub const STATUS_UPDATABLE: &str = "updatable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PackageEcosystem {
    Python,
    Node,
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

/// Resolve the default skills directory, respecting the `SKILLY_DIRECTORY` env var.
pub fn default_skills_directory() -> Result<PathBuf> {
    if let Some(directory) = env::var_os(SKILLY_DIRECTORY_ENV_VAR) {
        return absolute_path(Path::new(&directory));
    }
    Ok(PathBuf::from(DEFAULT_SKILLS_PATH))
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
    #[serde(default)]
    pub content: String,
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

#[cfg(feature = "python-bindings")]
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubContentItemData {
    pub r#type: String,
    pub name: String,
    pub path: String,
    pub commit_sha: Option<String>,
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
    pub content: String,
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
    pub github_url: Option<String>,
    pub github_commit_sha: Option<String>,
    pub skillsmp_id: Option<String>,
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
    pub github_url: Option<String>,
    pub github_commit_sha: Option<String>,
    pub skillsmp_id: Option<String>,
    pub package_ecosystem: Option<PackageEcosystem>,
}

/// Which part of a project produced a given dependency requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ProjectDependencyOrigin {
    PythonProject,
    PythonDependencyGroup { group: String },
    PythonOptionalDependency { extra: String },
    NodeDependencies,
    NodeDevDependencies,
    NodeOptionalDependencies,
}

impl ProjectDependencyOrigin {
    #[must_use]
    pub fn scan_label(&self) -> String {
        match self {
            Self::PythonProject => "python:project".to_string(),
            Self::PythonDependencyGroup { group } => format!("python:group:{group}"),
            Self::PythonOptionalDependency { extra } => format!("python:extra:{extra}"),
            Self::NodeDependencies => "node:dependencies".to_string(),
            Self::NodeDevDependencies => "node:devDependencies".to_string(),
            Self::NodeOptionalDependencies => "node:optionalDependencies".to_string(),
        }
    }

    #[must_use]
    pub fn detail_label(&self) -> String {
        match self {
            Self::PythonProject => "python project dependency".to_string(),
            Self::PythonDependencyGroup { group } => format!("python dependency group: {group}"),
            Self::PythonOptionalDependency { extra } => {
                format!("python optional dependency: {extra}")
            }
            Self::NodeDependencies => "node runtime dependency".to_string(),
            Self::NodeDevDependencies => "node development dependency".to_string(),
            Self::NodeOptionalDependencies => "node optional dependency".to_string(),
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
        match origin {
            ProjectDependencyOrigin::PythonProject => self.include_project_dependencies,
            ProjectDependencyOrigin::PythonDependencyGroup { group } => {
                self.dependency_groups.includes(group)
            }
            ProjectDependencyOrigin::PythonOptionalDependency { extra } => {
                self.optional_dependencies.includes(extra)
            }
            ProjectDependencyOrigin::NodeDependencies => self.include_node_dependencies,
            ProjectDependencyOrigin::NodeDevDependencies => self.include_node_dev_dependencies,
            ProjectDependencyOrigin::NodeOptionalDependencies => {
                self.include_node_optional_dependencies
            }
        }
    }
}

impl SkillSourceMetadata {
    pub fn new(
        source: Option<&str>,
        package_name: Option<&str>,
        package_version: Option<&str>,
        github_url: Option<&str>,
        github_commit_sha: Option<&str>,
        skillsmp_id: Option<&str>,
        package_ecosystem: Option<PackageEcosystem>,
    ) -> Self {
        Self {
            source: source.map(str::to_string),
            package_name: package_name.map(str::to_string),
            package_version: package_version.map(str::to_string),
            github_url: github_url.map(str::to_string),
            github_commit_sha: github_commit_sha.map(str::to_string),
            skillsmp_id: skillsmp_id.map(str::to_string),
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
        if self.github_url.is_none() {
            self.github_url = metadata.get(SKILLY_GITHUB_URL_METADATA_KEY).cloned();
        }
        if self.github_commit_sha.is_none() {
            self.github_commit_sha = metadata.get(SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY).cloned();
        }
        if self.skillsmp_id.is_none() {
            self.skillsmp_id = metadata.get(SKILLY_SKILLSMP_ID_METADATA_KEY).cloned();
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

    fn insert_managed_metadata(&self, metadata: &mut BTreeMap<String, String>) {
        if let Some(source) = self.source.as_deref().filter(|source| {
            matches!(
                *source,
                SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_GITHUB | SKILLY_SOURCE_SKILLSMP
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
        if let Some(package_ecosystem) = self.package_ecosystem {
            metadata.insert(
                SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY.to_string(),
                match package_ecosystem {
                    PackageEcosystem::Python => PACKAGE_ECOSYSTEM_PYTHON.to_string(),
                    PackageEcosystem::Node => PACKAGE_ECOSYSTEM_NODE.to_string(),
                },
            );
        }
        if let Some(github_url) = self.github_url.as_ref() {
            metadata.insert(
                SKILLY_GITHUB_URL_METADATA_KEY.to_string(),
                github_url.clone(),
            );
        }
        if let Some(github_commit_sha) = self.github_commit_sha.as_ref() {
            metadata.insert(
                SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY.to_string(),
                github_commit_sha.clone(),
            );
        }
        if let Some(skillsmp_id) = self.skillsmp_id.as_ref() {
            metadata.insert(
                SKILLY_SKILLSMP_ID_METADATA_KEY.to_string(),
                skillsmp_id.clone(),
            );
        }
    }
}

/// Environment paths and dependency selection for a project scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEnvironment {
    pub directory: PathBuf,
    pub pyproject_toml_path: PathBuf,
    pub venv_path: PathBuf,
    pub package_json_path: PathBuf,
    pub node_modules_path: PathBuf,
    pub dependency_selection: ScanDependencySelection,
}

impl Default for ProjectEnvironment {
    fn default() -> Self {
        Self {
            directory: PathBuf::from(DEFAULT_SKILLS_PATH),
            pyproject_toml_path: PathBuf::from("pyproject.toml"),
            venv_path: PathBuf::from(".venv"),
            package_json_path: PathBuf::from("package.json"),
            node_modules_path: PathBuf::from("node_modules"),
            dependency_selection: ScanDependencySelection::default(),
        }
    }
}

impl ProjectEnvironment {
    #[must_use = "constructs a ProjectEnvironment; assign to use it"]
    pub fn with_paths(
        directory: &Path,
        pyproject_toml_path: &Path,
        venv_path: &Path,
        dependency_selection: ScanDependencySelection,
    ) -> Self {
        Self {
            directory: directory.to_path_buf(),
            pyproject_toml_path: pyproject_toml_path.to_path_buf(),
            venv_path: venv_path.to_path_buf(),
            package_json_path: PathBuf::from("package.json"),
            node_modules_path: PathBuf::from("node_modules"),
            dependency_selection,
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
pub trait GitHubSnapshotFetcher {
    fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData>;
}

/// Abstract filesystem for pluggable backends (native, in-memory, remote).
pub trait FileSystem {
    fn read_file(&self, path: &Path) -> Result<String>;
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;
    fn list_files(&self, path: &Path) -> Result<Vec<String>>;
    fn exists(&self, path: &Path) -> Result<bool>;
    fn is_dir(&self, path: &Path) -> Result<bool>;
    fn make_dir(&self, path: &Path, parents: bool, exist_ok: bool) -> Result<()>;
    fn remove_tree(&self, path: &Path) -> Result<()>;
    fn replace_tree(&self, path: &Path, replacement: &Path) -> Result<()>;
    fn resolve(&self, path: &Path) -> Result<PathBuf>;
}

/// Native (std::fs) filesystem implementation.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeFileSystem;

const NATIVE_FILE_SYSTEM: NativeFileSystem = NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn read_file(&self, path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(Into::into)
    }

    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
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
            if trimmed_value.is_empty()
                || !trimmed_value.contains(": ")
                || !should_quote_relaxed_yaml_scalar(trimmed_value)
            {
                return line.clone();
            }
            let indentation = suffix.len() - trimmed_value.len();
            format!(
                "{prefix}:{}{}",
                " ".repeat(indentation),
                format_scalar(trimmed_value)
            )
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
            SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_GITHUB | SKILLY_SOURCE_SKILLSMP
        )
    {
        return source.clone();
    }
    if metadata.contains_key(SKILLY_SKILLSMP_ID_METADATA_KEY) {
        return SKILLY_SOURCE_SKILLSMP.to_string();
    }
    if metadata.contains_key(SKILLY_GITHUB_URL_METADATA_KEY) {
        return SKILLY_SOURCE_GITHUB.to_string();
    }
    SKILLY_UNKNOWN_SOURCE.to_string()
}

fn infer_package_ecosystem(metadata: &BTreeMap<String, String>) -> Option<PackageEcosystem> {
    if let Some(eco) = metadata.get(SKILLY_PACKAGE_ECOSYSTEM_METADATA_KEY) {
        return match eco.as_str() {
            PACKAGE_ECOSYSTEM_PYTHON => Some(PackageEcosystem::Python),
            PACKAGE_ECOSYSTEM_NODE => Some(PackageEcosystem::Node),
            _ => None,
        };
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
        match file_system.read_file(&child_path) {
            Ok(content) => {
                let kind = classify_resource_kind(&relative_path);
                resources.push(SkillResourceData {
                    relative_path,
                    kind,
                    content,
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
    {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    } else {
        value.to_string()
    }
}

fn write_text_file_in(
    file_system: &dyn FileSystem,
    path: &Path,
    content: &str,
    overwrite: bool,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        file_system.make_dir(parent, true, true)?;
    }
    if file_system.exists(path)? && !overwrite {
        bail!("refusing to overwrite existing file: {}", path.display());
    }
    file_system.write_file(path, content)?;
    Ok(())
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
            write_text_file_in(file_system, &destination, &resource.content, overwrite)?;
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
        let metadata = frontmatter_metadata(&parsed);
        let mut source_metadata = source_metadata.clone();
        source_metadata.apply_missing_from_metadata(&metadata);
        let source = source_metadata.resolved_source(&metadata);
        let package_ecosystem = source_metadata.package_ecosystem.or_else(|| {
            if source == SKILLY_SOURCE_DEPENDENCY {
                Some(PackageEcosystem::Python)
            } else {
                None
            }
        });

        Ok(Self {
            name: required_string_field(&parsed, "name")?,
            description: required_string_field(&parsed, "description")?,
            path: skill_directory
                .as_ref()
                .map(|value| value.to_string_lossy().to_string()),
            content: body,
            license: optional_string_field(&parsed, "license"),
            compatibility: optional_string_field(&parsed, "compatibility"),
            metadata,
            allowed_tools: optional_string_field(&parsed, "allowed-tools"),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source,
            package_name: source_metadata.package_name.clone(),
            package_version: source_metadata.package_version.clone(),
            github_url: source_metadata.github_url.clone(),
            github_commit_sha: source_metadata.github_commit_sha.clone(),
            skillsmp_id: source_metadata.skillsmp_id.clone(),
            package_ecosystem,
        })
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
        let text = file_system.read_file(path)?;
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
            frontmatter.iter().map(|s| s.len() + 1).sum::<usize>() + self.content.len() + 8;
        let mut output = String::with_capacity(total_estimate);
        output.push_str("---\n");
        for line in &frontmatter {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("---\n");
        if !self.content.is_empty() {
            output.push_str(&self.content);
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

    /// Build the full metadata map including the managed-by marker and source tracking.
    #[must_use]
    pub fn managed_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = self.metadata.clone();
        metadata.insert(
            SKILLY_MANAGED_METADATA_KEY.to_string(),
            SKILLY_MANAGED_METADATA_VALUE.to_string(),
        );
        self.source_metadata()
            .insert_managed_metadata(&mut metadata);
        metadata
    }

    /// Extract source provenance as a [`SkillSourceMetadata`] struct.
    #[must_use]
    pub fn source_metadata(&self) -> SkillSourceMetadata {
        SkillSourceMetadata {
            source: Some(self.source.clone()),
            package_name: self.package_name.clone(),
            package_version: self.package_version.clone(),
            github_url: self.github_url.clone(),
            github_commit_sha: self.github_commit_sha.clone(),
            skillsmp_id: self.skillsmp_id.clone(),
            package_ecosystem: self.package_ecosystem,
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
            self.package_ecosystem,
        ) {
            (Some(name), Some(version), Some(PackageEcosystem::Node)) if !version.is_empty() => {
                Some(format!("{name}@{version}"))
            }
            (Some(name), Some(version), _) if !version.is_empty() => {
                Some(format!("{name}=={version}"))
            }
            (Some(name), _, _) => Some(name.clone()),
            _ => None,
        }
    }

    /// Check whether two skills refer to the same logical skill, matching by
    /// package name, GitHub URL, or name.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        if let (Some(package_name), Some(other_package_name)) =
            (&self.package_name, &other.package_name)
        {
            return (self.package_ecosystem, package_name, &self.name)
                == (other.package_ecosystem, other_package_name, &other.name);
        }
        if let (Some(github_url), Some(other_github_url)) = (&self.github_url, &other.github_url) {
            return github_url == other_github_url;
        }
        self.name == other.name
    }

    /// Returns `true` when the skill carries the skilly-managed metadata marker.
    #[must_use]
    #[inline]
    pub fn is_installed(&self) -> bool {
        self.metadata
            .get(SKILLY_MANAGED_METADATA_KEY)
            .map(|value| value == SKILLY_MANAGED_METADATA_VALUE)
            .unwrap_or(false)
    }

    /// Returns `true` when the skill source is a dependency install.
    #[must_use]
    #[inline]
    pub fn is_dependency(&self) -> bool {
        self.source == SKILLY_SOURCE_DEPENDENCY
    }

    /// Returns `true` when the skill is sourced from SkillsMP.
    #[must_use]
    #[inline]
    pub fn is_skillsmp(&self) -> bool {
        self.source == SKILLY_SOURCE_SKILLSMP || self.skillsmp_id.is_some()
    }

    /// Returns `true` when the skill has a known update source (dependency or GitHub).
    #[cfg(feature = "python-bindings")]
    #[allow(dead_code)]
    #[must_use]
    #[inline]
    pub fn can_update(&self) -> bool {
        self.is_dependency() || self.github_url.is_some()
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
    let text = match file_system.read_file(&dist_info.join("METADATA")) {
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
    for (index, part) in parts.iter().enumerate() {
        if *part == ".agents" && parts.len() > index + 3 {
            return parts[index + 1] == "skills" && parts.last() == Some(&"SKILL.md");
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
            left.package_ecosystem,
            left.package_name.as_deref().unwrap_or(""),
            left.package_version.as_deref().unwrap_or(""),
            left.name.as_str(),
        )
            .cmp(&(
                right.package_ecosystem,
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
        let Ok(record_text) = file_system.read_file(&dist_info.join("RECORD")) else {
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
                    None,
                    None,
                    None,
                    Some(PackageEcosystem::Python),
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
        ProjectDependencyOrigin::PythonProject,
    );

    if let Some(groups) = parsed
        .get("dependency-groups")
        .and_then(|value| value.as_table())
    {
        for (group_name, values) in groups {
            dependencies.extend(collect_project_requirement_values(
                Some(values),
                ProjectDependencyOrigin::PythonDependencyGroup {
                    group: group_name.clone(),
                },
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
                ProjectDependencyOrigin::PythonOptionalDependency {
                    extra: extra_name.clone(),
                },
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
            ProjectDependencyOrigin::PythonProject => true,
            ProjectDependencyOrigin::PythonDependencyGroup { group } => selected.contains(group),
            ProjectDependencyOrigin::PythonOptionalDependency { extra } => selected.contains(extra),
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
    selection: &ScanDependencySelection,
) -> Result<Vec<ProjectRequirementData>> {
    let text = file_system.read_file(pyproject_toml_path)?;
    Ok(filter_scan_requirement_entries(
        parse_project_requirement_entries(&text)?,
        selection,
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
    let text = file_system.read_file(pyproject_toml_path)?;
    parse_project_requirements(&text, include_dev, include_extras)
}

/// Extract the package name from a pip requirement spec.
#[must_use]
#[inline]
pub fn requirement_name(spec: &str) -> Option<String> {
    let trimmed = spec.trim_start();
    let mut name = String::new();
    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
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
                origin: ProjectDependencyOrigin::NodeDependencies,
            });
        }
    }

    if let Some(deps) = parsed.get("devDependencies").and_then(|v| v.as_object()) {
        for (name, _version) in deps {
            validate_node_package_name(name)?;
            dependencies.push(ProjectRequirementData {
                spec: name.to_string(),
                origin: ProjectDependencyOrigin::NodeDevDependencies,
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
                origin: ProjectDependencyOrigin::NodeOptionalDependencies,
            });
        }
    }

    Ok(dependencies)
}

fn scan_node_requirements_in(
    file_system: &dyn FileSystem,
    package_json_path: &Path,
    selection: &ScanDependencySelection,
) -> Result<Vec<ProjectRequirementData>> {
    if !file_system.exists(package_json_path)? {
        return Ok(Vec::new());
    }
    let text = file_system.read_file(package_json_path)?;
    Ok(filter_scan_requirement_entries(
        parse_node_dependency_entries(&text)?,
        selection,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodePackageInfo {
    name: String,
    version: String,
}

#[cfg(feature = "python-bindings")]
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
    let skills_dir = package_path.join("skills");
    if !file_system.is_dir(&skills_dir)? {
        return Ok(None);
    }

    let package_json_path = package_path.join("package.json");
    let Some(package_info) = read_node_package_json_in(file_system, &package_json_path)? else {
        return Ok(None);
    };

    let skill_dirs = file_system.list_files(&skills_dir)?;
    let mut skills = Vec::new();
    for skill_dir_name in skill_dirs {
        let skill_dir = skills_dir.join(&skill_dir_name);
        if !file_system.is_dir(&skill_dir)? {
            continue;
        }
        let skill = SkillData::from_dir_with_source_metadata_in(
            file_system,
            &skill_dir,
            &SkillSourceMetadata::new(
                Some(SKILLY_SOURCE_DEPENDENCY),
                Some(&package_info.name),
                Some(&package_info.version),
                None,
                None,
                None,
                Some(PackageEcosystem::Node),
            ),
        );
        match skill {
            Ok(skill) => skills.push(skill),
            Err(_) => continue,
        }
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
    let text = file_system.read_file(package_json_path)?;
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

fn project_skill_matches_in(
    file_system: &dyn FileSystem,
    environment: &ProjectEnvironment,
) -> Result<Vec<SkillMatchData>> {
    let installed = discover_installed_skills_in(file_system, &environment.directory)?;
    let mut matches = Vec::new();

    // Python ecosystem scanning
    if file_system
        .exists(&environment.pyproject_toml_path)
        .unwrap_or(false)
    {
        let requirements = scan_project_requirements_in(
            file_system,
            &environment.pyproject_toml_path,
            &environment.dependency_selection,
        )?;
        let origins_by_package = package_dependency_origins(&requirements);

        for skill in discover_venv_skills_in(file_system, &environment.venv_path)? {
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

    // Node ecosystem scanning
    if file_system
        .exists(&environment.package_json_path)
        .unwrap_or(false)
    {
        let node_requirements = scan_node_requirements_in(
            file_system,
            &environment.package_json_path,
            &environment.dependency_selection,
        )?;
        let node_origins_by_package = package_dependency_origins(&node_requirements);

        for skill in discover_node_modules_skills_in(file_system, &environment.node_modules_path)? {
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
            left.available.package_ecosystem,
            left.available.package_name.as_deref().unwrap_or(""),
            left.available.name.as_str(),
            left.available.package_version.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.available.package_ecosystem,
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

/// Build a GitHub tree URL from a location, path, and optional ref.
#[must_use]
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

pub fn build_skill_from_github_files(
    files: &BTreeMap<String, GitHubFileBlobData>,
    skill_dir: &str,
    source: &str,
    github_url: Option<String>,
    skillsmp_id: Option<String>,
) -> Result<SkillData> {
    let skill_md_path = if skill_dir == "." {
        "SKILL.md".to_string()
    } else {
        format!("{skill_dir}/SKILL.md")
    };
    let skill_blob = files
        .get(&skill_md_path)
        .ok_or_else(|| anyhow!("SKILL.md not found at {skill_dir}"))?;
    let github_commit_sha = skill_blob.commit_sha.clone().or_else(|| {
        files.iter().find_map(|(path, blob)| {
            (path == &skill_md_path
                || path == skill_dir
                || path.starts_with(&format!("{skill_dir}/")))
            .then(|| blob.commit_sha.clone())
            .flatten()
        })
    });
    let mut skill = SkillData::from_text(
        &skill_blob.content,
        None,
        &SkillSourceMetadata::new(
            Some(source),
            None,
            None,
            github_url.as_deref(),
            github_commit_sha.as_deref(),
            skillsmp_id.as_deref(),
            None,
        ),
    )?;
    let prefix = if skill_dir == "." {
        String::new()
    } else {
        format!("{skill_dir}/")
    };
    skill.resources = files
        .iter()
        .filter_map(|(path, blob)| {
            if path == &skill_md_path {
                return None;
            }
            if !prefix.is_empty() && !path.starts_with(&prefix) {
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
                content: blob.content.clone(),
            })
        })
        .collect();
    skill
        .resources
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(skill)
}

pub fn discover_github_skills<F: GitHubSnapshotFetcher>(
    fetcher: &F,
    github_url: &str,
    source: &str,
    skillsmp_id: Option<String>,
) -> Result<Vec<SkillData>> {
    let location = parse_github_skill_url(github_url)?;
    let snapshot = fetcher.fetch_github_snapshot(&location)?;
    let skill_dirs = find_github_skill_dirs(&snapshot.files, &location.path);
    let single_skill = skill_dirs.len() == 1;
    if skill_dirs.is_empty() {
        bail!("no SKILL.md found at {github_url}");
    }
    if skillsmp_id.is_some() && !single_skill {
        bail!("SkillsMP metadata can only be attached to a single skill");
    }

    skill_dirs
        .into_iter()
        .map(|skill_dir| {
            let at_requested_location = skill_dirs_len_eq_location(&location.path, &skill_dir);
            let skill_github_url = if single_skill && at_requested_location {
                github_url.to_string()
            } else {
                build_github_skill_url(&location, &skill_dir, Some(&snapshot.ref_name))
                    .unwrap_or_else(|| github_url.to_string())
            };
            build_skill_from_github_files(
                &snapshot.files,
                &skill_dir,
                source,
                Some(skill_github_url),
                if single_skill {
                    skillsmp_id.clone()
                } else {
                    None
                },
            )
        })
        .collect()
}

fn skill_dirs_len_eq_location(location_path: &str, skill_dir: &str) -> bool {
    (location_path == "." && skill_dir == ".") || location_path == skill_dir
}

/// Check whether installed and available GitHub skills share the same commit SHA.
#[must_use]
#[inline]
pub fn github_versions_match(installed: &SkillData, available: &SkillData) -> bool {
    installed.github_commit_sha.is_some()
        && installed.github_commit_sha == available.github_commit_sha
}

#[cfg(test)]
mod tests {
    use super::{
        NamedSelection, PackageEcosystem, ProjectDependencyOrigin, ScanDependencySelection,
        SkillData, SkillSourceMetadata, parse_node_dependency_entries,
        parse_project_requirement_entries, parse_project_requirements, validate_node_package_name,
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
            requirement.origin
                == ProjectDependencyOrigin::PythonDependencyGroup {
                    group: "dev".to_string(),
                }
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
            ("shared-lib", ProjectDependencyOrigin::NodeDependencies)
        );
        assert_eq!(
            specs[1],
            ("shared-lib", ProjectDependencyOrigin::NodeDevDependencies)
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
            ProjectDependencyOrigin::NodeDependencies.scan_label(),
            "node:dependencies"
        );
        assert_eq!(
            ProjectDependencyOrigin::NodeDevDependencies.detail_label(),
            "node development dependency"
        );
        assert_eq!(
            ProjectDependencyOrigin::NodeOptionalDependencies.scan_label(),
            "node:optionalDependencies"
        );
    }

    // ── Ecosystem matching and provenance ────────────────────────────

    fn make_skill(name: &str, pkg: &str, ver: &str, eco: Option<PackageEcosystem>) -> SkillData {
        SkillData {
            name: name.to_string(),
            description: String::new(),
            path: None,
            content: String::new(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools: None,
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: "dependency".to_string(),
            package_name: Some(pkg.to_string()),
            package_version: Some(ver.to_string()),
            github_url: None,
            github_commit_sha: None,
            skillsmp_id: None,
            package_ecosystem: eco,
        }
    }

    #[test]
    fn ecosystem_prevents_cross_ecosystem_matches() {
        let python_skill = make_skill("lint", "ruff", "1.0.0", Some(PackageEcosystem::Python));
        let node_skill = make_skill("lint", "ruff", "1.0.0", Some(PackageEcosystem::Node));

        assert!(!python_skill.matches(&node_skill));
        assert!(!node_skill.matches(&python_skill));
    }

    #[test]
    fn ecosystem_same_type_allows_match() {
        let skill_a = make_skill("lint", "ruff", "1.0.0", Some(PackageEcosystem::Python));
        let skill_b = make_skill("lint", "ruff", "2.0.0", Some(PackageEcosystem::Python));

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
            None,
            None,
            None,
            None, // no explicit ecosystem
        );
        source_md.apply_missing_from_metadata(&metadata);

        // apply_missing_from_metadata does not set ecosystem for legacy (no key present)
        assert_eq!(source_md.package_ecosystem, None);

        // The or_else fallback in from_parsed would set Python:
        let source = source_md.resolved_source(&metadata);
        let inferred = source_md.package_ecosystem.or_else(|| {
            if source == "dependency" {
                Some(PackageEcosystem::Python)
            } else {
                None
            }
        });
        assert_eq!(inferred, Some(PackageEcosystem::Python));
    }

    #[test]
    fn package_reference_formats_node_with_at_sign() {
        let node = make_skill(
            "lint",
            "@scope/my-pkg",
            "2.1.0",
            Some(PackageEcosystem::Node),
        );
        assert_eq!(
            node.package_reference(),
            Some("@scope/my-pkg@2.1.0".to_string())
        );

        let unscoped = make_skill("lint", "typescript", "5.0.0", Some(PackageEcosystem::Node));
        assert_eq!(
            unscoped.package_reference(),
            Some("typescript@5.0.0".to_string())
        );
    }

    #[test]
    fn package_reference_formats_python_with_double_equals() {
        let python = make_skill("lint", "ruff", "0.12.0", Some(PackageEcosystem::Python));
        assert_eq!(python.package_reference(), Some("ruff==0.12.0".to_string()));
    }

    #[test]
    fn package_reference_omits_version_when_empty() {
        let no_version = make_skill("lint", "ruff", "", Some(PackageEcosystem::Python));
        assert_eq!(no_version.package_reference(), Some("ruff".to_string()));
    }

    #[test]
    fn source_metadata_persists_ecosystem_in_metadata_map() {
        let mut metadata = BTreeMap::new();
        let source_metadata = SkillSourceMetadata::new(
            Some("dependency"),
            Some("my-pkg"),
            Some("1.0.0"),
            None,
            None,
            None,
            Some(PackageEcosystem::Node),
        );

        source_metadata.insert_managed_metadata(&mut metadata);

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
    fn ecosystem_metadata_round_trips_through_source_metadata() {
        let mut metadata = BTreeMap::new();
        metadata.insert("skilly-package-ecosystem".to_string(), "node".to_string());

        let mut source_md =
            SkillSourceMetadata::new(Some("dependency"), None, None, None, None, None, None);
        source_md.apply_missing_from_metadata(&metadata);

        assert_eq!(source_md.package_ecosystem, Some(PackageEcosystem::Node));
    }

    #[test]
    fn scan_dependency_selection_includes_node_sections() {
        let selection = ScanDependencySelection {
            include_node_dependencies: true,
            include_node_dev_dependencies: false,
            include_node_optional_dependencies: true,
            ..Default::default()
        };

        assert!(selection.includes(&ProjectDependencyOrigin::NodeDependencies));
        assert!(!selection.includes(&ProjectDependencyOrigin::NodeDevDependencies));
        assert!(selection.includes(&ProjectDependencyOrigin::NodeOptionalDependencies));
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

        assert!(selection.includes(&ProjectDependencyOrigin::PythonProject));
        assert!(
            selection.includes(&ProjectDependencyOrigin::PythonDependencyGroup {
                group: "dev".to_string()
            })
        );
        assert!(
            !selection.includes(&ProjectDependencyOrigin::PythonDependencyGroup {
                group: "test".to_string()
            })
        );
        assert!(
            selection.includes(&ProjectDependencyOrigin::PythonOptionalDependency {
                extra: "lint".to_string()
            })
        );
        assert!(
            !selection.includes(&ProjectDependencyOrigin::PythonOptionalDependency {
                extra: "docs".to_string()
            })
        );
    }
}
