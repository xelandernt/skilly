use super::*;

pub(super) fn run_skillsmp_search(
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
                label: skill.name.clone(),
                subtitle: Some(skillsmp_search_source_label(skill)),
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
                    "Downloading skill metadata from repository {}",
                    selected_skill.repository_url
                ),
                move || {
                    let client = SkillsMpClient::new(selected_config)?;
                    discover_repository_skills(&client, &selected_skill.repository_url, None)?
                        .into_iter()
                        .next()
                        .context("repository URL resolves to no skills")
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
                        subtitle: None,
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
            VIEW_FILES_CHOICE => {
                run_file_viewer(&mut session, &installable)?;
            }
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

#[allow(dead_code)]
pub(super) fn run_skillsmp_list(
    destination: &DestinationArgs,
    config: ClientConfig,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    if !is_interactive_terminal() {
        let directory = destination.resolve()?;
        let skills = discover_installed_skills_report(&directory)?
            .valid_skills
            .into_iter()
            .filter(|skill| skill.repository_url.is_some())
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
                    .filter(|skill| skill.repository_url.is_some())
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
                label: skill.name.clone(),
                subtitle: Some(skill_source_label(skill)),
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
                                subtitle: None,
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
