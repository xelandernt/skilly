use super::*;

pub(super) fn run_scan(
    destination: &DestinationArgs,
    dependency_selection: ScanDependencySelection,
    skilly_config: &SkillyConfig,
) -> Result<()> {
    let single_directory = destination.resolve()?;
    let environment = build_project_environment(&single_directory, &dependency_selection);
    let matches = scan_project_in(&environment)?;
    if matches.is_empty() {
        println!("No dependency skills found in project");
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
                let environment =
                    build_project_environment(&destination.path, &dependency_selection);
                scan_project_in(&environment)
            })
            .collect::<Result<Vec<_>>>()?;
        let all_empty = all_matches_by_tab.iter().all(Vec::is_empty);
        if all_empty {
            println!("No dependency skills found in project");
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
                label: item.available.name.clone(),
                subtitle: Some(skill_source_label(&item.available)),
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
                                    subtitle: None,
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
                        VIEW_FILES_CHOICE => {
                            run_file_viewer(&mut session, &selected.available)?;
                        }
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
