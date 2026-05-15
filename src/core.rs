use anyhow::{Result, anyhow, bail};
use csv::ReaderBuilder;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;
use walkdir::WalkDir;

pub const DEFAULT_SKILLS_PATH: &str = ".agents/skills";
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

fn default_unknown_source() -> String {
    SKILLY_UNKNOWN_SOURCE.to_string()
}

fn resolve_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir()?.join(path))
}

fn normalize_skill_directory(path: Option<&Path>) -> Result<Option<PathBuf>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let resolved = resolve_path(path)?;
    let Some(name) = resolved.file_name().and_then(|value| value.to_str()) else {
        return Ok(Some(resolved));
    };
    if name.eq_ignore_ascii_case("SKILL.md") {
        return Ok(resolved.parent().map(Path::to_path_buf));
    }
    Ok(Some(resolved))
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

fn parse_frontmatter(lines: &[String]) -> Result<Mapping> {
    let parsed: YamlValue = serde_yaml::from_str(&lines.join("\n"))
        .map_err(|error| anyhow!("invalid YAML frontmatter: {error}"))?;
    match parsed {
        YamlValue::Null => Ok(Mapping::new()),
        YamlValue::Mapping(mapping) => Ok(mapping),
        _ => bail!("frontmatter must be a mapping"),
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

fn load_resource_files(skill_directory: &Path) -> (Vec<SkillResourceData>, Vec<String>) {
    if !skill_directory.is_dir() {
        return (Vec::new(), Vec::new());
    }

    let mut resources = Vec::new();
    let mut warnings = Vec::new();
    for entry in WalkDir::new(skill_directory)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative_path) = entry.path().strip_prefix(skill_directory) else {
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
        match fs::read_to_string(entry.path()) {
            Ok(content) => resources.push(SkillResourceData {
                relative_path: relative_path.clone(),
                kind: classify_resource_kind(&relative_path),
                content,
            }),
            Err(error) => warnings.push(format!(
                "{}: could not read bundled resource ({error})",
                entry.path().display()
            )),
        }
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

fn write_text_file(path: &Path, content: &str, overwrite: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() && !overwrite {
        bail!("Refusing to overwrite existing file: {}", path.display());
    }
    fs::write(path, content)?;
    Ok(())
}

fn find_skill_markdown_path(path: &Path) -> Result<PathBuf> {
    let directory = resolve_path(path)?;
    if !directory.is_dir() {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
    }
    let mut children = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let child_path = child.path();
        if child_path.is_file()
            && child_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("SKILL.md"))
                .unwrap_or(false)
        {
            return Ok(child_path);
        }
    }
    Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
}

impl SkillData {
    #[allow(clippy::too_many_arguments)]
    pub fn from_text_with_overrides(
        text: &str,
        path: Option<&Path>,
        source: Option<&str>,
        package_name: Option<&str>,
        package_version: Option<&str>,
        github_url: Option<&str>,
        github_commit_sha: Option<&str>,
        skillsmp_id: Option<&str>,
    ) -> Result<Self> {
        let skill_directory = normalize_skill_directory(path)?;
        let (frontmatter, body) = split_frontmatter(text)?;
        let parsed = parse_frontmatter(&frontmatter)?;

        let metadata = match mapping_get(&parsed, "metadata") {
            Some(YamlValue::Mapping(mapping)) => mapping
                .iter()
                .filter_map(|(key, value)| {
                    Some((yaml_scalar_to_string(key)?, yaml_scalar_to_string(value)?))
                })
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };

        let mut skill = Self {
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
            source: source
                .map(str::to_string)
                .unwrap_or_else(|| infer_source(&BTreeMap::new())),
            package_name: package_name.map(str::to_string),
            package_version: package_version.map(str::to_string),
            github_url: github_url.map(str::to_string),
            github_commit_sha: github_commit_sha.map(str::to_string),
            skillsmp_id: skillsmp_id.map(str::to_string),
        };

        if source.is_none() {
            skill.source = infer_source(&skill.metadata);
        }
        if skill.package_name.is_none() {
            skill.package_name = skill
                .metadata
                .get(SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY)
                .cloned();
        }
        if skill.package_version.is_none() {
            skill.package_version = skill
                .metadata
                .get(SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY)
                .cloned();
        }
        if skill.github_url.is_none() {
            skill.github_url = skill.metadata.get(SKILLY_GITHUB_URL_METADATA_KEY).cloned();
        }
        if skill.github_commit_sha.is_none() {
            skill.github_commit_sha = skill
                .metadata
                .get(SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY)
                .cloned();
        }
        if skill.skillsmp_id.is_none() {
            skill.skillsmp_id = skill.metadata.get(SKILLY_SKILLSMP_ID_METADATA_KEY).cloned();
        }

        if let Some(directory) = skill_directory.as_ref() {
            let (resources, warnings) = load_resource_files(directory);
            skill.resources = resources;
            skill.resource_warnings = warnings;
        }

        Ok(skill)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_file(
        path: &Path,
        source: Option<&str>,
        package_name: Option<&str>,
        package_version: Option<&str>,
        github_url: Option<&str>,
        github_commit_sha: Option<&str>,
        skillsmp_id: Option<&str>,
    ) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Self::from_text_with_overrides(
            &text,
            Some(path),
            source,
            package_name,
            package_version,
            github_url,
            github_commit_sha,
            skillsmp_id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_dir(
        path: &Path,
        source: Option<&str>,
        package_name: Option<&str>,
        package_version: Option<&str>,
        github_url: Option<&str>,
        github_commit_sha: Option<&str>,
        skillsmp_id: Option<&str>,
    ) -> Result<Self> {
        let skill_path = find_skill_markdown_path(path)?;
        Self::from_file(
            &skill_path,
            source,
            package_name,
            package_version,
            github_url,
            github_commit_sha,
            skillsmp_id,
        )
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
        let root = resolve_path(&directory.join(skill_name.unwrap_or(&self.name)))?;
        fs::create_dir_all(&root)?;
        write_text_file(
            &root.join("SKILL.md"),
            &self.render(Some(&self.managed_metadata())),
            overwrite,
        )?;
        for resource in &self.resources {
            let destination = root.join(PathBuf::from(&resource.relative_path));
            write_text_file(&destination, &resource.content, overwrite)?;
        }
        Self::from_dir(&root, None, None, None, None, None, None)
    }

    pub fn managed_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = self.metadata.clone();
        metadata.insert(
            SKILLY_MANAGED_METADATA_KEY.to_string(),
            SKILLY_MANAGED_METADATA_VALUE.to_string(),
        );
        if matches!(
            self.source.as_str(),
            SKILLY_SOURCE_DEPENDENCY | SKILLY_SOURCE_GITHUB | SKILLY_SOURCE_SKILLSMP
        ) {
            metadata.insert(SKILLY_SOURCE_METADATA_KEY.to_string(), self.source.clone());
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
        metadata
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

pub fn scan_match_status(available: &SkillData, installed: Option<&SkillData>) -> String {
    match installed {
        None => STATUS_INSTALLABLE.to_string(),
        Some(installed) if installed.package_version == available.package_version => {
            STATUS_INSTALLED.to_string()
        }
        Some(_) => STATUS_UPDATABLE.to_string(),
    }
}

pub fn discover_installed_skills(directory: &Path) -> Result<Vec<SkillData>> {
    let root = resolve_path(directory)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    if !root.is_dir() {
        bail!("{}", root.display());
    }
    let mut skills = Vec::new();
    let mut children = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let child_path = child.path();
        if !child_path.is_dir() {
            continue;
        }
        if let Ok(skill) = SkillData::from_dir(&child_path, None, None, None, None, None, None) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

pub fn remove_skill(name: &str, directory: &Path) -> Result<SkillData> {
    let skill = require_installed_skill(name, directory)?;
    let skill_directory = skill
        .path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("Installed skill has no directory: {name}"))?;
    fs::remove_dir_all(skill_directory)?;
    Ok(skill)
}

pub fn require_installed_skill(name: &str, directory: &Path) -> Result<SkillData> {
    let skills = discover_installed_skills(directory)?;
    for skill in &skills {
        let directory_name = skill
            .path
            .as_ref()
            .and_then(|path| Path::new(path).file_name().and_then(|value| value.to_str()))
            .unwrap_or(&skill.name);
        if directory_name == name {
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

pub fn find_site_packages_dir(venv_path: &Path) -> Option<PathBuf> {
    let windows_path = venv_path.join("Lib").join("site-packages");
    if windows_path.is_dir() {
        return Some(windows_path);
    }
    for lib_name in ["lib", "lib64"] {
        let lib_dir = venv_path.join(lib_name);
        if !lib_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&lib_dir) else {
            continue;
        };
        let mut children = entries.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name());
        children.reverse();
        for child in children {
            let child_path = child.path();
            let site_packages = child_path.join("site-packages");
            if child_path.is_dir()
                && child
                    .file_name()
                    .to_str()
                    .map(|name| name.starts_with("python"))
                    .unwrap_or(false)
                && site_packages.is_dir()
            {
                return Some(site_packages);
            }
        }
    }
    None
}

pub fn list_dist_info_dirs(site_packages: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(site_packages) else {
        return Vec::new();
    };
    let mut dirs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|value| value.ends_with(".dist-info"))
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

pub fn read_distribution_info(dist_info: &Path) -> Option<DistributionInfo> {
    let text = fs::read_to_string(dist_info.join("METADATA")).ok()?;
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Version:") {
            version = Some(rest.trim().to_string());
        }
    }
    Some(DistributionInfo {
        name: name?,
        version,
    })
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

pub fn discover_venv_skills(path: &Path) -> Result<Vec<SkillData>> {
    let venv_path = resolve_path(path)?;
    let Some(site_packages) = find_site_packages_dir(&venv_path) else {
        return Ok(Vec::new());
    };

    let mut skills = Vec::new();
    let mut seen_directories = BTreeSet::new();
    for dist_info in list_dist_info_dirs(&site_packages) {
        let Some(distribution) = read_distribution_info(&dist_info) else {
            continue;
        };
        let Ok(record_text) = fs::read_to_string(dist_info.join("RECORD")) else {
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
            if let Ok(skill) = SkillData::from_file(
                &skill_path,
                Some(SKILLY_SOURCE_DEPENDENCY),
                Some(&distribution.name),
                distribution.version.as_deref(),
                None,
                None,
                None,
            ) {
                skills.push(skill);
            }
        }
    }

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
    Ok(skills)
}

pub fn project_requirements(
    pyproject_toml_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<String>> {
    let text = fs::read_to_string(pyproject_toml_path)?;
    let parsed: toml::Value = text.parse()?;
    let mut dependencies = parsed
        .get("project")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut extras = include_extras.iter().cloned().collect::<BTreeSet<_>>();
    if include_dev {
        extras.insert("dev".to_string());
    }
    if let Some(groups) = parsed
        .get("dependency-groups")
        .and_then(|value| value.as_table())
    {
        for (group_name, values) in groups {
            if !extras.contains(group_name) {
                continue;
            }
            if let Some(values) = values.as_array() {
                dependencies.extend(
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string)),
                );
            }
        }
    }
    Ok(dependencies)
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

pub fn project_skills(
    pyproject_toml_path: &Path,
    venv_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<SkillData>> {
    let package_names = project_requirements(pyproject_toml_path, include_dev, include_extras)?
        .into_iter()
        .filter_map(|requirement| requirement_name(&requirement))
        .collect::<BTreeSet<_>>();
    Ok(discover_venv_skills(venv_path)?
        .into_iter()
        .filter(|skill| {
            skill
                .package_name
                .as_ref()
                .map(|package_name| package_names.contains(package_name))
                .unwrap_or(false)
        })
        .collect())
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

pub fn scan_project(
    directory: &Path,
    pyproject_toml_path: &Path,
    venv_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<SkillMatchData>> {
    let installed = discover_installed_skills(directory)?;
    let mut matches = project_skills(pyproject_toml_path, venv_path, include_dev, include_extras)?
        .into_iter()
        .map(|skill| SkillMatchData {
            installed: match_installed(&installed, &skill),
            available: skill,
        })
        .collect::<Vec<_>>();
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

pub fn dependency_updates(
    directory: &Path,
    pyproject_toml_path: &Path,
    venv_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Vec<SkillMatchData>> {
    Ok(scan_project(
        directory,
        pyproject_toml_path,
        venv_path,
        include_dev,
        include_extras,
    )?
    .into_iter()
    .filter(|item| scan_match_status(&item.available, item.installed.as_ref()) == STATUS_UPDATABLE)
    .collect())
}

pub fn available_dependency_skill(
    installed_skill: &SkillData,
    pyproject_toml_path: &Path,
    venv_path: &Path,
    include_dev: bool,
    include_extras: &[String],
) -> Result<Option<SkillData>> {
    Ok(
        project_skills(pyproject_toml_path, venv_path, include_dev, include_extras)?
            .into_iter()
            .find(|skill| skill.matches(installed_skill)),
    )
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
    let mut skill = SkillData::from_text_with_overrides(
        &skill_blob.content,
        Some(Path::new(skill_dir)),
        Some(source),
        None,
        None,
        github_url.as_deref(),
        github_commit_sha.as_deref(),
        skillsmp_id.as_deref(),
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
