use super::*;

pub(super) struct ConfigureFlags {
    pub(super) show: bool,
    pub(super) reset: bool,
    pub(super) add_global: Vec<String>,
    pub(super) remove_global: Vec<String>,
    pub(super) add_local: Vec<String>,
    pub(super) remove_local: Vec<String>,
    pub(super) add_provider: Option<RepositoryProvider>,
    pub(super) provider_url: Option<String>,
    pub(super) provider_token: Option<String>,
    pub(super) remove_provider: Option<RepositoryProvider>,
}

pub(super) fn run_configure(skilly_config: &SkillyConfig, flags: ConfigureFlags) -> Result<()> {
    if flags.reset {
        let default = SkillyConfig::default();
        default.save()?;
        println!("Configuration reset to defaults (saved to ~/.skilly.toml)");
        return Ok(());
    }

    let has_modifications = !flags.add_global.is_empty()
        || !flags.remove_global.is_empty()
        || !flags.add_local.is_empty()
        || !flags.remove_local.is_empty()
        || flags.add_provider.is_some()
        || flags.remove_provider.is_some();

    if flags.add_provider.is_some() && flags.remove_provider.is_some() {
        bail!("use either --add-provider or --remove-provider, not both");
    }

    let config_to_display = if has_modifications {
        let mut config = skilly_config.clone();
        for path in &flags.add_global {
            config.add_global_dir(path)?;
        }
        for path in &flags.remove_global {
            config.remove_global_dir(path)?;
        }
        for path in &flags.add_local {
            config.add_local_dir(path)?;
        }
        for path in &flags.remove_local {
            config.remove_local_dir(path)?;
        }
        if let Some(provider) = flags.add_provider {
            let url = flags
                .provider_url
                .as_deref()
                .context("--add-provider requires --provider-url")?;
            let token = flags
                .provider_token
                .as_deref()
                .context("--add-provider requires --provider-token")?;
            config.add_provider_credential(ProviderCredential::new(provider, url, token)?);
        }
        if let Some(provider) = flags.remove_provider {
            let url = flags
                .provider_url
                .as_deref()
                .context("--remove-provider requires --provider-url")?;
            config.remove_provider_credential(provider, url)?;
        }
        config.save()?;
        config
    } else {
        skilly_config.clone()
    };

    if flags.show {
        let content = config_to_display.display_toml()?;
        print!("{content}");
        if has_modifications {
            println!("Configuration updated (saved to ~/.skilly.toml)");
        }
        return Ok(());
    }

    if has_modifications {
        println!("Configuration updated (saved to ~/.skilly.toml)");
        return Ok(());
    }

    // Interactive terminal: launch TUI
    if is_interactive_terminal() {
        if let Some(config_path) = run_configure_tui(skilly_config)? {
            println!("Configuration saved to {}", config_path.display());
        }
        return Ok(());
    }

    // Non-interactive terminal with no flags: print current config
    let content = skilly_config.display_toml()?;
    print!("{content}");
    Ok(())
}
