pub(crate) mod args;
mod configure;
mod create;
mod download;
mod list;
mod operations;
mod presentation;
mod scan;
mod skillsmp;
#[cfg(test)]
mod tests;
pub(crate) mod tui;
mod update;
mod update_checks;
mod update_requests;
mod util;

use configure::*;
use create::*;
use download::*;
use list::*;
use operations::*;
use presentation::*;
use scan::*;
use skillsmp::*;
use update::*;
use update_requests::*;
use util::*;

use crate::cli::args::{
    Cli, Commands, CreateOptions, DestinationArgs, ResolvedDestination, SkillsMpSubcommand,
    UtilSubcommand, destination_tabs, first_non_empty_tab, next_non_empty_tab_index,
    next_tab_index, previous_non_empty_tab_index, previous_tab_index,
};
use crate::cli::tui::{
    DownloadableSkillMatch, InstalledSkillDiscoveryReport, InvalidInstalledSkill, ListedSkillEntry,
    MenuItemStatus, MenuItemUi, MenuUi, MultiSelectMenuResult, MultiSelectResult, SelectMenuResult,
    TerminalSession, is_interactive_terminal, multi_select_menu, multi_select_menu_with_tick,
    parse_metadata, run_configure_tui, run_create_tui, run_file_viewer, select_menu,
    select_menu_with_tick, show_loading_message,
};
use crate::cli::update_checks::{
    UpdateCheckKey, UpdateCheckProgress, UpdateCheckRequest, UpdateCheckState, UpdateChecks,
};
use crate::client::{ClientConfig, SkillsMpClient, SkillsMpSearchQuery, SkillsMpSkill};
use crate::config::{ProviderCredential, SkillyConfig};
use crate::core::{
    MavenSourceSettings, NodeSourceSettings, ProjectDependencyOrigin, ProjectEnvironment,
    ProjectSource, PythonSourceSettings, RepositoryLocationData, RepositoryProvider,
    SKILLY_SOURCE_REPOSITORY, SKILLY_UNKNOWN_SOURCE, STATUS_INSTALLABLE, STATUS_INSTALLED,
    STATUS_UPDATABLE, ScanDependencySelection, SkillData, SkillMatchData, SkillSourceMetadata,
    available_dependency_skill_in, available_repository_update as resolve_repository_update,
    discover_repository_skills, parse_repository_location, project_requirements, remove_skill,
    scan_match_status, scan_project_in,
};
use anyhow::{Context, Result, bail};
use clap::Parser;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const BACK_CHOICE: &str = "back";
#[allow(dead_code)]
pub(crate) const DELETE_CHOICE: &str = "delete";
pub(crate) const EXIT_CHOICE: &str = "exit";
pub(crate) const INSTALL_CHOICE: &str = "install";
pub(crate) const REMOVE_CHOICE: &str = "remove";
pub(crate) const UPDATE_CHOICE: &str = "update";
pub(crate) const VIEW_FILES_CHOICE: &str = "view files";
pub(crate) const APPLY_ALL_CHOICE: &str = "apply selected";
pub(crate) const INSTALL_ALL_CHOICE: &str = "install selected";
pub(crate) const UPDATE_ALL_CHOICE: &str = "update selected";
pub(crate) const REMOVE_ALL_CHOICE: &str = "remove selected";
const DEFAULT_HELP_TEXT: &str = "Up/Down move | Tab switch directory | Enter select | Esc cancel";
const MULTI_SELECT_HELP_TEXT: &str =
    "↑↓ move | Tab switch directory | Space select | A all | Enter action | Esc cancel";
const CONFIGURE_HINT: &str =
    "No directories configured. Run 'skilly configure' to set up directories.";
const DEFAULT_EMPTY_PREVIEW: &str = "No details available.";
pub(crate) const DEFAULT_CREATE_INSTRUCTIONS: &str =
    "# Instructions\n\nDescribe the procedure this skill should follow.";
pub(crate) const CREATE_HELP_TEXT: &str =
    "^S create | F2 create | ^X cancel | F10 cancel | Tab next field";
pub(crate) const LOADING_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
pub(crate) const LOADING_POLL_INTERVAL_MS: u64 = 120;

pub fn run(args: Vec<String>) -> i32 {
    match try_run(args) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn try_run(args: Vec<String>) -> Result<i32> {
    let cli = match Cli::try_parse_from(std::iter::once("skilly".to_string()).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            error.print()?;
            return Ok(if error.use_stderr() { 2 } else { 0 });
        }
    };

    let skilly_config = SkillyConfig::load()?;

    match cli.command {
        Commands::Create {
            name,
            description,
            instructions,
            license,
            compatibility,
            metadata,
            allowed_tools,
            with_scripts,
            with_references,
            with_assets,
            overwrite,
            yes: _yes,
            destination,
        } => run_create(
            &destination.resolve()?,
            CreateOptions {
                name,
                description,
                instructions,
                license,
                compatibility,
                metadata,
                allowed_tools,
                with_scripts,
                with_references,
                with_assets,
                overwrite,
            },
        )?,
        Commands::Scan {
            destination,
            dependencies,
        } => run_scan(&destination, dependencies.selection()?, &skilly_config)?,
        Commands::Download {
            repository_url,
            provider,
            destination,
            skill_name,
            all,
            overwrite,
            token,
        } => run_download(
            &DownloadRequest {
                repository_url,
                provider,
                skill_name,
                all,
                overwrite,
            },
            &destination,
            ClientConfig::new(None, None, None, None)
                .with_repository_token(token)
                .with_repository_credentials(skilly_config.repositories.providers.clone()),
            &skilly_config,
        )?,
        Commands::List { destination, token } => run_list(
            &destination,
            ClientConfig::new(None, None, None, None)
                .with_repository_token(token)
                .with_repository_credentials(skilly_config.repositories.providers.clone()),
            &skilly_config,
        )?,
        Commands::Update {
            destination,
            yes,
            token,
        } => run_update(
            &destination.resolve()?,
            ClientConfig::new(None, None, None, None)
                .with_repository_token(token)
                .with_repository_credentials(skilly_config.repositories.providers.clone()),
            yes,
        )?,
        Commands::Remove { name, destination } => {
            let removed = remove_skill(&name, &destination.resolve()?)?;
            println!("Removed {}", skill_directory_name(&removed));
        }
        Commands::Skillsmp(skillsmp) => match skillsmp.command {
            SkillsMpSubcommand::Search {
                query,
                destination,
                overwrite,
            } => run_skillsmp_search(
                &query,
                &destination,
                overwrite,
                ClientConfig::new(None, None, None, None)
                    .with_repository_credentials(skilly_config.repositories.providers.clone()),
                &skilly_config,
            )?,
        },
        Commands::Util(util) => match util.command {
            UtilSubcommand::Dependencies { file, dev, extras } => {
                for requirement in project_requirements(&file, dev, &extras)? {
                    if let Some(name) = crate::core::requirement_name(&requirement) {
                        println!("{name}");
                    }
                }
            }
            UtilSubcommand::Venv { path, detailed } => run_util_venv(&path, detailed)?,
        },
        Commands::Configure {
            show,
            reset,
            add_global,
            remove_global,
            add_local,
            remove_local,
            add_provider,
            provider_url,
            provider_token,
            remove_provider,
        } => run_configure(
            &skilly_config,
            ConfigureFlags {
                show,
                reset,
                add_global,
                remove_global,
                add_local,
                remove_local,
                add_provider,
                provider_url,
                provider_token,
                remove_provider,
            },
        )?,
    }

    Ok(0)
}
