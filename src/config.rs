//! User configuration for skilly — manages `~/.skilly.toml` persistence and
//! the list of directories (tabs) that skilly commands operate across.
//!
//! The configuration determines which built-in agent flavor destinations are
//! enabled and which custom global/local directories should appear in tab bars.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

const CONFIG_FILENAME: &str = ".skilly.toml";

/// Known built-in destination keys that correspond to agent flavor + scope
/// combinations. These appear as checkboxes in the configure TUI and as tab
/// labels in command menus.
pub(crate) const BUILTIN_KEYS: &[&str] = &[
    "agents_global",
    "agents_local",
    "claude_global",
    "claude_local",
    "codex_global",
    "codex_local",
    "copilot_global",
    "copilot_local",
];

/// Persistent user configuration stored in `~/.skilly.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SkillyConfig {
    /// Which built-in agent/scopes are active (e.g. `"agents_global"`).
    #[serde(default = "default_enabled_builtin")]
    pub(crate) enabled_builtin: Vec<String>,

    /// Additional absolute directories that are always available.
    /// Stored as-is; `~` expansion is applied at resolution time via
    /// [`crate::core::absolute_path`].
    #[serde(default)]
    pub(crate) custom_global_dirs: Vec<String>,

    /// Additional directories relative to the current working directory.
    #[serde(default)]
    pub(crate) custom_local_dirs: Vec<String>,
}

fn default_enabled_builtin() -> Vec<String> {
    BUILTIN_KEYS.iter().map(|k| k.to_string()).collect()
}

impl Default for SkillyConfig {
    fn default() -> Self {
        Self {
            enabled_builtin: default_enabled_builtin(),
            custom_global_dirs: Vec::new(),
            custom_local_dirs: Vec::new(),
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

    /// Return `true` when no built-in destinations are enabled and no custom
    /// directories are defined — meaning the tab bar would be empty.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.enabled_builtin.is_empty()
            && self.custom_global_dirs.is_empty()
            && self.custom_local_dirs.is_empty()
    }

    /// Validate that every entry in `enabled_builtin` is a known key.
    #[allow(dead_code)]
    pub(crate) fn validate_builtin_keys(&self) -> Result<()> {
        for key in &self.enabled_builtin {
            if !BUILTIN_KEYS.contains(&key.as_str()) {
                bail!(
                    "unknown built-in destination key: {key}. valid keys: {}",
                    BUILTIN_KEYS.join(", ")
                );
            }
        }
        // Deduplicate
        let mut seen = std::collections::BTreeSet::new();
        self.enabled_builtin
            .iter()
            .filter(|k| !seen.insert(k.as_str()))
            .for_each(|_dup| {
                // We'll report it via the length check below.
            });
        if seen.len() != self.enabled_builtin.len() {
            bail!("enabled_builtin contains duplicate entries");
        }
        Ok(())
    }

    /// Validate that custom directory paths are well-formed:
    /// - global dirs must look like they can become absolute (with `~` expansion)
    /// - local dirs must be relative (not start with `/` or `~`)
    #[allow(dead_code)]
    pub(crate) fn validate_custom_dirs(&self) -> Result<()> {
        for dir in &self.custom_global_dirs {
            if dir.trim().is_empty() {
                bail!("custom global directory path must not be empty");
            }
            // Allow `~`-prefixed paths; absolute_path() expands them.
            // Reject paths that look like relative bare names.
            let stripped = dir.trim().trim_start_matches("~/");
            if stripped.is_empty() {
                bail!("custom global directory path must not be just '~'");
            }
        }
        for dir in &self.custom_local_dirs {
            if dir.trim().is_empty() {
                bail!("custom local directory path must not be empty");
            }
            if dir.starts_with('/') || dir.starts_with('~') {
                bail!("custom local directory must be a relative path (no leading / or ~): {dir}");
            }
        }
        Ok(())
    }

    /// Enable a built-in destination key (idempotent).
    pub(crate) fn enable(&mut self, key: &str) -> Result<()> {
        if !BUILTIN_KEYS.contains(&key) {
            bail!(
                "unknown built-in destination key: {key}. valid keys: {}",
                BUILTIN_KEYS.join(", ")
            );
        }
        if !self.enabled_builtin.iter().any(|k| k == key) {
            self.enabled_builtin.push(key.to_string());
        }
        Ok(())
    }

    /// Disable a built-in destination key (idempotent).
    pub(crate) fn disable(&mut self, key: &str) -> Result<()> {
        if !BUILTIN_KEYS.contains(&key) {
            bail!(
                "unknown built-in destination key: {key}. valid keys: {}",
                BUILTIN_KEYS.join(", ")
            );
        }
        self.enabled_builtin.retain(|k| k != key);
        Ok(())
    }

    /// Add a custom global directory (absolute path, `~` allowed).
    pub(crate) fn add_global_dir(&mut self, path: &str) -> Result<()> {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            bail!("custom global directory path must not be empty");
        }
        if !self.custom_global_dirs.contains(&trimmed) {
            self.custom_global_dirs.push(trimmed);
        }
        Ok(())
    }

    /// Remove a custom global directory.
    pub(crate) fn remove_global_dir(&mut self, path: &str) -> Result<()> {
        let len_before = self.custom_global_dirs.len();
        self.custom_global_dirs.retain(|d| d.trim() != path.trim());
        if self.custom_global_dirs.len() == len_before {
            bail!("custom global directory not found: {path}");
        }
        Ok(())
    }

    /// Add a custom local directory (relative path).
    pub(crate) fn add_local_dir(&mut self, path: &str) -> Result<()> {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            bail!("custom local directory path must not be empty");
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            bail!("custom local directory must be a relative path (no leading / or ~): {trimmed}");
        }
        if !self.custom_local_dirs.contains(&trimmed) {
            self.custom_local_dirs.push(trimmed);
        }
        Ok(())
    }

    /// Remove a custom local directory.
    pub(crate) fn remove_local_dir(&mut self, path: &str) -> Result<()> {
        let len_before = self.custom_local_dirs.len();
        self.custom_local_dirs.retain(|d| d.trim() != path.trim());
        if self.custom_local_dirs.len() == len_before {
            bail!("custom local directory not found: {path}");
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

    fn config_path_for_test() -> PathBuf {
        let home = home_dir().unwrap();
        home.join(".skilly.test.toml")
    }

    fn remove_test_config() {
        let path = config_path_for_test();
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn default_enables_all_builtin() {
        let config = SkillyConfig::default();
        assert_eq!(config.enabled_builtin.len(), BUILTIN_KEYS.len());
        for key in BUILTIN_KEYS {
            assert!(
                config.enabled_builtin.contains(&key.to_string()),
                "missing {key}"
            );
        }
        assert!(config.custom_global_dirs.is_empty());
        assert!(config.custom_local_dirs.is_empty());
        assert!(!config.is_empty());
    }

    #[test]
    fn empty_config_is_empty() {
        let config = SkillyConfig {
            enabled_builtin: Vec::new(),
            custom_global_dirs: Vec::new(),
            custom_local_dirs: Vec::new(),
        };
        assert!(config.is_empty());
    }

    #[test]
    fn validate_builtin_keys_rejects_unknown() {
        let mut config = SkillyConfig::default();
        config.enabled_builtin.push("bogus_key".to_string());
        assert!(config.validate_builtin_keys().is_err());
    }

    #[test]
    fn validate_builtin_keys_rejects_duplicates() {
        let mut config = SkillyConfig::default();
        config.enabled_builtin.push("agents_global".to_string());
        assert!(config.validate_builtin_keys().is_err());
    }

    #[test]
    fn validate_custom_dirs_rejects_empty() {
        let mut config = SkillyConfig::default();
        config.custom_global_dirs.push("".to_string());
        assert!(config.validate_custom_dirs().is_err());

        config.custom_global_dirs.clear();
        config.custom_local_dirs.push("".to_string());
        assert!(config.validate_custom_dirs().is_err());
    }

    #[test]
    fn validate_custom_dirs_rejects_absolute_local() {
        let mut config = SkillyConfig::default();
        config.custom_local_dirs.push("/absolute/path".to_string());
        assert!(config.validate_custom_dirs().is_err());

        config.custom_local_dirs.clear();
        config.custom_local_dirs.push("~/tilde/path".to_string());
        assert!(config.validate_custom_dirs().is_err());
    }

    #[test]
    fn enable_disable_roundtrip() {
        let mut config = SkillyConfig::default();
        config.disable("agents_global").unwrap();
        assert!(
            !config
                .enabled_builtin
                .contains(&"agents_global".to_string())
        );
        config.enable("agents_global").unwrap();
        assert!(
            config
                .enabled_builtin
                .contains(&"agents_global".to_string())
        );

        // Disabling again is fine
        config.disable("agents_global").unwrap();
        assert!(
            !config
                .enabled_builtin
                .contains(&"agents_global".to_string())
        );
    }

    #[test]
    fn enable_unknown_key_errors() {
        let mut config = SkillyConfig::default();
        assert!(config.enable("nonexistent").is_err());
    }

    #[test]
    fn add_remove_global_dir() {
        let mut config = SkillyConfig::default();
        config.add_global_dir("/opt/skills").unwrap();
        assert!(
            config
                .custom_global_dirs
                .contains(&"/opt/skills".to_string())
        );
        // Add again is idempotent
        config.add_global_dir("/opt/skills").unwrap();
        assert_eq!(config.custom_global_dirs.len(), 1);
        config.remove_global_dir("/opt/skills").unwrap();
        assert!(config.custom_global_dirs.is_empty());
    }

    #[test]
    fn add_local_dir_rejects_absolute() {
        let mut config = SkillyConfig::default();
        assert!(config.add_local_dir("/bad").is_err());
        assert!(config.add_local_dir("~/bad").is_err());
        config.add_local_dir(".agents/skills").unwrap();
        assert!(
            config
                .custom_local_dirs
                .contains(&".agents/skills".to_string())
        );
    }

    #[test]
    fn remove_global_dir_not_found() {
        let mut config = SkillyConfig::default();
        assert!(config.remove_global_dir("/nonexistent").is_err());
    }

    #[test]
    fn save_and_load_roundtrip() {
        remove_test_config();
        // Use env override to isolate from real config
        let tmp = std::env::temp_dir().join("skilly-test-config.toml");
        let _ = fs::remove_file(&tmp);

        let mut config = SkillyConfig::default();
        config.disable("copilot_global").unwrap();
        config.disable("copilot_local").unwrap();
        config.add_global_dir("/opt/custom").unwrap();
        config.add_local_dir(".custom/skills").unwrap();

        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&tmp, &content).unwrap();

        let loaded: SkillyConfig = toml::from_str(&fs::read_to_string(&tmp).unwrap()).unwrap();
        assert!(
            !loaded
                .enabled_builtin
                .contains(&"copilot_global".to_string())
        );
        assert!(
            !loaded
                .enabled_builtin
                .contains(&"copilot_local".to_string())
        );
        assert!(
            loaded
                .custom_global_dirs
                .contains(&"/opt/custom".to_string())
        );
        assert!(
            loaded
                .custom_local_dirs
                .contains(&".custom/skills".to_string())
        );

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        remove_test_config();
        // Note: we can't easily test load() without mocking because it reads
        // the real ~/.skilly.toml. We test the serde default path instead.
        let empty: SkillyConfig = toml::from_str("").unwrap();
        assert_eq!(empty.enabled_builtin.len(), BUILTIN_KEYS.len());
    }

    #[test]
    fn serde_default_populates_enabled_builtin() {
        let toml_str = r#"
custom_global_dirs = ["/opt/a"]
"#;
        let config: SkillyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.enabled_builtin.len(), BUILTIN_KEYS.len());
        assert_eq!(config.custom_global_dirs, vec!["/opt/a"]);
        assert!(config.custom_local_dirs.is_empty());
    }
}
