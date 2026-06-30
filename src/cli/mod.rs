pub(crate) mod args;
pub(crate) mod tui;

use crate::cli::args::{
    Cli, Commands, CreateOptions, DestinationArgs, ResolvedDestination, SkillsMpSubcommand,
    UtilSubcommand, destination_tabs, first_non_empty_tab, next_non_empty_tab_index,
    next_tab_index, previous_non_empty_tab_index, previous_tab_index,
};
use crate::cli::tui::{
    DownloadableSkillMatch, InstalledSkillDiscoveryReport, InvalidInstalledSkill, ListedSkillEntry,
    MenuItemStatus, MenuItemUi, MenuUi, MultiSelectMenuResult, MultiSelectResult, SelectMenuResult,
    TerminalSession, is_interactive_terminal, multi_select_menu, parse_metadata, run_configure_tui,
    run_create_tui, run_file_viewer, select_menu, show_loading_message,
};
use crate::client::{ClientConfig, SkillsMpClient, SkillsMpSearchQuery, SkillsMpSkill};
use crate::config::SkillyConfig;
use crate::core::{
    ProjectDependencyOrigin, ProjectEnvironment, SKILLY_SOURCE_GITHUB, SKILLY_SOURCE_SKILLSMP,
    SKILLY_UNKNOWN_SOURCE, STATUS_INSTALLABLE, STATUS_INSTALLED, STATUS_UPDATABLE,
    ScanDependencySelection, SkillData, SkillMatchData, SkillSourceMetadata,
    available_dependency_skill_in, discover_github_skills, github_versions_match,
    project_requirements, remove_skill, scan_match_status, scan_project_in,
};
use anyhow::{Context, Result, bail};
use clap::Parser;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

pub(crate) const BACK_CHOICE: &str = "back";
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
            github_url,
            destination,
            skill_name,
            all,
            overwrite,
            github_token,
        } => run_download(
            &github_url,
            &destination,
            skill_name.as_deref(),
            all,
            overwrite,
            client_config(None, None, github_token, None),
            &skilly_config,
        )?,
        Commands::List {
            destination,
            github_token,
        } => run_list(
            &destination,
            client_config(None, None, github_token, None),
            &skilly_config,
        )?,
        Commands::Update {
            destination,
            yes,
            github_token,
        } => run_update(
            &destination.resolve()?,
            client_config(None, None, github_token, None),
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
                github_token,
            } => run_skillsmp_search(
                &query,
                &destination,
                overwrite,
                client_config(None, None, github_token, None),
                &skilly_config,
            )?,
            SkillsMpSubcommand::List {
                destination,
                github_token,
            } => run_skillsmp_list(
                &destination,
                client_config(None, None, github_token, None),
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
        } => run_configure(
            &skilly_config,
            ConfigureFlags {
                show,
                reset,
                add_global,
                remove_global,
                add_local,
                remove_local,
            },
        )?,
    }

    Ok(0)
}

struct ConfigureFlags {
    show: bool,
    reset: bool,
    add_global: Vec<String>,
    remove_global: Vec<String>,
    add_local: Vec<String>,
    remove_local: Vec<String>,
}

fn run_configure(skilly_config: &SkillyConfig, flags: ConfigureFlags) -> Result<()> {
    if flags.reset {
        let default = SkillyConfig::default();
        default.save()?;
        println!("Configuration reset to defaults (saved to ~/.skilly.toml)");
        return Ok(());
    }

    let has_modifications = !flags.add_global.is_empty()
        || !flags.remove_global.is_empty()
        || !flags.add_local.is_empty()
        || !flags.remove_local.is_empty();

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
        config.save()?;
        config
    } else {
        skilly_config.clone()
    };

    if flags.show {
        let content = toml::to_string_pretty(&config_to_display)
            .context("failed to serialize configuration")?;
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
    let content =
        toml::to_string_pretty(skilly_config).context("failed to serialize configuration")?;
    print!("{content}");
    Ok(())
}

fn client_config(
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> ClientConfig {
    ClientConfig::new(base_url, api_key, github_token, proxy)
}

fn run_create(directory: &Path, mut options: CreateOptions) -> Result<()> {
    let interactive = is_interactive_terminal();
    if interactive {
        let Some(submission) = run_create_tui(directory, options)? else {
            println!("Cancelled without creating skill");
            return Ok(());
        };
        options = submission;
    }

    let name = options
        .name
        .context("Skill name is required outside an interactive terminal")?;
    let description = options
        .description
        .context("Skill description is required outside an interactive terminal")?;
    let skill = SkillData {
        name: name.clone(),
        description,
        path: None,
        content: options
            .instructions
            .unwrap_or_else(|| DEFAULT_CREATE_INSTRUCTIONS.to_string()),
        license: options.license,
        compatibility: options.compatibility,
        metadata: parse_metadata(&options.metadata)?,
        allowed_tools: options.allowed_tools,
        resources: Vec::new(),
        resource_warnings: Vec::new(),
        source: SKILLY_UNKNOWN_SOURCE.to_string(),
        package_name: None,
        package_version: None,
        github_url: None,
        github_commit_sha: None,
        skillsmp_id: None,
        package_ecosystem: None,
    };
    skill.validate()?;

    let installed = if options.overwrite {
        skill.replace_to(directory, None)?
    } else {
        skill.install_to(directory, None, false)?
    };
    let root = installed
        .path
        .as_deref()
        .map(Path::new)
        .context("Created skill has no directory")?;
    for (requested, child) in [
        (options.with_scripts, "scripts"),
        (options.with_references, "references"),
        (options.with_assets, "assets"),
    ] {
        if requested {
            fs::create_dir_all(root.join(child))?;
        }
    }
    println!("Created {} at {}", installed.name, root.display());
    Ok(())
}

fn run_scan(
    destination: &DestinationArgs,
    dependency_selection: ScanDependencySelection,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    let single_directory = destination.resolve()?;
    let environment = ProjectEnvironment::with_paths(
        &single_directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        dependency_selection.clone(),
    );
    let matches = scan_project_in(&environment)?;
    if matches.is_empty() {
        println!("No dependency skills found in pyproject.toml and .venv");
        return Ok(());
    }

    if !is_interactive_terminal() {
        for item in &matches {
            println!("{}", scan_choice_label(item));
        }
        return Ok(());
    }

    let destinations = destination.resolve_interactive_destinations(skilly_config)?;
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut active_tab = 0usize;
    let mut selected_indices = vec![0usize; destinations.len()];
    let mut checked_indices = vec![Vec::<usize>::new(); destinations.len()];

    loop {
        let all_matches_by_tab = destinations
            .iter()
            .map(|destination| {
                let environment = ProjectEnvironment::with_paths(
                    &destination.path,
                    Path::new("pyproject.toml"),
                    Path::new(".venv"),
                    dependency_selection.clone(),
                );
                scan_project_in(&environment)
            })
            .collect::<Result<Vec<_>>>()?;
        let all_empty = all_matches_by_tab.iter().all(Vec::is_empty);
        if all_empty {
            println!("No dependency skills found in pyproject.toml and .venv");
            break;
        }
        if active_tab >= destinations.len() {
            active_tab = 0;
        }

        let directory = &destinations[active_tab].path;
        let all_matches = &all_matches_by_tab[active_tab];
        let selectable_count = all_matches.len();
        let mut items = all_matches
            .iter()
            .map(|item| MenuItemUi {
                label: scan_choice_label(item),
                preview_lines: scan_match_preview_lines(item),
                status: scan_menu_status(item),
                selectable: true,
                filter_text: Some(item.available.name.clone()),
            })
            .collect::<Vec<_>>();
        let exit_index = items.len();
        items.push(exit_menu_item("Exit scan"));

        let result = multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_for_destination(
                    "Select dependency skills to install".to_string(),
                    &destinations,
                    active_tab,
                ),
                items,
                default: selected_indices[active_tab].min(selectable_count),
                preview_title: "Dependency skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: destination_tabs(&destinations, &vec![false; destinations.len()]),
                active_tab,
            },
            selectable_count,
            &checked_indices[active_tab],
        )?;

        match result {
            MultiSelectMenuResult::Cancel => break,
            MultiSelectMenuResult::NextTab => {
                active_tab = next_tab_index(active_tab, destinations.len());
                continue;
            }
            MultiSelectMenuResult::PreviousTab => {
                active_tab = previous_tab_index(active_tab, destinations.len());
                continue;
            }
            MultiSelectMenuResult::Selection(result) => match result {
                MultiSelectResult::Single(index) => {
                    checked_indices[active_tab].clear();
                    if index == exit_index {
                        break;
                    }
                    selected_indices[active_tab] = index;
                    let selected = all_matches[index].clone();
                    let actions = scan_skill_actions(&selected);
                    let action_index = select_menu(
                        &mut session,
                        MenuUi {
                            title: menu_title_with_directory(
                                format!("Choose an action for {}", selected.available.name),
                                directory,
                            ),
                            items: actions
                                .iter()
                                .map(|item| MenuItemUi {
                                    label: (*item).to_string(),
                                    preview_lines: scan_match_preview_lines(&selected),
                                    status: MenuItemStatus::Default,
                                    selectable: true,
                                    filter_text: None,
                                })
                                .collect(),
                            default: action_menu_default(&actions),
                            preview_title: selected.available.name.clone(),
                            status: status_message.clone(),
                            help_text: DEFAULT_HELP_TEXT.to_string(),
                            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                            tabs: Vec::new(),
                            active_tab: 0,
                        },
                    )?;
                    let SelectMenuResult::Selected(action_index) = action_index else {
                        continue;
                    };
                    match actions[action_index] {
                        BACK_CHOICE => continue,
                        EXIT_CHOICE => break,
                        INSTALL_CHOICE | UPDATE_CHOICE => {
                            let installed = install_available_skill(
                                directory,
                                &selected.available,
                                selected
                                    .installed
                                    .as_ref()
                                    .map(skill_directory_name)
                                    .as_deref(),
                                selected.installed.is_some(),
                            )?;
                            let message = if selected.installed.is_none() {
                                format!(
                                    "Installed {} to {}",
                                    skill_directory_name(&installed),
                                    installed.path.as_deref().unwrap_or_default()
                                )
                            } else {
                                format!(
                                    "Updated {} to {}",
                                    skill_directory_name(&installed),
                                    installed.package_version.as_deref().unwrap_or("unknown")
                                )
                            };
                            remember_status(&mut messages, &mut status_message, message);
                        }
                        _ => {}
                    }
                }
                MultiSelectResult::Bulk(indices) => {
                    selected_indices[active_tab] = indices
                        .first()
                        .copied()
                        .unwrap_or(selected_indices[active_tab]);
                    let selected: Vec<SkillMatchData> =
                        indices.iter().map(|&i| all_matches[i].clone()).collect();
                    let preview_names = selected
                        .iter()
                        .map(|item| item.available.name.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let actions = vec![APPLY_ALL_CHOICE, BACK_CHOICE, EXIT_CHOICE];
                    let action_index = select_menu(
                        &mut session,
                        MenuUi {
                            title: menu_title_with_directory(
                                format!("Action for {} selected skills", selected.len()),
                                directory,
                            ),
                            items: actions
                                .iter()
                                .map(|a| MenuItemUi {
                                    label: (*a).to_string(),
                                    preview_lines: preview_names
                                        .lines()
                                        .map(str::to_owned)
                                        .collect(),
                                    status: MenuItemStatus::Default,
                                    selectable: true,
                                    filter_text: None,
                                })
                                .collect(),
                            default: action_menu_default(&actions),
                            preview_title: "Selected skills".to_string(),
                            status: status_message.clone(),
                            help_text: DEFAULT_HELP_TEXT.to_string(),
                            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                            tabs: Vec::new(),
                            active_tab: 0,
                        },
                    )?;
                    let SelectMenuResult::Selected(action_index) = action_index else {
                        checked_indices[active_tab] = indices;
                        continue;
                    };
                    checked_indices[active_tab] =
                        retained_multi_select_indices(Some(actions[action_index]), &indices);
                    match actions[action_index] {
                        BACK_CHOICE => continue,
                        EXIT_CHOICE => break,
                        APPLY_ALL_CHOICE => {
                            for item in &selected {
                                let installed = install_available_skill(
                                    directory,
                                    &item.available,
                                    item.installed.as_ref().map(skill_directory_name).as_deref(),
                                    item.installed.is_some(),
                                )?;
                                let message = if item.installed.is_none() {
                                    format!(
                                        "Installed {} to {}",
                                        skill_directory_name(&installed),
                                        installed.path.as_deref().unwrap_or_default()
                                    )
                                } else {
                                    format!(
                                        "Updated {} to {}",
                                        skill_directory_name(&installed),
                                        installed.package_version.as_deref().unwrap_or("unknown")
                                    )
                                };
                                remember_status(&mut messages, &mut status_message, message);
                            }
                        }
                        _ => {}
                    }
                }
            },
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_download(
    github_url: &str,
    destination: &DestinationArgs,
    skill_name: Option<&str>,
    all: bool,
    overwrite: bool,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    let client = SkillsMpClient::new(config)?;
    let mut skills = discover_github_skills(&client, github_url, SKILLY_SOURCE_GITHUB, None)?;
    let directory = destination.resolve()?;
    if all && skill_name.is_some() && skills.len() != 1 {
        bail!("use either --skill-name or --all when downloading multiple skills");
    }
    let interactive_destinations = if is_interactive_terminal() {
        destination.resolve_interactive_destinations(skilly_config)?
    } else {
        Vec::new()
    };
    if is_interactive_terminal() && (skills.len() > 1 || interactive_destinations.len() > 1) {
        if skills.len() > 1 && !all && skill_name.is_none() {
            return download_selected_skills(
                &client,
                &interactive_destinations,
                overwrite,
                &skills,
            );
        }
        if let Some(skill_name) = skill_name {
            if skills.len() != 1 && all {
                bail!("custom skill names can only be used when downloading a single skill");
            }
            if skills.len() != 1 {
                skills = vec![select_download_skill(&skills, skill_name)?];
            }
        }
        return download_selected_skills(&client, &interactive_destinations, overwrite, &skills);
    }
    if skills.len() > 1 && !all && skill_name.is_none() {
        if !is_interactive_terminal() {
            bail!("multiple skills found; use --skill-name <name> or --all");
        }
        return download_selected_skills(
            &client,
            &[ResolvedDestination {
                label: "current".to_string(),
                path: directory.clone(),
                color: ratatui::style::Color::White,
            }],
            overwrite,
            &skills,
        );
    }
    if let Some(skill_name) = skill_name {
        if skills.len() != 1 && all {
            bail!("custom skill names can only be used when downloading a single skill");
        }
        if skills.len() != 1 {
            skills = vec![select_download_skill(&skills, skill_name)?];
        }
    }

    let installed = skills
        .iter()
        .map(|skill| {
            skill.install_to(
                &directory,
                if skills.len() == 1 { skill_name } else { None },
                overwrite,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    if installed.len() == 1 {
        let installed = &installed[0];
        println!(
            "Downloaded {} files to {}",
            installed.resources.len() + 1,
            installed.path.as_deref().unwrap_or_default()
        );
        return Ok(());
    }
    for skill in installed {
        println!(
            "Downloaded {} with {} files to {}",
            skill_directory_name(&skill),
            skill.resources.len() + 1,
            skill.path.as_deref().unwrap_or_default()
        );
    }
    Ok(())
}

fn download_selected_skills(
    client: &SkillsMpClient,
    destinations: &[ResolvedDestination],
    overwrite: bool,
    skills: &[SkillData],
) -> Result<()> {
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut active_tab = 0usize;
    let mut selected_indices = vec![0usize; destinations.len()];
    let mut checked_indices = vec![Vec::<usize>::new(); destinations.len()];
    loop {
        let matches_by_tab = destinations
            .iter()
            .map(|destination| {
                let installed_skills =
                    discover_installed_skills_report(&destination.path)?.valid_skills;
                Ok(downloadable_skill_matches(skills, &installed_skills))
            })
            .collect::<Result<Vec<_>>>()?;
        if active_tab >= destinations.len() {
            active_tab = 0;
        }
        let directory = &destinations[active_tab].path;
        let matches = &matches_by_tab[active_tab];
        let selectable_count = matches.len();
        let mut items = matches
            .iter()
            .map(|item| MenuItemUi {
                label: downloadable_skill_label(item),
                preview_lines: downloadable_skill_preview_lines(item, directory),
                status: downloadable_skill_menu_status(item),
                selectable: true,
                filter_text: Some(item.available.name.clone()),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit download"));
        let result = multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_for_destination(
                    "Select skills to download".to_string(),
                    destinations,
                    active_tab,
                ),
                items,
                default: selected_indices[active_tab].min(selectable_count),
                preview_title: "Downloadable skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: destination_tabs(destinations, &vec![false; destinations.len()]),
                active_tab,
            },
            selectable_count,
            &checked_indices[active_tab],
        )?;

        match result {
            MultiSelectMenuResult::Cancel => break,
            MultiSelectMenuResult::NextTab => {
                active_tab = next_tab_index(active_tab, destinations.len());
                continue;
            }
            MultiSelectMenuResult::PreviousTab => {
                active_tab = previous_tab_index(active_tab, destinations.len());
                continue;
            }
            MultiSelectMenuResult::Selection(result) => match result {
                MultiSelectResult::Single(index) => {
                    checked_indices[active_tab].clear();
                    if index == selectable_count {
                        break;
                    }
                    selected_indices[active_tab] = index;
                    let selected = matches[index].clone();
                    let actions = downloadable_skill_actions(&selected);
                    let action_index = select_menu(
                        &mut session,
                        MenuUi {
                            title: menu_title_with_directory(
                                format!(
                                    "Choose an action for {}",
                                    skill_directory_name(&selected.available)
                                ),
                                directory,
                            ),
                            items: actions
                                .iter()
                                .map(|item| MenuItemUi {
                                    label: (*item).to_string(),
                                    preview_lines: downloadable_skill_preview_lines(
                                        &selected, directory,
                                    ),
                                    status: MenuItemStatus::Default,
                                    selectable: true,
                                    filter_text: None,
                                })
                                .collect(),
                            default: action_menu_default(&actions),
                            preview_title: skill_directory_name(&selected.available),
                            status: status_message.clone(),
                            help_text: DEFAULT_HELP_TEXT.to_string(),
                            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                            tabs: Vec::new(),
                            active_tab: 0,
                        },
                    )?;
                    let SelectMenuResult::Selected(action_index) = action_index else {
                        continue;
                    };
                    match actions[action_index] {
                        BACK_CHOICE => continue,
                        EXIT_CHOICE => break,
                        VIEW_FILES_CHOICE => {
                            run_file_viewer(&mut session, &selected.available)?;
                        }
                        INSTALL_CHOICE => {
                            let installed =
                                selected.available.install_to(directory, None, overwrite)?;
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                format!(
                                    "Downloaded {} with {} files to {}",
                                    skill_directory_name(&installed),
                                    installed.resources.len() + 1,
                                    installed.path.as_deref().unwrap_or_default()
                                ),
                            );
                        }
                        UPDATE_CHOICE => {
                            let installed = selected
                                .installed
                                .as_ref()
                                .context("Only installed skills can be updated")?;
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                update_skill(directory, installed, client)?,
                            );
                        }
                        REMOVE_CHOICE => {
                            let installed = selected
                                .installed
                                .as_ref()
                                .context("Only installed skills can be removed")?;
                            let removed =
                                remove_skill(&skill_directory_name(installed), directory)?;
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                format!("Removed {}", skill_directory_name(&removed)),
                            );
                        }
                        _ => {}
                    }
                }
                MultiSelectResult::Bulk(indices) => {
                    selected_indices[active_tab] = indices
                        .first()
                        .copied()
                        .unwrap_or(selected_indices[active_tab]);
                    let selected: Vec<DownloadableSkillMatch> =
                        indices.iter().map(|&i| matches[i].clone()).collect();
                    let has_installable = selected.iter().any(|m| m.status() == STATUS_INSTALLABLE);
                    let has_updatable = selected.iter().any(|m| m.status() == STATUS_UPDATABLE);
                    let has_removable = selected.iter().any(|m| m.installed.is_some());
                    let mut actions: Vec<&'static str> = Vec::new();
                    if has_installable {
                        actions.push(INSTALL_ALL_CHOICE);
                    }
                    if has_updatable {
                        actions.push(UPDATE_ALL_CHOICE);
                    }
                    if has_removable {
                        actions.push(REMOVE_ALL_CHOICE);
                    }
                    actions.push(BACK_CHOICE);
                    actions.push(EXIT_CHOICE);
                    let preview_names = selected
                        .iter()
                        .map(|m| skill_directory_name(&m.available))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let action_index = select_menu(
                        &mut session,
                        MenuUi {
                            title: menu_title_with_directory(
                                format!("Action for {} selected skills", selected.len()),
                                directory,
                            ),
                            items: actions
                                .iter()
                                .map(|a| MenuItemUi {
                                    label: (*a).to_string(),
                                    preview_lines: preview_names
                                        .lines()
                                        .map(str::to_owned)
                                        .collect(),
                                    status: MenuItemStatus::Default,
                                    selectable: true,
                                    filter_text: None,
                                })
                                .collect(),
                            default: action_menu_default(&actions),
                            preview_title: "Selected skills".to_string(),
                            status: status_message.clone(),
                            help_text: DEFAULT_HELP_TEXT.to_string(),
                            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                            tabs: Vec::new(),
                            active_tab: 0,
                        },
                    )?;
                    let SelectMenuResult::Selected(action_index) = action_index else {
                        checked_indices[active_tab] = indices;
                        continue;
                    };
                    checked_indices[active_tab] =
                        retained_multi_select_indices(Some(actions[action_index]), &indices);
                    match actions[action_index] {
                        BACK_CHOICE => continue,
                        EXIT_CHOICE => break,
                        INSTALL_ALL_CHOICE => {
                            for m in selected.iter().filter(|m| m.status() == STATUS_INSTALLABLE) {
                                let installed =
                                    m.available.install_to(directory, None, overwrite)?;
                                remember_status(
                                    &mut messages,
                                    &mut status_message,
                                    format!(
                                        "Downloaded {} with {} files to {}",
                                        skill_directory_name(&installed),
                                        installed.resources.len() + 1,
                                        installed.path.as_deref().unwrap_or_default()
                                    ),
                                );
                            }
                        }
                        UPDATE_ALL_CHOICE => {
                            for m in selected.iter().filter(|m| m.status() == STATUS_UPDATABLE) {
                                let installed_skill = m
                                    .installed
                                    .as_ref()
                                    .context("Only installed skills can be updated")?;
                                remember_status(
                                    &mut messages,
                                    &mut status_message,
                                    update_skill(directory, installed_skill, client)?,
                                );
                            }
                        }
                        REMOVE_ALL_CHOICE => {
                            for m in selected.iter().filter(|m| m.installed.is_some()) {
                                let installed_skill = m
                                    .installed
                                    .as_ref()
                                    .context("Only installed skills can be removed")?;
                                let removed = remove_skill(
                                    &skill_directory_name(installed_skill),
                                    directory,
                                )?;
                                remember_status(
                                    &mut messages,
                                    &mut status_message,
                                    format!("Removed {}", skill_directory_name(&removed)),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            },
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_list(
    destination: &DestinationArgs,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    if !is_interactive_terminal() {
        let directory = destination.resolve()?;
        let report = discover_installed_skills_report(&directory)?;
        let entries = listed_skill_entries(report);
        if entries.is_empty() {
            println!("{}", no_skills_found_message(&directory));
            return Ok(());
        }
        for entry in entries {
            println!("{}", listed_skill_label(&entry));
        }
        return Ok(());
    }

    let destinations = destination.resolve_interactive_destinations(skilly_config)?;
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let client = SkillsMpClient::new(config)?;
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_indices = vec![0usize; destinations.len()];
    let mut checked_indices = vec![Vec::<usize>::new(); destinations.len()];
    let mut active_tab = 0usize;

    loop {
        let reports_by_tab = destinations
            .iter()
            .map(|destination| discover_installed_skills_report(&destination.path))
            .collect::<Result<Vec<_>>>()?;
        let entries_by_tab = reports_by_tab
            .into_iter()
            .map(listed_skill_entries)
            .collect::<Vec<_>>();
        let empty_flags = entries_by_tab.iter().map(Vec::is_empty).collect::<Vec<_>>();
        if empty_flags.iter().all(|is_empty| *is_empty) {
            println!("{}", no_skills_found_message_anywhere());
            break;
        }
        if active_tab >= destinations.len() || empty_flags[active_tab] {
            active_tab = pick_best_list_tab(&destinations, &empty_flags);
        }

        let directory = &destinations[active_tab].path;
        let entries = &entries_by_tab[active_tab];
        let selectable_count = entries
            .iter()
            .filter(|entry| matches!(entry, ListedSkillEntry::Valid(_)))
            .count();
        let exit_index = entries.len();
        let mut items = entries
            .iter()
            .map(|entry| MenuItemUi {
                label: listed_skill_label(entry),
                preview_lines: listed_skill_preview_lines(entry),
                status: listed_skill_menu_status(entry),
                selectable: matches!(entry, ListedSkillEntry::Valid(_)),
                filter_text: Some(match entry {
                    ListedSkillEntry::Valid(skill) => skill.name.clone(),
                    ListedSkillEntry::Invalid(invalid) => invalid.directory_name.clone(),
                }),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit list"));

        match multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_for_destination(
                    "Select installed skills".to_string(),
                    &destinations,
                    active_tab,
                ),
                items,
                default: selected_indices[active_tab].min(selectable_count),
                preview_title: "Installed skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: destination_tabs(&destinations, &empty_flags),
                active_tab,
            },
            selectable_count,
            &checked_indices[active_tab],
        )? {
            MultiSelectMenuResult::Cancel => break,
            MultiSelectMenuResult::NextTab => {
                active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
            }
            MultiSelectMenuResult::PreviousTab => {
                active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
            }
            MultiSelectMenuResult::Selection(MultiSelectResult::Single(index)) => {
                checked_indices[active_tab].clear();
                if index == exit_index {
                    break;
                }
                selected_indices[active_tab] = index;
                match entries[index].clone() {
                    ListedSkillEntry::Valid(selected) => {
                        let update_available = update_available_or_remember_error(
                            directory,
                            &selected,
                            &client,
                            &mut status_message,
                        );
                        let actions = installed_skill_actions(update_available, REMOVE_CHOICE);
                        match select_menu(
                            &mut session,
                            MenuUi {
                                title: menu_title_for_destination(
                                    format!(
                                        "Choose an action for {}",
                                        skill_directory_name(&selected)
                                    ),
                                    &destinations,
                                    active_tab,
                                ),
                                items: actions
                                    .iter()
                                    .map(|item| MenuItemUi {
                                        label: (*item).to_string(),
                                        preview_lines: listed_skill_preview_lines(
                                            &ListedSkillEntry::Valid(selected.clone()),
                                        ),
                                        status: MenuItemStatus::Default,
                                        selectable: true,
                                        filter_text: None,
                                    })
                                    .collect(),
                                default: action_menu_default(&actions),
                                preview_title: skill_directory_name(&selected),
                                status: status_message.clone(),
                                help_text: DEFAULT_HELP_TEXT.to_string(),
                                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                                tabs: destination_tabs(&destinations, &empty_flags),
                                active_tab,
                            },
                        )? {
                            SelectMenuResult::Cancel => continue,
                            SelectMenuResult::NextTab => {
                                active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
                            }
                            SelectMenuResult::PreviousTab => {
                                active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
                            }
                            SelectMenuResult::Selected(action_index) => match actions[action_index]
                            {
                                BACK_CHOICE => continue,
                                EXIT_CHOICE => break,
                                VIEW_FILES_CHOICE => {
                                    run_file_viewer(&mut session, &selected)?;
                                }
                                UPDATE_CHOICE => {
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        update_skill(directory, &selected, &client)?,
                                    );
                                }
                                REMOVE_CHOICE => {
                                    let removed =
                                        remove_skill(&skill_directory_name(&selected), directory)?;
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        format!("Removed {}", skill_directory_name(&removed)),
                                    );
                                }
                                _ => {}
                            },
                            // ----- installed skill action menu end -----
                        }
                    }
                    ListedSkillEntry::Invalid(selected) => {
                        let actions = invalid_installed_skill_actions(REMOVE_CHOICE);
                        match select_menu(
                            &mut session,
                            MenuUi {
                                title: menu_title_for_destination(
                                    format!("Choose an action for {}", selected.directory_name),
                                    &destinations,
                                    active_tab,
                                ),
                                items: actions
                                    .iter()
                                    .map(|item| MenuItemUi {
                                        label: (*item).to_string(),
                                        preview_lines: listed_skill_preview_lines(
                                            &ListedSkillEntry::Invalid(selected.clone()),
                                        ),
                                        status: MenuItemStatus::Default,
                                        selectable: true,
                                        filter_text: None,
                                    })
                                    .collect(),
                                default: action_menu_default(&actions),
                                preview_title: selected.directory_name.clone(),
                                status: status_message.clone(),
                                help_text: DEFAULT_HELP_TEXT.to_string(),
                                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                                tabs: destination_tabs(&destinations, &empty_flags),
                                active_tab,
                            },
                        )? {
                            SelectMenuResult::Cancel => continue,
                            SelectMenuResult::NextTab => {
                                active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
                            }
                            SelectMenuResult::PreviousTab => {
                                active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
                            }
                            SelectMenuResult::Selected(action_index) => match actions[action_index]
                            {
                                BACK_CHOICE => continue,
                                EXIT_CHOICE => break,
                                REMOVE_CHOICE => {
                                    fs::remove_dir_all(&selected.path)?;
                                    let removed_name = selected.directory_name;
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        format!("Removed invalid skill {removed_name}"),
                                    );
                                }
                                _ => {}
                            },
                        }
                    }
                }
            }
            MultiSelectMenuResult::Selection(MultiSelectResult::Bulk(indices)) => {
                selected_indices[active_tab] = indices
                    .first()
                    .copied()
                    .unwrap_or(selected_indices[active_tab]);
                let selected = indices
                    .iter()
                    .map(|&i| entries[i].clone())
                    .collect::<Vec<_>>();
                let preview_names = selected
                    .iter()
                    .map(|entry| match entry {
                        ListedSkillEntry::Valid(skill) => skill_directory_name(skill),
                        ListedSkillEntry::Invalid(skill) => skill.directory_name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let has_valid = selected
                    .iter()
                    .any(|entry| matches!(entry, ListedSkillEntry::Valid(_)));
                let has_invalid = selected
                    .iter()
                    .any(|entry| matches!(entry, ListedSkillEntry::Invalid(_)));
                let mut actions = vec![REMOVE_ALL_CHOICE];
                if has_valid && !has_invalid {
                    actions.push(UPDATE_ALL_CHOICE);
                }
                actions.push(BACK_CHOICE);
                actions.push(EXIT_CHOICE);
                match select_menu(
                    &mut session,
                    MenuUi {
                        title: menu_title_for_destination(
                            format!("Action for {} selected skills", selected.len()),
                            &destinations,
                            active_tab,
                        ),
                        items: actions
                            .iter()
                            .map(|action| MenuItemUi {
                                label: (*action).to_string(),
                                preview_lines: preview_names.lines().map(str::to_owned).collect(),
                                status: MenuItemStatus::Default,
                                selectable: true,
                                filter_text: None,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: "Selected skills".to_string(),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                        tabs: destination_tabs(&destinations, &empty_flags),
                        active_tab,
                    },
                )? {
                    SelectMenuResult::Cancel => {
                        checked_indices[active_tab] = indices;
                    }
                    SelectMenuResult::NextTab => {
                        checked_indices[active_tab] = indices;
                        active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
                    }
                    SelectMenuResult::PreviousTab => {
                        checked_indices[active_tab] = indices;
                        active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
                    }
                    SelectMenuResult::Selected(action_index) => {
                        checked_indices[active_tab] =
                            retained_multi_select_indices(Some(actions[action_index]), &indices);
                        match actions[action_index] {
                            BACK_CHOICE => continue,
                            EXIT_CHOICE => break,
                            REMOVE_ALL_CHOICE => {
                                for entry in &selected {
                                    match entry {
                                        ListedSkillEntry::Valid(skill) => {
                                            let removed = remove_skill(
                                                &skill_directory_name(skill),
                                                directory,
                                            )?;
                                            remember_status(
                                                &mut messages,
                                                &mut status_message,
                                                format!(
                                                    "Removed {}",
                                                    skill_directory_name(&removed)
                                                ),
                                            );
                                        }
                                        ListedSkillEntry::Invalid(skill) => {
                                            fs::remove_dir_all(&skill.path)?;
                                            remember_status(
                                                &mut messages,
                                                &mut status_message,
                                                format!(
                                                    "Removed invalid skill {}",
                                                    skill.directory_name
                                                ),
                                            );
                                        }
                                    }
                                }
                            }
                            UPDATE_ALL_CHOICE => {
                                for entry in &selected {
                                    if let ListedSkillEntry::Valid(skill) = entry {
                                        match update_skill(directory, skill, &client) {
                                            Ok(msg) if !msg.contains("already up to date") => {
                                                remember_status(
                                                    &mut messages,
                                                    &mut status_message,
                                                    msg,
                                                );
                                            }
                                            Ok(_) => {}
                                            Err(error) => {
                                                remember_status(
                                                    &mut messages,
                                                    &mut status_message,
                                                    format!(
                                                        "Failed to update {}: {error}",
                                                        skill_directory_name(skill)
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct PendingSkillUpdate {
    installed: SkillData,
    available: SkillData,
}

fn run_update(directory: &Path, config: ClientConfig, yes: bool) -> Result<()> {
    let client = SkillsMpClient::new(config)?;
    let installed_skills = discover_installed_skills_report(directory)?.valid_skills;
    let environment = ProjectEnvironment::with_paths(
        directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        ScanDependencySelection::default(),
    );

    let mut updates = Vec::new();
    for item in crate::core::dependency_updates_in(&environment)? {
        let installed = item.installed.context("Missing installed skill")?;
        updates.push(PendingSkillUpdate {
            installed,
            available: item.available,
        });
    }

    let dependency_names = updates
        .iter()
        .map(|item| skill_directory_name(&item.installed))
        .collect::<std::collections::BTreeSet<_>>();
    let mut errors = Vec::new();
    for installed in installed_skills
        .into_iter()
        .filter(|skill| skill.github_url.is_some() && !skill.is_dependency())
    {
        if dependency_names.contains(&skill_directory_name(&installed)) {
            continue;
        }
        match available_github_update(&installed, &client) {
            Ok(Some(available)) => updates.push(PendingSkillUpdate {
                installed,
                available,
            }),
            Ok(None) => {}
            Err(error) => errors.push(format!(
                "Could not check updates for {}: {error}",
                skill_directory_name(&installed)
            )),
        }
    }

    updates.sort_by(|left, right| {
        skill_directory_name(&left.installed).cmp(&skill_directory_name(&right.installed))
    });

    if updates.is_empty() {
        for error in errors {
            println!("{error}");
        }
        println!("No installed skill updates available");
        return Ok(());
    }

    println!("Available skill updates:");
    for update in &updates {
        println!("{}", format_pending_update(update));
    }
    for error in &errors {
        println!("{error}");
    }
    println!("Use `skilly list` to review or apply updates one skill at a time.");

    let apply_updates = if yes {
        true
    } else if io::stdin().is_terminal() {
        confirm_apply_updates()?
    } else {
        println!("Re-run with --yes to apply these updates");
        return Ok(());
    };

    if !apply_updates {
        println!("Cancelled without applying updates");
        return Ok(());
    }

    for update in &updates {
        let updated = install_available_skill(
            directory,
            &update.available,
            Some(&skill_directory_name(&update.installed)),
            true,
        )?;
        println!("{}", format_applied_update(&update.installed, &updated));
    }
    Ok(())
}

fn available_github_update(
    skill: &SkillData,
    client: &SkillsMpClient,
) -> Result<Option<SkillData>> {
    let Some(github_url) = skill.github_url.as_deref() else {
        return Ok(None);
    };
    let refreshed = discover_github_skills(
        client,
        github_url,
        if skill.is_skillsmp() {
            SKILLY_SOURCE_SKILLSMP
        } else {
            SKILLY_SOURCE_GITHUB
        },
        skill.skillsmp_id.clone(),
    )?
    .into_iter()
    .next()
    .context("GitHub URL resolves to no skills")?;
    if github_versions_match(skill, &refreshed) {
        return Ok(None);
    }
    Ok(Some(refreshed))
}

fn confirm_apply_updates() -> Result<bool> {
    print!("Apply these updates? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let normalized = answer.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
}

fn format_pending_update(update: &PendingSkillUpdate) -> String {
    if update.installed.is_dependency() {
        return format!(
            "{} [dependency]: {} {} -> {}",
            skill_directory_name(&update.installed),
            update
                .available
                .package_name
                .as_deref()
                .unwrap_or("unknown"),
            update
                .installed
                .package_version
                .as_deref()
                .unwrap_or("unknown"),
            update
                .available
                .package_version
                .as_deref()
                .unwrap_or("unknown")
        );
    }

    format!(
        "{} [{}]: {} -> {}",
        skill_directory_name(&update.installed),
        if update.installed.is_skillsmp() {
            "skillsmp/github"
        } else {
            "github"
        },
        short_revision(update.installed.github_commit_sha.as_deref()),
        short_revision(update.available.github_commit_sha.as_deref())
    )
}

fn format_applied_update(previous: &SkillData, updated: &SkillData) -> String {
    if previous.is_dependency() {
        return format!(
            "Updated {} to {}",
            skill_directory_name(updated),
            updated.package_version.as_deref().unwrap_or("unknown")
        );
    }

    format!(
        "Updated {} to commit {}",
        skill_directory_name(updated),
        short_revision(updated.github_commit_sha.as_deref())
    )
}

fn short_revision(revision: Option<&str>) -> String {
    let value = revision.unwrap_or("unknown");
    value.chars().take(7).collect()
}

fn run_skillsmp_search(
    query: &str,
    destination: &DestinationArgs,
    overwrite: bool,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    let client = SkillsMpClient::new(config.clone())?;
    let response = client.search(&SkillsMpSearchQuery::new(query))?;
    if response.data.skills.is_empty() {
        println!("No SkillsMP skills found for {query}");
        return Ok(());
    }
    if !is_interactive_terminal() {
        for skill in response.data.skills {
            println!("{} [{}] ({})", skill.name, skill.author, skill.id);
        }
        return Ok(());
    }

    let destinations = destination.resolve_interactive_destinations(skilly_config)?;
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let mut session = TerminalSession::new()?;
    let mut cache = std::collections::BTreeMap::<String, SkillData>::new();
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut active_tab = 0usize;
    let mut selected_indices = vec![0usize; destinations.len()];
    loop {
        let installed_skills_by_tab = destinations
            .iter()
            .map(
                |destination| Ok(discover_installed_skills_report(&destination.path)?.valid_skills),
            )
            .collect::<Result<Vec<_>>>()?;
        let empty_flags = installed_skills_by_tab
            .iter()
            .map(Vec::is_empty)
            .collect::<Vec<_>>();
        if active_tab >= destinations.len() || empty_flags[active_tab] {
            active_tab = first_non_empty_tab(&empty_flags);
        }
        let directory = &destinations[active_tab].path;
        let search_matches = response
            .data
            .skills
            .iter()
            .map(|skill| {
                (
                    skill,
                    installed_skillsmp_match(skill, &installed_skills_by_tab[active_tab]),
                )
            })
            .collect::<Vec<_>>();
        let mut items = response
            .data
            .skills
            .iter()
            .zip(search_matches.iter())
            .map(|(skill, (_matched_skill, installed))| MenuItemUi {
                label: search_skill_label(skill, installed.as_ref()),
                preview_lines: skillsmp_search_preview_lines(skill, installed.as_ref(), directory),
                status: skillsmp_search_menu_status(installed.as_ref()),
                selectable: true,
                filter_text: Some(skill.name.clone()),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit search"));
        let index = select_menu(
            &mut session,
            MenuUi {
                title: menu_title_for_destination(
                    format!("Select a skill for \"{query}\""),
                    &destinations,
                    active_tab,
                ),
                items,
                default: selected_indices[active_tab],
                preview_title: "SkillsMP result".to_string(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: destination_tabs(&destinations, &vec![false; destinations.len()]),
                active_tab,
            },
        )?;
        let index = match index {
            SelectMenuResult::Cancel => break,
            SelectMenuResult::NextTab => {
                active_tab = next_tab_index(active_tab, destinations.len());
                continue;
            }
            SelectMenuResult::PreviousTab => {
                active_tab = previous_tab_index(active_tab, destinations.len());
                continue;
            }
            SelectMenuResult::Selected(index) => index,
        };
        if index == response.data.skills.len() {
            break;
        }
        selected_indices[active_tab] = index;
        let skill = search_matches[index].0;
        let matched_installed = search_matches[index].1.clone();
        let installable = if let Some(existing) = cache.get(&skill.id) {
            existing.clone()
        } else {
            let selected_skill = skill.clone();
            let selected_config = config.clone();
            let skill_data = show_loading_message(
                &format!("Preparing {}", selected_skill.name),
                &format!(
                    "Downloading skill metadata from GitHub for {}",
                    selected_skill.github_url
                ),
                move || {
                    let client = SkillsMpClient::new(selected_config)?;
                    discover_github_skills(
                        &client,
                        &selected_skill.github_url,
                        SKILLY_SOURCE_SKILLSMP,
                        Some(selected_skill.id.clone()),
                    )?
                    .into_iter()
                    .next()
                    .context("GitHub URL resolves to no skills")
                },
            )?;
            cache.insert(skill.id.clone(), skill_data.clone());
            skill_data
        };
        let downloadable_match = DownloadableSkillMatch {
            available: installable.clone(),
            installed: matched_installed,
        };
        let actions = downloadable_skill_actions(&downloadable_match);
        let action_index = select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    format!("Choose an action for {}", skill.name),
                    directory,
                ),
                items: actions
                    .iter()
                    .map(|item| MenuItemUi {
                        label: (*item).to_string(),
                        preview_lines: skillsmp_installable_preview_lines(
                            skill,
                            &downloadable_match,
                            directory,
                        ),
                        status: MenuItemStatus::Default,
                        selectable: true,
                        filter_text: None,
                    })
                    .collect(),
                default: action_menu_default(&actions),
                preview_title: skill.name.clone(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: Vec::new(),
                active_tab: 0,
            },
        )?;
        let SelectMenuResult::Selected(action_index) = action_index else {
            continue;
        };
        match actions[action_index] {
            BACK_CHOICE => continue,
            EXIT_CHOICE => break,
            INSTALL_CHOICE => {
                let installed = installable.install_to(directory, None, overwrite)?;
                remember_status(
                    &mut messages,
                    &mut status_message,
                    format!(
                        "Installed {} to {}",
                        installed.name,
                        installed.path.as_deref().unwrap_or_default()
                    ),
                );
            }
            UPDATE_CHOICE => {
                let installed = downloadable_match
                    .installed
                    .as_ref()
                    .context("Only installed skills can be updated")?;
                remember_status(
                    &mut messages,
                    &mut status_message,
                    update_skill(directory, installed, &client)?,
                );
            }
            REMOVE_CHOICE => {
                let installed = downloadable_match
                    .installed
                    .as_ref()
                    .context("Only installed skills can be removed")?;
                let removed = remove_skill(&skill_directory_name(installed), directory)?;
                remember_status(
                    &mut messages,
                    &mut status_message,
                    format!("Removed {}", skill_directory_name(&removed)),
                );
            }
            _ => {}
        }
    }
    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_skillsmp_list(
    destination: &DestinationArgs,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    if !is_interactive_terminal() {
        let directory = destination.resolve()?;
        let skills = discover_installed_skills_report(&directory)?
            .valid_skills
            .into_iter()
            .filter(|skill| skill.is_skillsmp())
            .collect::<Vec<_>>();
        if skills.is_empty() {
            println!(
                "No SkillsMP-installed skills found in {}",
                directory.display()
            );
        } else {
            for skill in skills {
                println!("{}", installed_skill_label(&skill));
            }
        }
        return Ok(());
    }

    let destinations = destination.resolve_interactive_destinations(skilly_config)?;
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let client = SkillsMpClient::new(config)?;
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_indices = vec![0usize; destinations.len()];
    let mut active_tab = 0usize;

    loop {
        let skills_by_tab = destinations
            .iter()
            .map(|destination| {
                Ok(discover_installed_skills_report(&destination.path)?
                    .valid_skills
                    .into_iter()
                    .filter(|skill| skill.is_skillsmp())
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>>>()?;
        let empty_flags = skills_by_tab.iter().map(Vec::is_empty).collect::<Vec<_>>();
        if empty_flags.iter().all(|is_empty| *is_empty) {
            println!("No SkillsMP-installed skills found in any managed directory");
            break;
        }
        if active_tab >= destinations.len() || empty_flags[active_tab] {
            active_tab = first_non_empty_tab(&empty_flags);
        }

        let directory = &destinations[active_tab].path;
        let skills = &skills_by_tab[active_tab];
        let mut items = skills
            .iter()
            .map(|skill| MenuItemUi {
                label: installed_skill_label(skill),
                preview_lines: installed_skill_preview_lines(skill),
                status: MenuItemStatus::Default,
                selectable: true,
                filter_text: Some(skill.name.clone()),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit list"));

        match select_menu(
            &mut session,
            MenuUi {
                title: menu_title_for_destination(
                    "Select an installed SkillsMP skill".to_string(),
                    &destinations,
                    active_tab,
                ),
                items,
                default: selected_indices[active_tab].min(skills.len()),
                preview_title: "Installed skill".to_string(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                tabs: destination_tabs(&destinations, &empty_flags),
                active_tab,
            },
        )? {
            SelectMenuResult::Cancel => break,
            SelectMenuResult::NextTab => {
                active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
            }
            SelectMenuResult::PreviousTab => {
                active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
            }
            SelectMenuResult::Selected(index) => {
                if index == skills.len() {
                    break;
                }
                selected_indices[active_tab] = index;
                let selected = skills[index].clone();
                let update_available = update_available_or_remember_error(
                    directory,
                    &selected,
                    &client,
                    &mut status_message,
                );
                let actions = installed_skill_actions(update_available, DELETE_CHOICE);
                match select_menu(
                    &mut session,
                    MenuUi {
                        title: menu_title_for_destination(
                            format!("Choose an action for {}", skill_directory_name(&selected)),
                            &destinations,
                            active_tab,
                        ),
                        items: actions
                            .iter()
                            .map(|item| MenuItemUi {
                                label: (*item).to_string(),
                                preview_lines: installed_skill_preview_lines(&selected),
                                status: MenuItemStatus::Default,
                                selectable: true,
                                filter_text: None,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: skill_directory_name(&selected),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                        tabs: destination_tabs(&destinations, &empty_flags),
                        active_tab,
                    },
                )? {
                    SelectMenuResult::Cancel => continue,
                    SelectMenuResult::NextTab => {
                        active_tab = next_non_empty_tab_index(active_tab, &empty_flags);
                    }
                    SelectMenuResult::PreviousTab => {
                        active_tab = previous_non_empty_tab_index(active_tab, &empty_flags);
                    }
                    SelectMenuResult::Selected(action_index) => match actions[action_index] {
                        BACK_CHOICE => continue,
                        EXIT_CHOICE => break,
                        UPDATE_CHOICE => {
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                update_skill(directory, &selected, &client)?,
                            );
                        }
                        DELETE_CHOICE => {
                            let removed =
                                remove_skill(&skill_directory_name(&selected), directory)?;
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                format!("Removed {}", skill_directory_name(&removed)),
                            );
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_util_venv(path: &Path, detailed: bool) -> Result<()> {
    let skills = crate::core::discover_venv_skills(path)?;
    println!("Found {} skills:", skills.len());
    for skill in skills {
        println!(
            "{}[{}]:\n{}",
            skill.name,
            skill
                .package_reference()
                .unwrap_or_else(|| "unknown".to_string()),
            skill.description
        );
        if detailed {
            println!("\tResources:");
            for resource in skill.resources {
                let content_length = resource.content.lines().count();
                println!(
                    "\t\t{} [{}]: {} lines.",
                    resource.relative_path, resource.kind, content_length
                );
            }
        }
    }
    Ok(())
}

fn remember_status(
    messages: &mut Vec<String>,
    status_message: &mut Option<String>,
    message: String,
) {
    *status_message = Some(message.clone());
    messages.push(message);
}

fn install_available_skill(
    directory: &Path,
    skill: &SkillData,
    skill_name: Option<&str>,
    replace: bool,
) -> Result<SkillData> {
    if replace {
        return skill.replace_to(directory, skill_name);
    }
    skill.install_to(directory, skill_name, true)
}

fn update_skill(directory: &Path, skill: &SkillData, client: &SkillsMpClient) -> Result<String> {
    if skill.is_dependency() {
        let environment = ProjectEnvironment::with_paths(
            directory,
            Path::new("pyproject.toml"),
            Path::new(".venv"),
            ScanDependencySelection::default(),
        );
        let Some(available) = available_dependency_skill_in(skill, &environment)? else {
            return Ok(format!(
                "No dependency source found for {}",
                skill_directory_name(skill)
            ));
        };
        if available.package_version == skill.package_version {
            return Ok(format!(
                "{} is already up to date ({})",
                skill_directory_name(skill),
                available.package_version.as_deref().unwrap_or("unknown")
            ));
        }
        let updated = install_available_skill(
            directory,
            &available,
            Some(&skill_directory_name(skill)),
            true,
        )?;
        return Ok(format!(
            "Updated {} to {}",
            skill_directory_name(&updated),
            updated.package_version.as_deref().unwrap_or("unknown")
        ));
    }

    if let Some(github_url) = skill.github_url.as_deref() {
        let refreshed = discover_github_skills(
            client,
            github_url,
            if skill.is_skillsmp() {
                SKILLY_SOURCE_SKILLSMP
            } else {
                SKILLY_SOURCE_GITHUB
            },
            skill.skillsmp_id.clone(),
        )?
        .into_iter()
        .next()
        .context("GitHub URL resolves to no skills")?;
        if github_versions_match(skill, &refreshed) {
            return Ok(format!(
                "{} is already up to date ({})",
                skill_directory_name(skill),
                skill.github_commit_sha.as_deref().unwrap_or("unknown")
            ));
        }
        let updated = install_available_skill(
            directory,
            &refreshed,
            Some(&skill_directory_name(skill)),
            true,
        )?;
        return Ok(format!(
            "Updated {} with {} files",
            skill_directory_name(&updated),
            updated.resources.len() + 1
        ));
    }

    Ok(format!(
        "Cannot update {}: unknown source",
        skill_directory_name(skill)
    ))
}

fn skill_update_available(
    directory: &Path,
    skill: &SkillData,
    client: &SkillsMpClient,
) -> Result<bool> {
    if skill.is_dependency() {
        let environment = ProjectEnvironment::with_paths(
            directory,
            Path::new("pyproject.toml"),
            Path::new(".venv"),
            ScanDependencySelection::default(),
        );
        return Ok(available_dependency_skill_in(skill, &environment)?
            .is_some_and(|available| available.package_version != skill.package_version));
    }
    let Some(github_url) = skill.github_url.as_deref() else {
        return Ok(false);
    };
    let refreshed = discover_github_skills(
        client,
        github_url,
        if skill.is_skillsmp() {
            SKILLY_SOURCE_SKILLSMP
        } else {
            SKILLY_SOURCE_GITHUB
        },
        skill.skillsmp_id.clone(),
    )?
    .into_iter()
    .next()
    .context("GitHub URL resolves to no skills")?;
    Ok(!github_versions_match(skill, &refreshed))
}

fn update_available_or_remember_error(
    directory: &Path,
    skill: &SkillData,
    client: &SkillsMpClient,
    status_message: &mut Option<String>,
) -> bool {
    match skill_update_available(directory, skill, client) {
        Ok(available) => available,
        Err(error) => {
            *status_message = Some(format!(
                "Could not check updates for {}: {error}",
                skill_directory_name(skill)
            ));
            false
        }
    }
}

fn skillsmp_search_status(installed: Option<&SkillData>) -> &'static str {
    if installed.is_some() {
        STATUS_INSTALLED
    } else {
        STATUS_INSTALLABLE
    }
}

fn search_skill_label(skill: &SkillsMpSkill, installed: Option<&SkillData>) -> String {
    format!(
        "{} [{}] ({}) [{}]",
        skill.name,
        skill.author,
        skill.id,
        skillsmp_search_status(installed)
    )
}

fn skill_directory_name(skill: &SkillData) -> String {
    skill.directory_name()
}

fn installed_skill_label(skill: &SkillData) -> String {
    let mut details = Vec::new();
    if let Some(package_reference) = skill.package_reference() {
        details.push(package_reference);
    }
    if let Some(skillsmp_id) = skill.skillsmp_id.as_ref() {
        details.push(format!("id={skillsmp_id}"));
    }
    let detail_suffix = if details.is_empty() {
        String::new()
    } else {
        format!(" ({})", details.join(", "))
    };
    format!(
        "{}: {} [{}]{}",
        skill_directory_name(skill),
        skill.name,
        skill.source,
        detail_suffix
    )
}

fn invalid_installed_skill_label(skill: &InvalidInstalledSkill) -> String {
    format!("{}: invalid [invalid]", skill.directory_name)
}

fn listed_skill_label(entry: &ListedSkillEntry) -> String {
    match entry {
        ListedSkillEntry::Valid(skill) => installed_skill_label(skill),
        ListedSkillEntry::Invalid(skill) => invalid_installed_skill_label(skill),
    }
}

fn listed_skill_menu_status(entry: &ListedSkillEntry) -> MenuItemStatus {
    match entry {
        ListedSkillEntry::Valid(_) => MenuItemStatus::Default,
        ListedSkillEntry::Invalid(_) => MenuItemStatus::Disabled,
    }
}

fn scan_choice_label(item: &SkillMatchData) -> String {
    format!(
        "{} [{}] [{}] [{}]",
        item.available.name,
        item.available
            .package_reference()
            .unwrap_or_else(|| "unknown".to_string()),
        scan_dependency_label(&item.dependency_origins),
        scan_match_status(&item.available, item.installed.as_ref())
    )
}

fn scan_dependency_label(origins: &[ProjectDependencyOrigin]) -> String {
    if origins.is_empty() {
        return "unknown".to_string();
    }
    origins
        .iter()
        .map(ProjectDependencyOrigin::scan_label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn scan_skill_actions(item: &SkillMatchData) -> Vec<&'static str> {
    match scan_match_status(&item.available, item.installed.as_ref()) {
        STATUS_UPDATABLE => vec![UPDATE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        STATUS_INSTALLED => vec![BACK_CHOICE, EXIT_CHOICE],
        _ => vec![INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE],
    }
}

fn scan_menu_status(item: &SkillMatchData) -> MenuItemStatus {
    match scan_match_status(&item.available, item.installed.as_ref()) {
        STATUS_UPDATABLE => MenuItemStatus::Updatable,
        STATUS_INSTALLED => MenuItemStatus::Installed,
        _ => MenuItemStatus::Default,
    }
}

fn installed_skill_actions(update_available: bool, remove_choice: &str) -> Vec<&str> {
    let mut actions = vec![VIEW_FILES_CHOICE, remove_choice, BACK_CHOICE, EXIT_CHOICE];
    if update_available {
        actions.insert(1, UPDATE_CHOICE);
    }
    actions
}

fn invalid_installed_skill_actions(remove_choice: &str) -> Vec<&str> {
    vec![remove_choice, BACK_CHOICE, EXIT_CHOICE]
}

fn action_menu_default(actions: &[&str]) -> usize {
    actions
        .iter()
        .position(|action| {
            matches!(
                *action,
                INSTALL_CHOICE
                    | UPDATE_CHOICE
                    | VIEW_FILES_CHOICE
                    | APPLY_ALL_CHOICE
                    | INSTALL_ALL_CHOICE
                    | UPDATE_ALL_CHOICE
            )
        })
        .or_else(|| actions.iter().position(|action| *action == BACK_CHOICE))
        .unwrap_or(0)
}

fn retained_multi_select_indices(action: Option<&str>, indices: &[usize]) -> Vec<usize> {
    match action {
        None | Some(BACK_CHOICE) => indices.to_vec(),
        Some(_) => Vec::new(),
    }
}

fn menu_title_with_directory(title: String, directory: &Path) -> String {
    format!("{title} | Directory: {}", directory.display())
}

fn menu_title_for_destination(
    title: String,
    destinations: &[ResolvedDestination],
    active_tab: usize,
) -> String {
    if destinations.len() <= 1 {
        return menu_title_with_directory(title, &destinations[active_tab].path);
    }
    title
}

fn absolute_skill_path(skill: &SkillData) -> Option<PathBuf> {
    skill.path.as_deref().map(PathBuf::from)
}

fn target_skill_path(skill: &SkillData, directory: &Path) -> PathBuf {
    directory.join(skill_directory_name(skill))
}

fn no_skills_found_message(directory: &Path) -> String {
    format!("No skills found in directory {}", directory.display())
}

fn no_skills_found_message_anywhere() -> String {
    "No skills found in any managed directory".to_string()
}

fn pick_best_list_tab(destinations: &[ResolvedDestination], empty_flags: &[bool]) -> usize {
    for (i, dest) in destinations.iter().enumerate() {
        if !empty_flags[i] && dest.label.contains("local") {
            return i;
        }
    }
    first_non_empty_tab(empty_flags)
}

fn discover_installed_skills_report(directory: &Path) -> Result<InstalledSkillDiscoveryReport> {
    if !directory.exists() {
        return Ok(InstalledSkillDiscoveryReport::default());
    }
    if !directory.is_dir() {
        bail!("{}", directory.display());
    }

    let mut children =
        fs::read_dir(directory)?.collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    children.sort_by_key(|entry| entry.file_name());

    let mut report = InstalledSkillDiscoveryReport::default();
    for child in children {
        let child_path = child.path();
        if !child.file_type()?.is_dir() {
            continue;
        }
        match SkillData::from_dir_with_source_metadata(&child_path, &SkillSourceMetadata::default())
        {
            Ok(skill) => report.valid_skills.push(skill),
            Err(error) => report.invalid_skills.push(InvalidInstalledSkill {
                directory_name: child.file_name().to_string_lossy().to_string(),
                path: child_path,
                error: error.to_string(),
            }),
        }
    }
    Ok(report)
}

fn listed_skill_entries(report: InstalledSkillDiscoveryReport) -> Vec<ListedSkillEntry> {
    let mut entries = report
        .valid_skills
        .into_iter()
        .map(|skill| ListedSkillEntry::Valid(Box::new(skill)))
        .collect::<Vec<_>>();
    entries.extend(
        report
            .invalid_skills
            .into_iter()
            .map(ListedSkillEntry::Invalid),
    );
    entries
}

fn downloadable_skill_label(item: &DownloadableSkillMatch) -> String {
    format!(
        "{}: {} [{}]",
        skill_directory_name(&item.available),
        item.available.name,
        item.status()
    )
}

fn downloadable_skill_actions(item: &DownloadableSkillMatch) -> Vec<&'static str> {
    match item.status() {
        STATUS_INSTALLABLE => vec![INSTALL_CHOICE, VIEW_FILES_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        STATUS_UPDATABLE => {
            vec![
                UPDATE_CHOICE,
                VIEW_FILES_CHOICE,
                REMOVE_CHOICE,
                BACK_CHOICE,
                EXIT_CHOICE,
            ]
        }
        _ => vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
    }
}

fn downloadable_skill_menu_status(item: &DownloadableSkillMatch) -> MenuItemStatus {
    match item.status() {
        STATUS_INSTALLED => MenuItemStatus::Installed,
        STATUS_UPDATABLE => MenuItemStatus::Updatable,
        _ => MenuItemStatus::Default,
    }
}

fn skillsmp_search_menu_status(installed: Option<&SkillData>) -> MenuItemStatus {
    if installed.is_some() {
        MenuItemStatus::Installed
    } else {
        MenuItemStatus::Default
    }
}

fn downloadable_skill_matches(
    skills: &[SkillData],
    installed_skills: &[SkillData],
) -> Vec<DownloadableSkillMatch> {
    skills
        .iter()
        .map(|available| DownloadableSkillMatch {
            available: available.clone(),
            installed: installed_skills
                .iter()
                .find(|installed| {
                    (installed.source == SKILLY_SOURCE_GITHUB
                        || installed.source == SKILLY_SOURCE_SKILLSMP)
                        && available.matches(installed)
                })
                .cloned(),
        })
        .collect()
}

fn select_download_skill(skills: &[SkillData], skill_name: &str) -> Result<SkillData> {
    if let Some(skill) = skills
        .iter()
        .find(|skill| skill_directory_name(skill) == skill_name)
    {
        return Ok(skill.clone());
    }
    let matches = skills
        .iter()
        .filter(|skill| skill.name == skill_name)
        .cloned()
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches[0].clone()),
        0 => bail!(
            "downloadable skill not found: {}. available: {}",
            skill_name,
            skills
                .iter()
                .map(skill_directory_name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => bail!("multiple downloadable skills match name: {skill_name}"),
    }
}

fn exit_menu_item(label: &str) -> MenuItemUi {
    MenuItemUi {
        label: EXIT_CHOICE.to_string(),
        preview_lines: vec![label.to_string()],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    }
}

fn bundled_file_lines(skill: &SkillData) -> Vec<String> {
    std::iter::once("Files:".to_string())
        .chain(std::iter::once("  SKILL.md".to_string()))
        .chain(
            skill
                .resources
                .iter()
                .map(|resource| format!("  {}", resource.relative_path)),
        )
        .collect()
}

fn skill_preview_lines(skill: &SkillData, extra_lines: &[String]) -> Vec<String> {
    let mut lines = vec![
        format!("Name: {}", skill.name),
        format!("Description: {}", skill.description),
        format!("Source: {}", skill.source),
        format!("Installed: {}", skill.is_installed()),
    ];
    if let Some(skill_path) = absolute_skill_path(skill) {
        lines.push(format!("Skill Path: {}", skill_path.display()));
    }
    if let Some(package_reference) = skill.package_reference() {
        lines.push(format!("Package: {package_reference}"));
    }
    if let Some(github_url) = skill.github_url.as_ref() {
        lines.push(format!("GitHub Url: {github_url}"));
    }
    if let Some(github_commit_sha) = skill.github_commit_sha.as_ref() {
        lines.push(format!("GitHub Commit: {github_commit_sha}"));
    }
    if let Some(skillsmp_id) = skill.skillsmp_id.as_ref() {
        lines.push(format!("SkillsMP Id: {skillsmp_id}"));
    }
    if !extra_lines.is_empty() {
        lines.push(String::new());
        lines.extend(extra_lines.iter().cloned());
    }
    lines.push(String::new());
    lines.extend(bundled_file_lines(skill));
    lines
}

fn scan_match_preview_lines(item: &SkillMatchData) -> Vec<String> {
    let mut extra = vec![format!(
        "Status: {}",
        scan_match_status(&item.available, item.installed.as_ref())
    )];
    extra.push(format!(
        "Dependency Sources: {}",
        scan_dependency_label(&item.dependency_origins)
    ));
    for origin in &item.dependency_origins {
        extra.push(format!("  - {}", origin.detail_label()));
    }
    if let Some(installed) = item.installed.as_ref() {
        extra.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
        if let Some(skill_path) = absolute_skill_path(installed) {
            extra.push(format!("Installed Path: {}", skill_path.display()));
        }
    }
    skill_preview_lines(&item.available, &extra)
}

fn installed_skill_preview_lines(skill: &SkillData) -> Vec<String> {
    skill_preview_lines(skill, &[])
}

fn invalid_installed_skill_preview_lines(skill: &InvalidInstalledSkill) -> Vec<String> {
    vec![
        format!("Directory: {}", skill.directory_name),
        format!("Status: invalid"),
        format!("Path: {}", skill.path.display()),
        String::new(),
        "Error:".to_string(),
        skill.error.clone(),
    ]
}

fn listed_skill_preview_lines(entry: &ListedSkillEntry) -> Vec<String> {
    match entry {
        ListedSkillEntry::Valid(skill) => installed_skill_preview_lines(skill),
        ListedSkillEntry::Invalid(skill) => invalid_installed_skill_preview_lines(skill),
    }
}

fn skillsmp_search_preview_lines(
    skill: &SkillsMpSkill,
    installed: Option<&SkillData>,
    directory: &Path,
) -> Vec<String> {
    let mut lines = vec![
        format!("Name: {}", skill.name),
        format!("Description: {}", skill.description),
        format!("Author: {}", skill.author),
        format!("Status: {}", skillsmp_search_status(installed)),
        format!("Destination Directory: {}", directory.display()),
        format!("SkillsMP Url: {}", skill.skill_url),
        format!("GitHub Url: {}", skill.github_url),
        format!("SkillsMP Id: {}", skill.id),
    ];
    if let Some(installed) = installed {
        lines.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
        if let Some(skill_path) = absolute_skill_path(installed) {
            lines.push(format!("Installed Path: {}", skill_path.display()));
        }
    }
    if let Some(stars) = skill.stars {
        lines.push(format!("Stars: {stars}"));
    }
    if let Some(updated_at) = skill.updated_at.as_ref() {
        lines.push(format!("Updated At: {}", updated_at));
    }
    lines
}

fn skillsmp_installable_preview_lines(
    skill: &SkillsMpSkill,
    download_match: &DownloadableSkillMatch,
    directory: &Path,
) -> Vec<String> {
    let mut lines =
        skillsmp_search_preview_lines(skill, download_match.installed.as_ref(), directory);
    lines.push(format!("Resolved Status: {}", download_match.status()));
    lines.push(format!(
        "Target Skill Path: {}",
        target_skill_path(&download_match.available, directory).display()
    ));
    lines.push(String::new());
    lines.extend(bundled_file_lines(&download_match.available));
    lines
}

fn installed_skillsmp_match(
    skill: &SkillsMpSkill,
    installed_skills: &[SkillData],
) -> Option<SkillData> {
    installed_skills
        .iter()
        .find(|installed| {
            installed.skillsmp_id.as_deref() == Some(skill.id.as_str())
                || installed.github_url.as_deref() == Some(skill.github_url.as_str())
        })
        .cloned()
}

fn downloadable_skill_preview_lines(
    item: &DownloadableSkillMatch,
    directory: &Path,
) -> Vec<String> {
    let mut extra = vec![
        format!("Status: {}", item.status()),
        format!("Destination Directory: {}", directory.display()),
        format!(
            "Target Skill Path: {}",
            target_skill_path(&item.available, directory).display()
        ),
    ];
    if let Some(installed) = item.installed.as_ref() {
        extra.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
        if let Some(skill_path) = absolute_skill_path(installed) {
            extra.push(format!("Installed Path: {}", skill_path.display()));
        }
    }
    skill_preview_lines(&item.available, &extra)
}

#[cfg(test)]
mod tests {
    use super::args::{
        Cli, Commands, CreateAction, ScanDependencyArgs, next_non_empty_tab_index,
        next_selectable_index, next_tab_index, previous_non_empty_tab_index,
        previous_selectable_index, previous_tab_index,
    };
    use super::tui::{
        DownloadableSkillMatch, MenuAction, MenuItemStatus, MenuItemUi, TextBuffer,
        adjust_focused_on_filter, build_visible_indices, create_action, filter_matches,
        filterable_count, menu_action, visible_position,
    };
    use super::{
        APPLY_ALL_CHOICE, BACK_CHOICE, EXIT_CHOICE, INSTALL_ALL_CHOICE, INSTALL_CHOICE,
        PendingSkillUpdate, REMOVE_CHOICE, UPDATE_ALL_CHOICE, UPDATE_CHOICE, VIEW_FILES_CHOICE,
        action_menu_default, downloadable_skill_actions, downloadable_skill_menu_status,
        downloadable_skill_preview_lines, exit_menu_item, format_pending_update,
        installed_skill_actions, installed_skill_preview_lines, installed_skillsmp_match,
        retained_multi_select_indices, scan_choice_label, scan_match_preview_lines,
        search_skill_label, skillsmp_search_preview_lines, skillsmp_search_status,
    };
    use crate::client::SkillsMpSkill;
    use crate::core::{
        NamedSelection, ProjectDependencyOrigin, SKILLY_SOURCE_DEPENDENCY, SKILLY_SOURCE_GITHUB,
        SKILLY_SOURCE_SKILLSMP, SkillData, SkillMatchData,
    };
    use clap::Parser;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use serde_json::Value as JsonValue;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn installed_skill(skillsmp_id: Option<&str>, github_url: Option<&str>) -> SkillData {
        SkillData {
            name: "python".to_string(),
            description: "Installed skill".to_string(),
            path: Some("/tmp/python".to_string()),
            content: "Body".to_string(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools: None,
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: if skillsmp_id.is_some() {
                SKILLY_SOURCE_SKILLSMP.to_string()
            } else {
                SKILLY_SOURCE_GITHUB.to_string()
            },
            package_name: None,
            package_version: None,
            github_url: github_url.map(str::to_string),
            github_commit_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            skillsmp_id: skillsmp_id.map(str::to_string),
            package_ecosystem: None,
        }
    }

    fn dependency_match(origins: Vec<ProjectDependencyOrigin>) -> SkillMatchData {
        SkillMatchData {
            available: SkillData {
                name: "python".to_string(),
                description: "Available skill".to_string(),
                path: None,
                content: "Body".to_string(),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools: None,
                resources: Vec::new(),
                resource_warnings: Vec::new(),
                source: SKILLY_SOURCE_DEPENDENCY.to_string(),
                package_name: Some("ruff".to_string()),
                package_version: Some("0.12.0".to_string()),
                github_url: None,
                github_commit_sha: None,
                skillsmp_id: None,
                package_ecosystem: None,
            },
            installed: None,
            dependency_origins: origins,
        }
    }

    fn search_result() -> SkillsMpSkill {
        SkillsMpSkill {
            id: "skill-1".to_string(),
            name: "python-production".to_string(),
            author: "idossha".to_string(),
            description: "Python production code patterns.".to_string(),
            github_url: "https://github.com/example/project/tree/main/skills/python".to_string(),
            skill_url: "https://skillsmp.com/skills/skill-1".to_string(),
            stars: Some(42),
            updated_at: Some(JsonValue::String("1778091502".to_string())),
        }
    }

    #[test]
    fn skillsmp_search_detects_installed_skill_by_id() {
        let matched =
            installed_skillsmp_match(&search_result(), &[installed_skill(Some("skill-1"), None)]);

        assert!(matched.is_some());
        assert_eq!(skillsmp_search_status(matched.as_ref()), "installed");
    }

    #[test]
    fn skillsmp_search_detects_installed_skill_by_github_url() {
        let matched = installed_skillsmp_match(
            &search_result(),
            &[installed_skill(
                None,
                Some("https://github.com/example/project/tree/main/skills/python"),
            )],
        );

        assert!(matched.is_some());
    }

    #[test]
    fn skillsmp_search_label_and_preview_include_installed_status() {
        let matched = installed_skill(Some("skill-1"), None);

        let label = search_skill_label(&search_result(), Some(&matched));
        let preview = skillsmp_search_preview_lines(
            &search_result(),
            Some(&matched),
            Path::new("/tmp/install"),
        );

        assert_eq!(label, "python-production [idossha] (skill-1) [installed]");
        assert!(preview.iter().any(|line| line == "Status: installed"));
        assert!(
            preview
                .iter()
                .any(|line| line == "Installed Directory: python")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "Installed Path: /tmp/python")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "Destination Directory: /tmp/install")
        );
    }

    #[test]
    fn menu_action_ignores_non_press_events() {
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            None
        );
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            )),
            None
        );
    }

    #[test]
    fn menu_action_maps_press_events() {
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(MenuAction::Select)
        );
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(MenuAction::MoveDown)
        );
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Up,
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(MenuAction::MoveUp)
        );
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Esc,
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(MenuAction::Cancel)
        );
        assert_eq!(
            menu_action(KeyEvent::new_with_kind(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            )),
            Some(MenuAction::Cancel)
        );
    }

    #[test]
    fn create_action_maps_create_and_cancel_shortcuts() {
        assert_eq!(
            create_action(KeyEvent::new_with_kind(
                KeyCode::Char('s'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            )),
            Some(CreateAction::Save)
        );
        assert_eq!(
            create_action(KeyEvent::new_with_kind(
                KeyCode::F(2),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(CreateAction::Save)
        );
        assert_eq!(
            create_action(KeyEvent::new_with_kind(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            )),
            Some(CreateAction::Cancel)
        );
        assert_eq!(
            create_action(KeyEvent::new_with_kind(
                KeyCode::F(10),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(CreateAction::Cancel)
        );
    }

    #[test]
    fn text_buffer_supports_multiline_editing() {
        let mut buffer = TextBuffer::from_text("line 1");

        buffer.move_end();
        buffer.insert_newline();
        buffer.insert_char('l');
        buffer.insert_char('i');
        buffer.insert_char('n');
        buffer.insert_char('e');
        buffer.insert_char(' ');
        buffer.insert_char('2');
        buffer.move_up();
        buffer.move_end();
        buffer.insert_char('!');

        assert_eq!(buffer.text(), "line 1!\nline 2");
    }

    #[test]
    fn destination_flags_resolve_for_directory_commands() {
        let cli = Cli::try_parse_from([
            "skilly",
            "download",
            "https://github.com/example/repo",
            "--global",
            "--copilot",
        ])
        .expect("download command should parse");
        let Commands::Download { destination, .. } = cli.command else {
            panic!("expected download command");
        };

        assert_eq!(
            destination.resolve().expect("destination should resolve"),
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME should be set"))
                .join(".copilot/skills")
        );
    }

    #[test]
    fn explicit_directory_ignores_all_destination_flags() {
        let cli = Cli::try_parse_from([
            "skilly",
            "list",
            "--directory",
            "custom",
            "--local",
            "--global",
            "--claude",
            "--codex",
            "--copilot",
        ])
        .expect("list command should parse");
        let Commands::List { destination, .. } = cli.command else {
            panic!("expected list command");
        };

        assert_eq!(
            destination.resolve().expect("destination should resolve"),
            std::env::current_dir()
                .expect("current directory should resolve")
                .join("custom")
        );
    }

    #[test]
    fn absolute_path_expands_home_and_relative_paths() {
        assert_eq!(
            crate::core::absolute_path(Path::new("custom")).expect("relative path should resolve"),
            std::env::current_dir()
                .expect("current directory should resolve")
                .join("custom")
        );
        assert_eq!(
            crate::core::absolute_path(Path::new("~/.copilot")).expect("home path should resolve"),
            PathBuf::from(std::env::var_os("HOME").expect("HOME should be set")).join(".copilot")
        );
    }

    #[test]
    fn agent_destination_flags_resolve_to_absolute_paths() {
        let cases = [
            (
                ["skilly", "list", "--claude"],
                std::env::current_dir()
                    .expect("current directory should resolve")
                    .join(".claude/skills"),
            ),
            (
                ["skilly", "list", "--codex"],
                std::env::current_dir()
                    .expect("current directory should resolve")
                    .join(".codex/skills"),
            ),
            (
                ["skilly", "list", "--copilot"],
                std::env::current_dir()
                    .expect("current directory should resolve")
                    .join(".github/skills"),
            ),
        ];

        for (args, expected) in cases {
            let cli = Cli::try_parse_from(args).expect("list command should parse");
            let Commands::List { destination, .. } = cli.command else {
                panic!("expected list command");
            };
            assert_eq!(
                destination.resolve().expect("destination should resolve"),
                expected
            );
        }
    }

    #[test]
    fn update_command_accepts_yes_and_github_token() {
        let cli = Cli::try_parse_from(["skilly", "update", "--yes", "--github-token", "token"])
            .expect("update command should parse");
        let Commands::Update {
            yes, github_token, ..
        } = cli.command
        else {
            panic!("expected update command");
        };

        assert!(yes);
        assert_eq!(github_token.as_deref(), Some("token"));
    }

    #[test]
    fn installed_skill_actions_only_include_update_when_available() {
        assert_eq!(
            installed_skill_actions(false, REMOVE_CHOICE),
            vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
        );
        assert_eq!(
            installed_skill_actions(true, REMOVE_CHOICE),
            vec![
                VIEW_FILES_CHOICE,
                UPDATE_CHOICE,
                REMOVE_CHOICE,
                BACK_CHOICE,
                EXIT_CHOICE
            ]
        );
    }

    #[test]
    fn action_menu_default_prefers_safe_or_primary_action() {
        assert_eq!(
            action_menu_default(&[REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]),
            1
        );
        assert_eq!(
            action_menu_default(&[UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]),
            0
        );
        assert_eq!(
            action_menu_default(&[REMOVE_CHOICE, UPDATE_ALL_CHOICE, BACK_CHOICE, EXIT_CHOICE,]),
            1
        );
        assert_eq!(
            action_menu_default(&[INSTALL_ALL_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE,]),
            0
        );
        assert_eq!(
            action_menu_default(&[APPLY_ALL_CHOICE, BACK_CHOICE, EXIT_CHOICE]),
            0
        );
        assert_eq!(
            action_menu_default(&[INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE]),
            0
        );
    }

    #[test]
    fn retained_multi_select_indices_only_keeps_selection_for_back_or_cancel() {
        let indices = vec![1, 3];

        assert_eq!(retained_multi_select_indices(None, &indices), indices);
        assert_eq!(
            retained_multi_select_indices(Some(BACK_CHOICE), &[1, 3]),
            vec![1, 3]
        );
        assert!(retained_multi_select_indices(Some(UPDATE_CHOICE), &[1, 3]).is_empty());
        assert!(retained_multi_select_indices(Some(INSTALL_CHOICE), &[1, 3]).is_empty());
    }

    #[test]
    fn tab_navigation_skips_empty_tabs() {
        let empty_flags = [false, true, false, true];

        assert_eq!(next_non_empty_tab_index(0, &empty_flags), 2);
        assert_eq!(next_non_empty_tab_index(2, &empty_flags), 0);
        assert_eq!(previous_non_empty_tab_index(2, &empty_flags), 0);
        assert_eq!(previous_non_empty_tab_index(0, &empty_flags), 2);
    }

    #[test]
    fn tab_navigation_wraps_when_empty_tabs_are_allowed() {
        assert_eq!(next_tab_index(0, 4), 1);
        assert_eq!(next_tab_index(3, 4), 0);
        assert_eq!(previous_tab_index(0, 4), 3);
        assert_eq!(previous_tab_index(2, 4), 1);
    }

    #[test]
    fn list_navigation_skips_non_selectable_invalid_entries() {
        let items = vec![
            MenuItemUi {
                label: "python".to_string(),
                preview_lines: Vec::new(),
                status: MenuItemStatus::Default,
                selectable: true,
                filter_text: None,
            },
            MenuItemUi {
                label: ".system".to_string(),
                preview_lines: Vec::new(),
                status: MenuItemStatus::Disabled,
                selectable: false,
                filter_text: None,
            },
            exit_menu_item("Exit list"),
        ];

        assert_eq!(next_selectable_index(&items, 0), 2);
        assert_eq!(previous_selectable_index(&items, 2), 0);
    }

    #[test]
    fn downloadable_skill_menu_status_marks_updatable_entries() {
        let installed = installed_skill(None, Some("https://github.com/example/repo"));
        let available = SkillData {
            github_commit_sha: Some("fedcba98765432100123456789abcdef01234567".to_string()),
            ..installed.clone()
        };
        let matched = DownloadableSkillMatch {
            available,
            installed: Some(installed),
        };

        assert_eq!(
            downloadable_skill_menu_status(&matched),
            MenuItemStatus::Updatable
        );
    }

    #[test]
    fn downloadable_skill_actions_omit_update_when_versions_match() {
        let installed = installed_skill(None, Some("https://github.com/example/repo"));
        let available = installed.clone();
        let matched = DownloadableSkillMatch {
            available,
            installed: Some(installed),
        };

        assert_eq!(
            downloadable_skill_actions(&matched),
            vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
        );
    }

    #[test]
    fn pending_github_update_preview_uses_short_commits() {
        let update = PendingSkillUpdate {
            installed: installed_skill(
                None,
                Some("https://github.com/example/project/tree/main/skills/python"),
            ),
            available: SkillData {
                github_commit_sha: Some("fedcba98765432100123456789abcdef01234567".to_string()),
                ..installed_skill(
                    None,
                    Some("https://github.com/example/project/tree/main/skills/python"),
                )
            },
        };

        assert_eq!(
            format_pending_update(&update),
            "python [github]: 0123456 -> fedcba9"
        );
    }

    #[test]
    fn installed_skill_preview_includes_absolute_skill_path() {
        let preview = installed_skill_preview_lines(&installed_skill(None, None));

        assert!(preview.iter().any(|line| line == "Skill Path: /tmp/python"));
    }

    #[test]
    fn downloadable_skill_preview_includes_absolute_destination_and_target_paths() {
        let preview = downloadable_skill_preview_lines(
            &DownloadableSkillMatch {
                available: SkillData {
                    name: "python".to_string(),
                    description: "Downloadable skill".to_string(),
                    path: None,
                    content: "Body".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: BTreeMap::new(),
                    allowed_tools: None,
                    resources: Vec::new(),
                    resource_warnings: Vec::new(),
                    source: SKILLY_SOURCE_GITHUB.to_string(),
                    package_name: None,
                    package_version: None,
                    github_url: Some(
                        "https://github.com/example/project/tree/main/skills/python".to_string(),
                    ),
                    github_commit_sha: None,
                    skillsmp_id: None,
                    package_ecosystem: None,
                },
                installed: Some(installed_skill(None, None)),
            },
            Path::new("/tmp/install"),
        );

        assert!(
            preview
                .iter()
                .any(|line| line == "Destination Directory: /tmp/install")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "Target Skill Path: /tmp/install/python")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "Installed Path: /tmp/python")
        );
    }

    #[test]
    fn scan_dependency_args_default_to_including_all_dependency_sources() {
        let selection = ScanDependencyArgs::default()
            .selection()
            .expect("default selection should succeed");

        assert!(selection.include_project_dependencies);
        assert_eq!(selection.dependency_groups, NamedSelection::All);
        assert_eq!(selection.optional_dependencies, NamedSelection::All);
    }

    #[test]
    fn scan_dependency_args_reject_conflicting_named_filters() {
        let error = ScanDependencyArgs {
            groups: vec!["dev".to_string()],
            exclude_groups: vec!["docs".to_string()],
            ..ScanDependencyArgs::default()
        }
        .selection()
        .expect_err("conflicting group filters should fail");

        assert!(
            error
                .to_string()
                .contains("Include and exclude filters cannot be combined")
        );
    }

    #[test]
    fn scan_choice_label_and_preview_include_dependency_origins() {
        let item = dependency_match(vec![
            ProjectDependencyOrigin::PythonProject,
            ProjectDependencyOrigin::PythonDependencyGroup {
                group: "dev".to_string(),
            },
            ProjectDependencyOrigin::PythonOptionalDependency {
                extra: "docs".to_string(),
            },
        ]);

        let label = scan_choice_label(&item);
        let preview = scan_match_preview_lines(&item);

        assert_eq!(
            label,
            "python [ruff==0.12.0] [python:project, python:group:dev, python:extra:docs] [installable]"
        );
        assert!(preview.iter().any(|line| line
            == "Dependency Sources: python:project, python:group:dev, python:extra:docs"));
        assert!(
            preview
                .iter()
                .any(|line| line == "  - python dependency group: dev")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "  - python optional dependency: docs")
        );
    }

    // --- file viewer tests ---

    use super::tui::{
        build_file_tree, compute_visible, file_viewer_move_selection_down,
        file_viewer_move_selection_up,
    };
    use crate::core::SkillResourceData;
    use std::collections::HashSet;

    fn resource(path: &str, content: &str) -> SkillResourceData {
        SkillResourceData {
            relative_path: path.to_string(),
            kind: "other".to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn file_tree_always_has_skill_md_first() {
        let skill = SkillData {
            content: "# Title\n\nBody".to_string(),
            resources: vec![],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "SKILL.md");
        assert_eq!(tree[0].content, "# Title\n\nBody");
        assert_eq!(tree[0].depth, 0);
        assert!(!tree[0].is_dir);
    }

    #[test]
    fn file_tree_builds_flat_resources() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("README.md", "readme content"),
                resource("setup.py", "setup content"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        assert_eq!(tree.len(), 3);
        // SKILL.md first
        assert_eq!(tree[0].name, "SKILL.md");
        assert_eq!(tree[0].depth, 0);
        // Then files in sorted order
        assert_eq!(tree[1].name, "README.md");
        assert_eq!(tree[1].depth, 0);
        assert_eq!(tree[1].content, "readme content");
        assert!(!tree[1].is_dir);
        assert_eq!(tree[2].name, "setup.py");
        assert_eq!(tree[2].depth, 0);
    }

    #[test]
    fn file_tree_creates_directory_entries() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("scripts/run.py", "print('hello')"),
                resource("references/api.md", "# API"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        // Alphabetical: references/ before scripts/
        // SKILL.md, references/, api.md, scripts/, run.py
        assert_eq!(tree.len(), 5);

        assert_eq!(tree[0].name, "SKILL.md");
        assert_eq!(tree[0].depth, 0);
        assert!(!tree[0].is_dir);

        assert_eq!(tree[1].name, "references");
        assert_eq!(tree[1].depth, 0);
        assert!(tree[1].is_dir);
        assert_eq!(tree[1].relative_path, "references/");

        assert_eq!(tree[2].name, "api.md");
        assert_eq!(tree[2].depth, 1);
        assert!(!tree[2].is_dir);

        assert_eq!(tree[3].name, "scripts");
        assert_eq!(tree[3].depth, 0);
        assert!(tree[3].is_dir);

        assert_eq!(tree[4].name, "run.py");
        assert_eq!(tree[4].depth, 1);
        assert!(!tree[4].is_dir);
    }

    #[test]
    fn file_tree_handles_nested_directories() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("assets/icons/logo.svg", "<svg></svg>"),
                resource("scripts/sub/helper.py", "# helper"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        // Order: SKILL.md, assets/, icons/, logo.svg, scripts/, sub/, helper.py
        let paths: Vec<&str> = tree.iter().map(|e| e.relative_path.as_str()).collect();

        assert!(paths[1].starts_with("assets"));
        assert!(paths[2] == "assets/icons/");
        assert!(paths[3] == "assets/icons/logo.svg");
        assert!(paths[4].starts_with("scripts"));
        assert!(paths[5] == "scripts/sub/");
        assert!(paths[6] == "scripts/sub/helper.py");

        // Check depths
        assert_eq!(tree[1].depth, 0); // assets/
        assert_eq!(tree[2].depth, 1); // assets/icons/
        assert_eq!(tree[3].depth, 2); // logo.svg
        assert_eq!(tree[4].depth, 0); // scripts/
        assert_eq!(tree[5].depth, 1); // scripts/sub/
        assert_eq!(tree[6].depth, 2); // helper.py
    }

    #[test]
    fn file_tree_multiple_dirs_sorted() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("z/back/file.txt", "back"),
                resource("a/front/file.txt", "front"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        // Should be sorted: a/... first, then z/...
        assert_eq!(tree[1].name, "a");
        // Find z dir somewhere after a
        let z_pos = tree.iter().position(|e| e.name == "z").unwrap();
        let a_pos = tree.iter().position(|e| e.name == "a").unwrap();
        assert!(a_pos < z_pos);
    }

    #[test]
    fn compute_visible_shows_all_when_nothing_collapsed() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("scripts/run.py", "print('hello')"),
                resource("references/api.md", "# API"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        let collapsed = HashSet::new();
        let visible = compute_visible(&tree, &collapsed);

        assert_eq!(visible.len(), tree.len());
    }

    #[test]
    fn compute_visible_hides_children_of_collapsed_dir() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("scripts/run.py", "print('hello')"),
                resource("scripts/helper.py", "# helper"),
                resource("references/api.md", "# API"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        // Alphabetical: references/ before scripts/
        // 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py, 5=helper.py

        let mut collapsed = HashSet::new();
        collapsed.insert("scripts/".to_string());

        let visible = compute_visible(&tree, &collapsed);
        let visible_paths: Vec<&str> = visible
            .iter()
            .map(|&i| tree[i].relative_path.as_str())
            .collect();

        // scripts/ children (run.py, helper.py) should be hidden
        assert!(visible_paths.contains(&"SKILL.md"));
        assert!(visible_paths.contains(&"references/"));
        assert!(visible_paths.contains(&"references/api.md"));
        assert!(visible_paths.contains(&"scripts/"));
        assert!(!visible_paths.contains(&"scripts/run.py"));
        assert!(!visible_paths.contains(&"scripts/helper.py"));
    }

    #[test]
    fn compute_visible_hides_nested_children() {
        let skill = SkillData {
            content: "Body".to_string(),
            resources: vec![
                resource("a/b/c/file.txt", "deep"),
                resource("a/other.txt", "other"),
            ],
            ..installed_skill(None, None)
        };

        let tree = build_file_tree(&skill);
        // SKILL.md, a/, b/, c/, file.txt, other.txt
        let mut collapsed = HashSet::new();
        collapsed.insert("a/".to_string());

        let visible = compute_visible(&tree, &collapsed);
        let visible_paths: Vec<&str> = visible
            .iter()
            .map(|&i| tree[i].relative_path.as_str())
            .collect();

        // a/ is visible but all descendants (b/, c/, file.txt, other.txt) are hidden
        assert!(visible_paths.contains(&"SKILL.md"));
        assert!(visible_paths.contains(&"a/"));
        assert!(!visible_paths.contains(&"a/b/"));
        assert!(!visible_paths.contains(&"a/b/c/"));
        assert!(!visible_paths.contains(&"a/b/c/file.txt"));
        assert!(!visible_paths.contains(&"a/other.txt"));
    }

    #[test]
    fn file_viewer_move_selection_navigates_visible_only() {
        // Tree: 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py
        // (alphabetical: references/ before scripts/)
        let skill = SkillData {
            content: "B".to_string(),
            resources: vec![
                resource("scripts/run.py", "x"),
                resource("references/api.md", "x"),
            ],
            ..installed_skill(None, None)
        };
        let tree = build_file_tree(&skill);

        let mut collapsed = HashSet::new();
        collapsed.insert("scripts/".to_string());

        let visible = compute_visible(&tree, &collapsed);
        // visible: [0=SKILL.md, 1=references/, 2=api.md, 3=scripts/]

        // Down from SKILL.md (0) -> references/ (1)
        assert_eq!(file_viewer_move_selection_down(&visible, 0), 1);
        // Down from references/ (1) -> api.md (2)
        assert_eq!(file_viewer_move_selection_down(&visible, 1), 2);
        // Down from api.md (2) -> scripts/ (3)
        assert_eq!(file_viewer_move_selection_down(&visible, 2), 3);
        // Down at end -> stays
        assert_eq!(file_viewer_move_selection_down(&visible, 3), 3);

        // Up from scripts/ (3) -> api.md (2)
        assert_eq!(file_viewer_move_selection_up(&visible, 3), 2);
        // Up from api.md (2) -> references/ (1)
        assert_eq!(file_viewer_move_selection_up(&visible, 2), 1);
        // Up from references/ (1) -> SKILL.md (0)
        assert_eq!(file_viewer_move_selection_up(&visible, 1), 0);
        // Up at start -> stays
        assert_eq!(file_viewer_move_selection_up(&visible, 0), 0);
    }

    #[test]
    fn file_viewer_move_selection_handles_current_becoming_hidden() {
        let skill = SkillData {
            content: "B".to_string(),
            resources: vec![
                resource("scripts/run.py", "x"),
                resource("references/api.md", "x"),
            ],
            ..installed_skill(None, None)
        };
        let tree = build_file_tree(&skill);
        // 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py

        let mut collapsed = HashSet::new();
        collapsed.insert("scripts/".to_string());

        let visible = compute_visible(&tree, &collapsed);
        // visible: [0, 1, 2, 3]

        // Moving down from run.py (4, hidden) should go to next visible: scripts/ (3 is the next visible? Actually 4 is hidden, so find first visible at or after 4)
        // No visible at or after 4, so should wrap to the first: 0
        // But looking at the function: visible.iter().find(|&&i| i >= current) - no match for >= 4, so unwrap_or(*visible.last() = 3)
        assert_eq!(file_viewer_move_selection_down(&visible, 4), 3);
        // Moving up from run.py (4, hidden) should find first visible >= 4: actually none, but then return first visible
        // Wait: visible.iter().find(|&&i| i >= current) -> no match -> unwrap_or(visible[0]) -> 0
        assert_eq!(file_viewer_move_selection_up(&visible, 4), 3);
    }

    // --- Filtering tests ---

    fn skill_item(name: &str) -> MenuItemUi {
        MenuItemUi {
            label: name.to_string(),
            preview_lines: Vec::new(),
            status: MenuItemStatus::Default,
            selectable: true,
            filter_text: Some(name.to_string()),
        }
    }

    fn exit_item() -> MenuItemUi {
        MenuItemUi {
            label: "Exit".to_string(),
            preview_lines: vec!["Exit menu".to_string()],
            status: MenuItemStatus::Default,
            selectable: true,
            filter_text: None,
        }
    }

    fn disabled_item(label: &str) -> MenuItemUi {
        MenuItemUi {
            label: label.to_string(),
            preview_lines: Vec::new(),
            status: MenuItemStatus::Disabled,
            selectable: false,
            filter_text: Some(label.to_string()),
        }
    }

    #[test]
    fn filter_matches_non_filterable_always_returns_true() {
        let item = exit_item();
        assert!(filter_matches("", &item));
    }

    #[test]
    fn filter_matches_non_filterable_returns_false_when_filtering() {
        let item = exit_item();
        assert!(!filter_matches("xyz", &item));
    }

    #[test]
    fn filter_matches_case_insensitive_substring() {
        let item = skill_item("Python");
        assert!(filter_matches("p", &item));
        assert!(filter_matches("PY", &item));
        assert!(filter_matches("thon", &item));
        assert!(!filter_matches("xyz", &item));
    }

    #[test]
    fn filterable_count_counts_only_filterable_items() {
        let items = vec![skill_item("a"), exit_item(), skill_item("b")];
        assert_eq!(filterable_count(&items), 2);
    }

    #[test]
    fn filterable_count_returns_zero_when_none_filterable() {
        let items = vec![exit_item(), exit_item()];
        assert_eq!(filterable_count(&items), 0);
    }

    #[test]
    fn build_visible_indices_empty_filter_returns_all() {
        let items = vec![skill_item("a"), exit_item(), skill_item("b")];
        assert_eq!(build_visible_indices(&items, ""), vec![0, 1, 2]);
    }

    #[test]
    fn build_visible_indices_filters_by_name() {
        let items = vec![skill_item("alpha"), exit_item(), skill_item("beta")];
        assert_eq!(build_visible_indices(&items, "alpha"), vec![0]);
    }

    #[test]
    fn build_visible_indices_no_match_returns_empty() {
        let items = vec![skill_item("alpha"), exit_item(), skill_item("beta")];
        assert_eq!(build_visible_indices(&items, "xyz"), Vec::<usize>::new());
    }

    #[test]
    fn build_visible_indices_disabled_filterable_still_visible_when_matched() {
        let items = vec![skill_item("alpha"), disabled_item("beta")];
        let visible = build_visible_indices(&items, "beta");
        assert_eq!(visible, vec![1]);
    }

    #[test]
    fn visible_position_finds_index_in_visible() {
        let visible = vec![2, 5, 7];
        assert_eq!(visible_position(&visible, 5), Some(1));
        assert_eq!(visible_position(&visible, 2), Some(0));
        assert_eq!(visible_position(&visible, 99), None);
    }

    #[test]
    fn visible_position_empty_list_returns_none() {
        let visible: Vec<usize> = vec![];
        assert_eq!(visible_position(&visible, 0), None);
    }

    #[test]
    fn adjust_focused_on_filter_changes_when_not_in_visible() {
        let visible = vec![2, 5, 7];
        let mut focused = 3;
        adjust_focused_on_filter(&visible, &mut focused);
        assert_eq!(focused, 2);
    }

    #[test]
    fn adjust_focused_on_filter_preserves_when_in_visible() {
        let visible = vec![2, 5, 7];
        let mut focused = 5;
        adjust_focused_on_filter(&visible, &mut focused);
        assert_eq!(focused, 5);
    }

    #[test]
    fn adjust_focused_on_filter_empty_visible_does_nothing() {
        let visible: Vec<usize> = vec![];
        let mut focused = 3;
        adjust_focused_on_filter(&visible, &mut focused);
        assert_eq!(focused, 3);
    }
}
