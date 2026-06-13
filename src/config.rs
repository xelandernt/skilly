//! User configuration for skilly — manages `~/.skilly.toml` persistence and
//! the list of directories (tabs) that skilly commands operate across.
//!
//! The configuration stores two lists of directory paths: global (absolute or
//! `~`-prefixed) and local (relative to the current working directory).

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

const CONFIG_FILENAME: &str = ".skilly.toml";

/// Default global skill directories stored with `~` prefix for portability.
pub(crate) const DEFAULT_GLOBAL_DIRS: &[&str] = &["~/.agents/skills"];

/// Default local skill directories stored as relative paths.
pub(crate) const DEFAULT_LOCAL_DIRS: &[&str] = &[".agents/skills"];

/// All known global directories available as toggles in the configure TUI.
pub(crate) const KNOWN_GLOBAL_DIRS: &[&str] = &[
    "~/.agents/skills",
    "~/.claude/skills",
    "~/.codex/skills",
    "~/.copilot/skills",
];

/// All known local directories available as toggles in the configure TUI.
pub(crate) const KNOWN_LOCAL_DIRS: &[&str] = &[
    ".agents/skills",
    ".claude/skills",
    ".codex/skills",
    ".github/skills",
];

/// Persistent user configuration stored in `~/.skilly.toml`.
///
/// ```toml
/// default_directory = ".agents/skills"
///
/// [global]
/// directories = ["~/.agents/skills", "~/.claude/skills"]
///
/// [local]
/// directories = [".agents/skills", ".claude/skills"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SkillyConfig {
    /// The directory path that is opened by default in interactive menus.
    /// Must be present in either `global.directories` or `local.directories`.
    #[serde(default = "default_directory_path")]
    pub(crate) default_directory: String,

    #[serde(default)]
    pub(crate) global: GlobalConfig,

    #[serde(default)]
    pub(crate) local: LocalConfig,
}

fn default_directory_path() -> String {
    ".agents/skills".to_string()
}

impl Default for SkillyConfig {
    fn default() -> Self {
        Self {
            default_directory: default_directory_path(),
            global: GlobalConfig::default(),
            local: LocalConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GlobalConfig {
    #[serde(default = "default_global_dirs")]
    pub(crate) directories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LocalConfig {
    #[serde(default = "default_local_dirs")]
    pub(crate) directories: Vec<String>,
}

fn default_global_dirs() -> Vec<String> {
    DEFAULT_GLOBAL_DIRS.iter().map(|d| d.to_string()).collect()
}

fn default_local_dirs() -> Vec<String> {
    DEFAULT_LOCAL_DIRS.iter().map(|d| d.to_string()).collect()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            directories: default_global_dirs(),
        }
    }
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            directories: default_local_dirs(),
        }
    }
}

impl SkillyConfig {
    /// Resolve the path to the config file (`~/.skilly.toml`).
    pub(crate) fn config_path() -> Result<PathBuf> {
        let home = home_dir()?;
        Ok(home.join(CONFIG_FILENAME))
    }

    /// Load configuration from `~/.skilly.toml`. Returns the default
    /// configuration when the file does not exist.
    pub(crate) fn load() -> Result<Self> {
        let path = Self::config_path()?;
        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content)
                .with_context(|| format!("failed to parse configuration at {}", path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err)
                .with_context(|| format!("failed to read configuration at {}", path.display())),
        }
    }

    /// Write configuration to `~/.skilly.toml`, creating parent directories
    /// as needed.
    pub(crate) fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self).context("failed to serialize configuration")?;
        fs::write(&path, content)
            .with_context(|| format!("failed to write configuration to {}", path.display()))
    }

    /// Return `true` when no directories are configured.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.global.directories.is_empty() && self.local.directories.is_empty()
    }

    /// Check that the default directory is present in the configured directories.
    #[allow(dead_code)]
    pub(crate) fn validate_default_directory(&self) -> Result<()> {
        if self.global.directories.contains(&self.default_directory)
            || self.local.directories.contains(&self.default_directory)
        {
            return Ok(());
        }
        bail!(
            "default_directory '{}' is not in the configured directories. \
             Select a default directory before saving.",
            self.default_directory
        );
    }

    /// Set the default directory. Returns an error if the path is empty.
    #[allow(dead_code)]
    pub(crate) fn set_default_directory(&mut self, path: &str) -> Result<()> {
        if path.trim().is_empty() {
            bail!("default directory path must not be empty");
        }
        self.default_directory = path.trim().to_string();
        Ok(())
    }

    /// Validate that custom directory paths are well-formed:
    /// - global dirs must look like they can become absolute (with `~` expansion)
    /// - local dirs must be relative (not start with `/` or `~`)
    #[allow(dead_code)]
    pub(crate) fn validate_custom_dirs(&self) -> Result<()> {
        for dir in &self.global.directories {
            if dir.trim().is_empty() {
                bail!("global directory path must not be empty");
            }
            let stripped = dir.trim().trim_start_matches("~/");
            if stripped.is_empty() {
                bail!("global directory path must not be just '~'");
            }
        }
        for dir in &self.local.directories {
            if dir.trim().is_empty() {
                bail!("local directory path must not be empty");
            }
            if dir.starts_with('/') || dir.starts_with('~') {
                bail!("local directory must be a relative path (no leading / or ~): {dir}");
            }
        }
        Ok(())
    }

    /// Add a global directory path (absolute or `~`-prefixed).
    pub(crate) fn add_global_dir(&mut self, path: &str) -> Result<()> {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            bail!("global directory path must not be empty");
        }
        if !self.global.directories.contains(&trimmed) {
            self.global.directories.push(trimmed);
        }
        Ok(())
    }

    /// Remove a global directory path.
    pub(crate) fn remove_global_dir(&mut self, path: &str) -> Result<()> {
        let len_before = self.global.directories.len();
        self.global.directories.retain(|d| d.trim() != path.trim());
        if self.global.directories.len() == len_before {
            bail!("global directory not found: {path}");
        }
        Ok(())
    }

    /// Add a local directory path (relative).
    pub(crate) fn add_local_dir(&mut self, path: &str) -> Result<()> {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            bail!("local directory path must not be empty");
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            bail!("local directory must be a relative path (no leading / or ~): {trimmed}");
        }
        if !self.local.directories.contains(&trimmed) {
            self.local.directories.push(trimmed);
        }
        Ok(())
    }

    /// Remove a local directory path.
    pub(crate) fn remove_local_dir(&mut self, path: &str) -> Result<()> {
        let len_before = self.local.directories.len();
        self.local.directories.retain(|d| d.trim() != path.trim());
        if self.local.directories.len() == len_before {
            bail!("local directory not found: {path}");
        }
        Ok(())
    }
}

/// Shared home-directory resolution used by the config module and callers
/// that need the home path without a specific file.
pub(crate) fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("could not determine the user home directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove_test_config() {
        let home = home_dir().unwrap();
        let path = home.join(".skilly.test.toml");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn default_enables_all_dirs() {
        let config = SkillyConfig::default();
        assert_eq!(config.global.directories.len(), DEFAULT_GLOBAL_DIRS.len());
        assert_eq!(config.local.directories.len(), DEFAULT_LOCAL_DIRS.len());
        for dir in DEFAULT_GLOBAL_DIRS {
            assert!(
                config.global.directories.contains(&dir.to_string()),
                "missing {dir}"
            );
        }
        for dir in DEFAULT_LOCAL_DIRS {
            assert!(
                config.local.directories.contains(&dir.to_string()),
                "missing {dir}"
            );
        }
        assert!(!config.is_empty());
    }

    #[test]
    fn empty_config_is_empty() {
        let config = SkillyConfig {
            default_directory: String::new(),
            global: GlobalConfig {
                directories: Vec::new(),
            },
            local: LocalConfig {
                directories: Vec::new(),
            },
        };
        assert!(config.is_empty());
    }

    #[test]
    fn validate_custom_dirs_rejects_empty() {
        let mut config = SkillyConfig::default();
        config.global.directories.push("".to_string());
        assert!(config.validate_custom_dirs().is_err());

        config.global.directories.clear();
        config.local.directories.push("".to_string());
        assert!(config.validate_custom_dirs().is_err());
    }

    #[test]
    fn validate_custom_dirs_rejects_absolute_local() {
        let mut config = SkillyConfig::default();
        config.local.directories.push("/absolute/path".to_string());
        assert!(config.validate_custom_dirs().is_err());

        config.local.directories.clear();
        config.local.directories.push("~/tilde/path".to_string());
        assert!(config.validate_custom_dirs().is_err());
    }

    #[test]
    fn add_remove_global_dir() {
        let mut config = SkillyConfig {
            default_directory: String::new(),
            global: GlobalConfig {
                directories: Vec::new(),
            },
            local: LocalConfig {
                directories: Vec::new(),
            },
        };
        config.add_global_dir("/opt/skills").unwrap();
        assert!(
            config
                .global
                .directories
                .contains(&"/opt/skills".to_string())
        );
        // Add again is idempotent
        config.add_global_dir("/opt/skills").unwrap();
        assert_eq!(config.global.directories.len(), 1);
        config.remove_global_dir("/opt/skills").unwrap();
        assert!(config.global.directories.is_empty());
    }

    #[test]
    fn add_local_dir_rejects_absolute() {
        let mut config = SkillyConfig::default();
        assert!(config.add_local_dir("/bad").is_err());
        assert!(config.add_local_dir("~/bad").is_err());
        config.add_local_dir(".agents/skills").unwrap();
        assert!(
            config
                .local
                .directories
                .contains(&".agents/skills".to_string())
        );
    }

    #[test]
    fn remove_global_dir_not_found() {
        let mut config = SkillyConfig {
            default_directory: String::new(),
            global: GlobalConfig {
                directories: Vec::new(),
            },
            local: LocalConfig {
                directories: Vec::new(),
            },
        };
        assert!(config.remove_global_dir("/nonexistent").is_err());
    }

    #[test]
    fn save_and_load_roundtrip() {
        remove_test_config();
        let tmp = std::env::temp_dir().join("skilly-test-config.toml");
        let _ = fs::remove_file(&tmp);

        let mut config = SkillyConfig::default();
        config
            .global
            .directories
            .retain(|d| d != "~/.agents/skills");
        config.local.directories.retain(|d| d != ".agents/skills");
        config.add_global_dir("/opt/custom").unwrap();
        config.add_local_dir(".custom/skills").unwrap();

        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&tmp, &content).unwrap();

        let loaded: SkillyConfig = toml::from_str(&fs::read_to_string(&tmp).unwrap()).unwrap();
        assert_eq!(loaded.global.directories.len(), 1); // only /opt/custom
        assert!(
            loaded
                .global
                .directories
                .contains(&"/opt/custom".to_string())
        );
        assert_eq!(loaded.local.directories.len(), 1); // only .custom/skills
        assert!(
            loaded
                .local
                .directories
                .contains(&".custom/skills".to_string())
        );

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        remove_test_config();
        // Empty TOML string → serde default fills all fields
        let empty: SkillyConfig = toml::from_str("").unwrap();
        assert_eq!(empty.global.directories.len(), DEFAULT_GLOBAL_DIRS.len());
        assert_eq!(empty.local.directories.len(), DEFAULT_LOCAL_DIRS.len());
    }

    #[test]
    fn serde_default_populates_missing_sections() {
        // Only [global] section present — [local] gets its default
        let toml_str = r#"
[global]
directories = ["/opt/a"]
"#;
        let config: SkillyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.global.directories, vec!["/opt/a"]);
        assert_eq!(config.local.directories.len(), DEFAULT_LOCAL_DIRS.len());
    }

    #[test]
    fn serde_default_populates_missing_dirs_field() {
        // [global] section exists but no directories field
        let toml_str = r#"
[global]
"#;
        let config: SkillyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.global.directories.len(), DEFAULT_GLOBAL_DIRS.len());
    }

    #[test]
    fn remove_local_dir_not_found() {
        let mut config = SkillyConfig {
            default_directory: String::new(),
            global: GlobalConfig {
                directories: Vec::new(),
            },
            local: LocalConfig {
                directories: Vec::new(),
            },
        };
        assert!(config.remove_local_dir("/nonexistent").is_err());
    }
}
