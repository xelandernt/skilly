use super::*;

pub(super) fn run_list(
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

    let destinations = destination.resolve_configured_destinations(skilly_config)?;
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let client = Arc::new(SkillsMpClient::new(config)?);
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_indices = vec![0usize; destinations.len()];
    let mut checked_indices = vec![Vec::<usize>::new(); destinations.len()];
    let mut active_tab = 0usize;
    let mut update_checks = None;

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
        let checks = update_checks.get_or_insert_with(|| {
            UpdateChecks::start(
                build_update_check_requests(&destinations, &entries_by_tab),
                Arc::clone(&client),
            )
        });
        let mut restart_update_checks = false;

        let directory = &destinations[active_tab].path;
        let entries = &entries_by_tab[active_tab];
        let selectable_count = entries
            .iter()
            .filter(|entry| matches!(entry, ListedSkillEntry::Valid(_)))
            .count();
        let exit_index = entries.len();
        let mut items = entries
            .iter()
            .map(|entry| {
                let state = update_check_state_for(checks, directory, entry);
                MenuItemUi {
                    label: listed_skill_name(entry),
                    subtitle: listed_skill_source_label(entry),
                    preview_lines: listed_skill_preview_lines_with_update_state(
                        entry,
                        state.as_ref(),
                    ),
                    status: listed_skill_menu_status(entry, state.as_ref()),
                    selectable: matches!(entry, ListedSkillEntry::Valid(_)),
                    filter_text: Some(match entry {
                        ListedSkillEntry::Valid(skill) => skill.name.clone(),
                        ListedSkillEntry::Invalid(invalid) => invalid.directory_name.clone(),
                    }),
                }
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit list"));

        match multi_select_menu_with_tick(
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
            |menu, frame_index| {
                refresh_list_menu(
                    menu,
                    entries,
                    directory,
                    checks,
                    frame_index,
                    status_message.as_deref(),
                )
            },
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
                        let state = checks.state(&UpdateCheckKey::new(directory, &selected));
                        let update_available =
                            matches!(state, Some(UpdateCheckState::Updatable(_)));
                        let actions = installed_skill_actions(update_available, REMOVE_CHOICE);
                        match select_menu_with_tick(
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
                                        subtitle: None,
                                        preview_lines: listed_skill_preview_lines_with_update_state(
                                            &ListedSkillEntry::Valid(selected.clone()),
                                            state.as_ref(),
                                        ),
                                        status: MenuItemStatus::Default,
                                        selectable: true,
                                        filter_text: None,
                                    })
                                    .collect(),
                                default: action_menu_default(&actions),
                                preview_title: skill_directory_name(&selected),
                                status: update_check_status(
                                    checks.progress(),
                                    0,
                                    status_message.as_deref(),
                                ),
                                help_text: DEFAULT_HELP_TEXT.to_string(),
                                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                                tabs: destination_tabs(&destinations, &empty_flags),
                                active_tab,
                            },
                            |menu, frame_index| {
                                let progress = checks.progress();
                                let selected_state =
                                    checks.state(&UpdateCheckKey::new(directory, &selected));
                                menu.status = if progress.is_checking() {
                                    update_check_status(progress, frame_index, None)
                                } else {
                                    match selected_state {
                                        Some(UpdateCheckState::Updatable(_))
                                            if !update_available =>
                                        {
                                            Some(
                                                "Update available; return to the list to update"
                                                    .to_string(),
                                            )
                                        }
                                        Some(UpdateCheckState::Failed(error)) => Some(format!(
                                            "Update check failed: {}",
                                            sanitize_tui_line(&error)
                                        )),
                                        _ => update_check_status(
                                            progress,
                                            frame_index,
                                            status_message.as_deref(),
                                        ),
                                    }
                                };
                                progress.is_checking()
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
                                    let state =
                                        checks.state(&UpdateCheckKey::new(directory, &selected));
                                    let Some(UpdateCheckState::Updatable(available)) = state else {
                                        status_message = Some(
                                            "Update is no longer available; return to the list"
                                                .to_string(),
                                        );
                                        continue;
                                    };
                                    let updated = install_available_skill(
                                        directory,
                                        &available,
                                        Some(&skill_directory_name(&selected)),
                                        true,
                                    )?;
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        format_applied_update(&selected, &updated),
                                    );
                                    restart_update_checks = true;
                                }
                                REMOVE_CHOICE => {
                                    let removed =
                                        remove_skill(&skill_directory_name(&selected), directory)?;
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        format!("Removed {}", skill_directory_name(&removed)),
                                    );
                                    restart_update_checks = true;
                                }
                                _ => {}
                            },
                            // ----- installed skill action menu end -----
                        }
                    }
                    ListedSkillEntry::Invalid(selected) => {
                        let actions = invalid_installed_skill_actions(REMOVE_CHOICE);
                        match select_menu_with_tick(
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
                                        subtitle: None,
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
                            |menu, frame_index| {
                                let progress = checks.progress();
                                menu.status = update_check_status(
                                    progress,
                                    frame_index,
                                    status_message.as_deref(),
                                );
                                progress.is_checking()
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
                                    restart_update_checks = true;
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
                match select_menu_with_tick(
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
                                subtitle: None,
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
                    |menu, frame_index| {
                        let progress = checks.progress();
                        menu.status =
                            update_check_status(progress, frame_index, status_message.as_deref());
                        progress.is_checking()
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
                                restart_update_checks = true;
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
                                restart_update_checks = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if restart_update_checks {
            update_checks = None;
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}
