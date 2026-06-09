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

pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_INSTALLABLE: &str = "installable";
pub const STATUS_UPDATABLE: &str = "updatable";

static TEMPORARY_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SkillDirectoryFlavor {
    #[default]
    Agents,
    Claude,
    Codex,
    Copilot,
}

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
        .ok_or_else(|| anyhow!("Could not determine the user home directory"))?;
    Ok(home.join(relative))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillResourceData {
    pub relative_path: String,
    pub kind: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubSkillLocationData {
    pub owner: String,
    pub repo: String,
    pub r#ref: Option<String>,
    pub path: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubContentItemData {
    pub r#type: String,
    pub name: String,
    pub path: String,
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubFileBlobData {
    pub path: String,
    pub content: String,
    pub size: usize,
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubRepositorySnapshotData {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub commit_sha: String,
    pub files: BTreeMap<String, GitHubFileBlobData>,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMatchData {
    pub available: SkillData,
    pub installed: Option<SkillData>,
    #[serde(default)]
    pub dependency_origins: Vec<ProjectDependencyOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkillSourceMetadata {
    pub source: Option<String>,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
    pub github_url: Option<String>,
    pub github_commit_sha: Option<String>,
    pub skillsmp_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectDependencyOrigin {
    Project,
    DependencyGroup { group: String },
    OptionalDependency { extra: String },
}

impl ProjectDependencyOrigin {
    pub fn scan_label(&self) -> String {
        match self {
            Self::Project => "project".to_string(),
            Self::DependencyGroup { group } => format!("group:{group}"),
            Self::OptionalDependency { extra } => format!("extra:{extra}"),
        }
    }

    pub fn detail_label(&self) -> String {
        match self {
            Self::Project => "project dependency".to_string(),
            Self::DependencyGroup { group } => format!("dependency group: {group}"),
            Self::OptionalDependency { extra } => format!("optional dependency: {extra}"),
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
    pub include_dependency_groups: bool,
    pub include_optional_dependencies: bool,
}

impl Default for ScanDependencySelection {
    fn default() -> Self {
        Self {
            include_project_dependencies: true,
            include_dependency_groups: true,
            include_optional_dependencies: true,
        }
    }
}

impl ScanDependencySelection {
    fn includes(&self, origin: &ProjectDependencyOrigin) -> bool {
        match origin {
            ProjectDependencyOrigin::Project => self.include_project_dependencies,
            ProjectDependencyOrigin::DependencyGroup { .. } => self.include_dependency_groups,
            ProjectDependencyOrigin::OptionalDependency { .. } => {
                self.include_optional_dependencies
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
    ) -> Self {
        Self {
            source: source.map(str::to_string),
            package_name: package_name.map(str::to_string),
            package_version: package_version.map(str::to_string),
            github_url: github_url.map(str::to_string),
            github_commit_sha: github_commit_sha.map(str::to_string),
            skillsmp_id: skillsmp_id.map(str::to_string),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEnvironment {
    pub directory: PathBuf,
    pub pyproject_toml_path: PathBuf,
    pub venv_path: PathBuf,
    pub dependency_selection: ScanDependencySelection,
}

impl Default for ProjectEnvironment {
    fn default() -> Self {
        Self {
            directory: PathBuf::from(DEFAULT_SKILLS_PATH),
            pyproject_toml_path: PathBuf::from("pyproject.toml"),
            venv_path: PathBuf::from(".venv"),
            dependency_selection: ScanDependencySelection::default(),
        }
    }
}

impl ProjectEnvironment {
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
            dependency_selection,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionInfo {
    pub name: String,
    pub version: Option<String>,
}

pub trait GitHubSnapshotFetcher {
    fn fetch_github_snapshot(
        &self,
        location: &GitHubSkillLocationData,
    ) -> Result<GitHubRepositorySnapshotData>;
}

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
        Ok(fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect())
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
                bail!("File exists: {}", path.display());
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
    if let Some(source) = metadata.get(SKILLY_SOURCE_METADATA_KEY) {
        if matches!(
            source.as_str(),
            SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_GITHUB | SKILLY_SOURCE_SKILLSMP
        ) {
            return source.clone();
        }
    }
    if metadata.contains_key(SKILLY_SKILLSMP_ID_METADATA_KEY) {
        return SKILLY_SOURCE_SKILLSMP.to_string();
    }
    if metadata.contains_key(SKILLY_GITHUB_URL_METADATA_KEY) {
        return SKILLY_SOURCE_GITHUB.to_string();
    }
    SKILLY_UNKNOWN_SOURCE.to_string()
}

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
            Ok(content) => resources.push(SkillResourceData {
                relative_path: relative_path.clone(),
                kind: classify_resource_kind(&relative_path),
                content,
            }),
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
        bail!("Refusing to overwrite existing file: {}", path.display());
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
            "Invalid skill name {name:?}: use 1-64 lowercase letters, numbers, and single hyphens"
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
        bail!("Invalid relative resource path: {path}")
    }
}

fn validate_install_paths(skill: &SkillData, skill_name: Option<&str>) -> Result<()> {
    skill.validate()?;
    validate_skill_name(skill_name.unwrap_or(&skill.name))?;
    let mut seen = BTreeSet::new();
    for resource in &skill.resources {
        validate_resource_path(&resource.relative_path)?;
        if !seen.insert(resource.relative_path.to_ascii_lowercase()) {
            bail!("Duplicate resource path: {}", resource.relative_path);
        }
    }
    Ok(())
}

fn temporary_sibling(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Skill destination has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("Skill destination has no valid name: {}", path.display()))?;
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
        let _ = fs::rename(&backup, path);
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
    pub fn validate(&self) -> Result<()> {
        validate_skill_name(&self.name)?;
        if self.description.is_empty() || self.description.len() > 1024 {
            bail!("Skill description must contain 1-1024 characters");
        }
        if self
            .compatibility
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > 500)
        {
            bail!("Skill compatibility must contain 1-500 characters when provided");
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
        })
    }

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

    pub fn from_text(
        text: &str,
        path: Option<&Path>,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_text_in(&NATIVE_FILE_SYSTEM, text, path, source_metadata)
    }

    pub fn from_file_with_source_metadata(
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_file_with_source_metadata_in(&NATIVE_FILE_SYSTEM, path, source_metadata)
    }

    pub fn from_file_with_source_metadata_in(
        file_system: &dyn FileSystem,
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let text = file_system.read_file(path)?;
        Self::from_text_in(file_system, &text, Some(path), source_metadata)
    }

    pub fn from_dir_with_source_metadata(
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        Self::from_dir_with_source_metadata_in(&NATIVE_FILE_SYSTEM, path, source_metadata)
    }

    pub fn from_dir_with_source_metadata_in(
        file_system: &dyn FileSystem,
        path: &Path,
        source_metadata: &SkillSourceMetadata,
    ) -> Result<Self> {
        let skill_path = find_skill_markdown_path_in(file_system, path)?;
        Self::from_file_with_source_metadata_in(file_system, &skill_path, source_metadata)
    }

    pub fn render(&self, metadata_override: Option<&BTreeMap<String, String>>) -> String {
        let mut combined_metadata = self.metadata.clone();
        if let Some(metadata_override) = metadata_override {
            for (key, value) in metadata_override {
                combined_metadata.insert(key.clone(), value.clone());
            }
        }

        let mut frontmatter = vec![
            format!("name: {}", format_scalar(&self.name)),
            format!("description: {}", format_scalar(&self.description)),
        ];
        if let Some(license) = self.license.as_ref() {
            frontmatter.push(format!("license: {}", format_scalar(license)));
        }
        if let Some(compatibility) = self.compatibility.as_ref() {
            frontmatter.push(format!("compatibility: {}", format_scalar(compatibility)));
        }
        if let Some(allowed_tools) = self.allowed_tools.as_ref() {
            frontmatter.push(format!("allowed-tools: {}", format_scalar(allowed_tools)));
        }
        if !combined_metadata.is_empty() {
            frontmatter.push("metadata:".to_string());
            for (key, value) in combined_metadata {
                frontmatter.push(format!("  {key}: {}", format_scalar(&value)));
            }
        }

        let header = std::iter::once("---".to_string())
            .chain(frontmatter)
            .chain(std::iter::once("---".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        if self.content.is_empty() {
            format!("{header}\n")
        } else {
            format!("{header}\n{}", self.content)
        }
    }

    pub fn install_to(
        &self,
        directory: &Path,
        skill_name: Option<&str>,
        overwrite: bool,
    ) -> Result<Self> {
        self.install_to_in(&NATIVE_FILE_SYSTEM, directory, skill_name, overwrite)
    }

    pub fn replace_to(&self, directory: &Path, skill_name: Option<&str>) -> Result<Self> {
        self.replace_to_in(&NATIVE_FILE_SYSTEM, directory, skill_name)
    }

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
            if matches!(file_system.exists(&replacement), Ok(true)) {
                let _ = file_system.remove_tree(&replacement);
            }
            return Err(error);
        }
        file_system.replace_tree(&root, &replacement)?;
        Self::from_dir_with_source_metadata_in(file_system, &root, &SkillSourceMetadata::default())
    }

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

    pub fn source_metadata(&self) -> SkillSourceMetadata {
        SkillSourceMetadata {
            source: Some(self.source.clone()),
            package_name: self.package_name.clone(),
            package_version: self.package_version.clone(),
            github_url: self.github_url.clone(),
            github_commit_sha: self.github_commit_sha.clone(),
            skillsmp_id: self.skillsmp_id.clone(),
        }
    }

    pub fn directory_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(|path| Path::new(path).file_name().and_then(|value| value.to_str()))
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.name)
            .to_string()
    }

    pub fn package_reference(&self) -> Option<String> {
        match (&self.package_name, &self.package_version) {
            (Some(name), Some(version)) if !version.is_empty() => {
                Some(format!("{name}=={version}"))
            }
            (Some(name), _) => Some(name.clone()),
            _ => None,
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        if let (Some(package_name), Some(other_package_name)) =
            (&self.package_name, &other.package_name)
        {
            return (package_name, &self.name) == (other_package_name, &other.name);
        }
        if let (Some(github_url), Some(other_github_url)) = (&self.github_url, &other.github_url) {
            return github_url == other_github_url;
        }
        self.name == other.name
    }

    pub fn is_installed(&self) -> bool {
        self.metadata
            .get(SKILLY_MANAGED_METADATA_KEY)
            .map(|value| value == SKILLY_MANAGED_METADATA_VALUE)
            .unwrap_or(false)
    }

    pub fn is_dependency(&self) -> bool {
        self.source == SKILLY_SOURCE_DEPENDENCY
    }

    pub fn is_skillsmp(&self) -> bool {
        self.source == SKILLY_SOURCE_SKILLSMP || self.skillsmp_id.is_some()
    }

    pub fn can_update(&self) -> bool {
        self.is_dependency() || self.github_url.is_some()
    }
}

pub fn scan_match_status(available: &SkillData, installed: Option<&SkillData>) -> &'static str {
    match installed {
        None => STATUS_INSTALLABLE,
        Some(installed) if installed.package_version == available.package_version => {
            STATUS_INSTALLED
        }
        Some(_) => STATUS_UPDATABLE,
    }
}

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
        .ok_or_else(|| anyhow!("Installed skill has no directory: {name}"))?;
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
        .collect::<Vec<_>>();
    match matches.len() {
        0 => bail!("Installed skill not found: {name}"),
        1 => Ok(matches[0].clone()),
        _ => bail!("Multiple installed skills match name: {name}"),
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
            left.package_name.as_deref().unwrap_or(""),
            left.package_version.as_deref().unwrap_or(""),
            left.name.as_str(),
        )
            .cmp(&(
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
        ProjectDependencyOrigin::Project,
    );

    if let Some(groups) = parsed
        .get("dependency-groups")
        .and_then(|value| value.as_table())
    {
        for (group_name, values) in groups {
            dependencies.extend(collect_project_requirement_values(
                Some(values),
                ProjectDependencyOrigin::DependencyGroup {
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
                ProjectDependencyOrigin::OptionalDependency {
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
            ProjectDependencyOrigin::Project => true,
            ProjectDependencyOrigin::DependencyGroup { group } => selected.contains(group),
            ProjectDependencyOrigin::OptionalDependency { extra } => selected.contains(extra),
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

fn project_skill_matches_in(
    file_system: &dyn FileSystem,
    environment: &ProjectEnvironment,
) -> Result<Vec<SkillMatchData>> {
    let installed = discover_installed_skills_in(file_system, &environment.directory)?;
    let requirements = scan_project_requirements_in(
        file_system,
        &environment.pyproject_toml_path,
        &environment.dependency_selection,
    )?;
    let origins_by_package = package_dependency_origins(&requirements);

    Ok(
        discover_venv_skills_in(file_system, &environment.venv_path)?
            .into_iter()
            .filter_map(|skill| {
                let package_name = skill.package_name.as_ref()?;
                let dependency_origins = origins_by_package.get(package_name)?.clone();
                Some(SkillMatchData {
                    installed: match_installed(&installed, &skill),
                    available: skill,
                    dependency_origins,
                })
            })
            .collect(),
    )
}

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
            left.available.package_name.as_deref().unwrap_or(""),
            left.available.name.as_str(),
            left.available.package_version.as_deref().unwrap_or(""),
        )
            .cmp(&(
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

pub fn parse_github_skill_url(github_url: &str) -> Result<GitHubSkillLocationData> {
    let parsed = Url::parse(github_url)?;
    if parsed.host_str() != Some("github.com") {
        bail!(
            "Unsupported GitHub URL host: {}",
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
                "GitHub skill URLs must look like https://github.com/<owner>/<repo> or https://github.com/<owner>/<repo>/tree/<ref>/<path>"
            );
        }
        if parts.len() < 4 {
            bail!(
                "GitHub tree URLs must include a ref like https://github.com/<owner>/<repo>/tree/<ref>"
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
        bail!("No SKILL.md found at {github_url}");
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

pub fn github_versions_match(installed: &SkillData, available: &SkillData) -> bool {
    installed.github_commit_sha.is_some()
        && installed.github_commit_sha == available.github_commit_sha
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectDependencyOrigin, ScanDependencySelection, parse_project_requirement_entries,
        parse_project_requirements,
    };

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
                    include_dependency_groups: true,
                    include_optional_dependencies: false,
                }
                .includes(&requirement.origin)
            })
            .collect::<Vec<_>>();

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|requirement| {
            requirement.origin
                == ProjectDependencyOrigin::DependencyGroup {
                    group: "dev".to_string(),
                }
        }));
    }
}
