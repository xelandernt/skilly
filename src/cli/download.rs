use super::*;

pub(super) struct DownloadRequest {
    pub(super) repository_url: String,
    pub(super) provider: Option<RepositoryProvider>,
    pub(super) skill_name: Option<String>,
    pub(super) all: bool,
    pub(super) overwrite: bool,
}

pub(super) fn run_download(
    request: &DownloadRequest,
    destination: &DestinationArgs,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    let client = SkillsMpClient::new(config)?;
    let mut skills =
        discover_repository_skills(&client, &request.repository_url, request.provider)?;
    let directory = destination.resolve()?;
    if request.all && request.skill_name.is_some() && skills.len() != 1 {
        bail!("use either --skill-name or --all when downloading multiple skills");
    }
    let interactive_destinations = if is_interactive_terminal() {
        destination.resolve_configured_destinations(skilly_config)?
    } else {
        Vec::new()
    };
    if is_interactive_terminal() && (skills.len() > 1 || interactive_destinations.len() > 1) {
        if skills.len() > 1 && !request.all && request.skill_name.is_none() {
            return download_selected_skills(
                &client,
                &interactive_destinations,
                request.overwrite,
                &skills,
            );
        }
        if let Some(skill_name) = request.skill_name.as_deref() {
            if skills.len() != 1 && request.all {
                bail!("custom skill names can only be used when downloading a single skill");
            }
            if skills.len() != 1 {
                skills = vec![select_download_skill(&skills, skill_name)?];
            }
        }
        return download_selected_skills(
            &client,
            &interactive_destinations,
            request.overwrite,
            &skills,
        );
    }
    if skills.len() > 1 && !request.all && request.skill_name.is_none() {
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
            request.overwrite,
            &skills,
        );
    }
    if let Some(skill_name) = request.skill_name.as_deref() {
        if skills.len() != 1 && request.all {
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
                if skills.len() == 1 {
                    request.skill_name.as_deref()
                } else {
                    None
                },
                request.overwrite,
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

pub(super) fn download_selected_skills(
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
                label: item.available.name.clone(),
                subtitle: Some(skill_source_label(&item.available)),
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
                                    subtitle: None,
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
                                    subtitle: None,
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
