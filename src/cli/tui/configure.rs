use super::*;

/// Interactive configuration TUI that lets users select which directories skilly
/// should manage, plus stored repository credentials.
pub(crate) fn run_configure_tui(config: &crate::config::SkillyConfig) -> Result<Option<PathBuf>> {
    use crate::config::{KNOWN_GLOBAL_DIRS, KNOWN_LOCAL_DIRS};

    let mut global_enabled: Vec<bool> = KNOWN_GLOBAL_DIRS
        .iter()
        .map(|d| config.global.directories.contains(&d.to_string()))
        .collect();
    let mut local_enabled: Vec<bool> = KNOWN_LOCAL_DIRS
        .iter()
        .map(|d| config.local.directories.contains(&d.to_string()))
        .collect();

    let mut custom_global: Vec<String> = config
        .global
        .directories
        .iter()
        .filter(|d| !KNOWN_GLOBAL_DIRS.contains(&d.as_str()))
        .cloned()
        .collect();
    let mut custom_local: Vec<String> = config
        .local
        .directories
        .iter()
        .filter(|d| !KNOWN_LOCAL_DIRS.contains(&d.as_str()))
        .cloned()
        .collect();

    let mut default_directory = config.default_directory.clone();
    let mut provider_credentials = config.repositories.providers.clone();
    let mut provider_form: Option<ProviderCredentialForm> = None;

    let mut session = TerminalSession::new()?;
    let mut active_tab: usize = 0; // 0 = Global, 1 = Local, 2 = Providers
    let mut selected = 0usize;
    let mut status_message: Option<String> = None;
    let mut input_buffer: Option<TextBuffer> = None;

    let tabs = vec![
        MenuTabUi {
            label: "Global Directories".to_string(),
            color: Color::Cyan,
            dimmed: false,
        },
        MenuTabUi {
            label: "Local Directories".to_string(),
            color: Color::Yellow,
            dimmed: false,
        },
        MenuTabUi {
            label: "Provider Credentials".to_string(),
            color: Color::Magenta,
            dimmed: false,
        },
    ];

    loop {
        let items = if active_tab == 2 {
            build_provider_credential_items(&provider_credentials, provider_form.as_ref())
        } else {
            build_configure_items(
                active_tab,
                &global_enabled,
                &local_enabled,
                &custom_global,
                &custom_local,
                &default_directory,
            )
        };
        if selected >= items.len() {
            selected = items.len().saturating_sub(1);
        }

        // --- Render ---
        session.terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(if tabs.len() > 1 { 1 } else { 0 }),
                    Constraint::Min(3),
                    Constraint::Length(if status_message.is_some() { 1 } else { 0 }),
                    Constraint::Length(if input_buffer.is_some() { 3 } else { 1 }),
                ])
                .split(frame.area());
            let title_area = layout[0];
            let tabs_area = layout[1];
            let content_area = layout[2];
            let status_area = layout[3];
            let bottom_area = layout[4];

            frame.render_widget(
                Paragraph::new("Configure — skilly")
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                title_area,
            );
            if tabs.len() > 1 {
                render_menu_tabs(frame, tabs_area, &tabs, active_tab);
            }

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content_area);

            let display_items: Vec<ListItem<'_>> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let style = menu_item_style(item);
                    let line = if i == selected && input_buffer.is_none() {
                        Line::from(Span::styled(format!("\u{276f} {}", item.label), style))
                    } else {
                        Line::from(Span::styled(item.label.clone(), style))
                    };
                    ListItem::new(line)
                })
                .collect();

            let mut list_state = ListState::default();
            if input_buffer.is_none() {
                list_state.select(Some(selected));
            }

            let list_title = if active_tab == 2 {
                "Provider credentials".to_string()
            } else {
                format!(
                    "Directories  (default: {})",
                    configure_dir_label(&default_directory)
                )
            };
            let list = List::new(display_items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("");
            frame.render_stateful_widget(list, panes[0], &mut list_state);

            let preview_lines = if active_tab == 2
                && let Some(form) = provider_form.as_ref()
                && selected == 0
            {
                provider_selection_preview(form)
            } else {
                items
                    .get(selected)
                    .map(|item| {
                        if item.preview_lines.is_empty() {
                            vec![Line::from("No item selected.")]
                        } else {
                            item.preview_lines
                                .iter()
                                .map(|s| Line::from(s.clone()))
                                .collect()
                        }
                    })
                    .unwrap_or_else(|| vec![Line::from("No item selected.")])
            };

            let preview = Paragraph::new(preview_lines)
                .block(Block::default().borders(Borders::ALL).title("Preview"))
                .wrap(Wrap { trim: false });
            frame.render_widget(preview, panes[1]);

            if let Some(ref msg) = status_message {
                frame.render_widget(
                    Paragraph::new(msg.clone()).style(Style::default().fg(Color::Yellow)),
                    status_area,
                );
            }

            if let Some(ref buf) = input_buffer {
                let input_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Length(1)])
                    .split(bottom_area);
                frame.render_widget(
                    Paragraph::new("Enter path (Enter confirm, Esc cancel):")
                        .style(Style::default().fg(Color::Cyan)),
                    input_layout[0],
                );
                let input_text = buf.text();
                frame.render_widget(
                    Paragraph::new(format!("> {input_text}_")).style(Style::default()),
                    input_layout[1],
                );
            } else {
                let mut help = vec![Span::styled(
                    configure_help_text(active_tab, provider_form.is_some()),
                    Style::default().fg(Color::Gray),
                )];
                if default_directory.is_empty() {
                    help.push(Span::styled(
                        " | ⚠ No default directory set — select one with Enter before saving",
                        Style::default().fg(Color::Red),
                    ));
                }
                frame.render_widget(Paragraph::new(Line::from(help)), bottom_area);
            }
        })?;

        // --- Handle input ---
        if let Some(ref mut buf) = input_buffer {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc => {
                    input_buffer = None;
                    status_message = None;
                }
                KeyCode::Enter => {
                    let path = buf.text().trim().to_string();
                    if path.is_empty() {
                        status_message = Some("path must not be empty".to_string());
                        continue;
                    }
                    if active_tab == 0 {
                        if path.starts_with('/') || path.starts_with('~') {
                            if !custom_global.contains(&path)
                                && !KNOWN_GLOBAL_DIRS.contains(&path.as_str())
                            {
                                custom_global.push(path);
                            }
                            input_buffer = None;
                            status_message = Some("Global directory added".to_string());
                        } else {
                            status_message = Some(
                                "global directories must be absolute (start with / or ~)"
                                    .to_string(),
                            );
                        }
                    } else {
                        if path.starts_with('/') || path.starts_with('~') {
                            status_message = Some(
                                "local directories must be relative (no leading / or ~)"
                                    .to_string(),
                            );
                        } else {
                            if !custom_local.contains(&path)
                                && !KNOWN_LOCAL_DIRS.contains(&path.as_str())
                            {
                                custom_local.push(path);
                            }
                            input_buffer = None;
                            status_message = Some("Local directory added".to_string());
                        }
                    }
                    selected = items.len().saturating_sub(1);
                }
                KeyCode::Backspace => {
                    buf.backspace();
                }
                KeyCode::Char(c) => {
                    buf.insert_char(c);
                }
                _ => {}
            }
        } else {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Ctrl+S save
            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if active_tab == 2
                    && provider_form.is_some()
                    && let Some(form) = provider_form.clone()
                    && let Err(error) = save_provider_credential(
                        config,
                        &form,
                        &mut provider_credentials,
                        &mut provider_form,
                    )
                {
                    status_message = Some(error.to_string());
                    continue;
                }
                if default_directory.is_empty() {
                    status_message = Some(
                        "Set a default directory (Enter on an item) before saving.".to_string(),
                    );
                    continue;
                }
                // Check that the default is among the enabled directories
                let all_enabled: Vec<String> = {
                    let mut v: Vec<String> = Vec::new();
                    for (i, dir) in KNOWN_GLOBAL_DIRS.iter().enumerate() {
                        if global_enabled[i] {
                            v.push(dir.to_string());
                        }
                    }
                    v.extend(custom_global.clone());
                    for (i, dir) in KNOWN_LOCAL_DIRS.iter().enumerate() {
                        if local_enabled[i] {
                            v.push(dir.to_string());
                        }
                    }
                    v.extend(custom_local.clone());
                    v
                };
                if !all_enabled.contains(&default_directory) {
                    status_message = Some(format!(
                        "Default directory '{}' is not enabled. Enable it or pick a different default.",
                        default_directory
                    ));
                    continue;
                }

                let mut new_config = config.clone();
                new_config.default_directory = default_directory.clone();
                new_config.global.directories.clear();
                for (i, dir) in KNOWN_GLOBAL_DIRS.iter().enumerate() {
                    if global_enabled[i] {
                        new_config.global.directories.push(dir.to_string());
                    }
                }
                new_config.global.directories.extend(custom_global.clone());
                new_config.local.directories.clear();
                for (i, dir) in KNOWN_LOCAL_DIRS.iter().enumerate() {
                    if local_enabled[i] {
                        new_config.local.directories.push(dir.to_string());
                    }
                }
                new_config.local.directories.extend(custom_local.clone());
                new_config.repositories.providers = provider_credentials.clone();
                new_config.save()?;
                return Ok(Some(crate::config::SkillyConfig::config_path()?));
            }

            if active_tab == 2 && provider_form.is_some() {
                handle_provider_form_key(
                    key,
                    config,
                    &mut provider_form,
                    &mut provider_credentials,
                    &mut status_message,
                )?;
                selected = provider_form.as_ref().map_or(0, |form| form.focus);
                continue;
            }

            match key.code {
                KeyCode::Up => {
                    selected = crate::cli::args::previous_selectable_index(&items, selected);
                }
                KeyCode::Down => {
                    selected = crate::cli::args::next_selectable_index(&items, selected);
                }
                KeyCode::Tab => {
                    active_tab = (active_tab + 1) % tabs.len();
                    selected = 0;
                    status_message = None;
                }
                KeyCode::BackTab => {
                    active_tab = (active_tab + tabs.len() - 1) % tabs.len();
                    selected = 0;
                    status_message = None;
                }
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && active_tab == 2
                        && selected < provider_credentials.len() =>
                {
                    manage_provider_credential(
                        &mut provider_credentials,
                        &mut provider_form,
                        selected,
                        &mut status_message,
                    )?;
                }
                KeyCode::Char(' ') => {
                    if active_tab == 2 {
                        if selected == provider_credentials.len() {
                            manage_provider_credential(
                                &mut provider_credentials,
                                &mut provider_form,
                                selected,
                                &mut status_message,
                            )?;
                            if provider_form.is_some() {
                                selected = 0;
                            }
                        } else {
                            status_message = Some(
                                "Press Ctrl+D to remove the selected provider credential."
                                    .to_string(),
                            );
                        }
                        continue;
                    }
                    let add_custom_index = items.len().saturating_sub(1);
                    if selected == add_custom_index {
                        input_buffer = Some(TextBuffer::from_text(""));
                        status_message = None;
                    } else {
                        let toggled = toggle_configure_selection(
                            active_tab,
                            selected,
                            &mut global_enabled,
                            &mut local_enabled,
                            &mut custom_global,
                            &mut custom_local,
                            &default_directory,
                        );
                        if let Some(msg) = toggled {
                            status_message = Some(msg);
                        }
                    }
                }
                KeyCode::Enter => {
                    if active_tab == 2 {
                        if selected == provider_credentials.len() {
                            manage_provider_credential(
                                &mut provider_credentials,
                                &mut provider_form,
                                selected,
                                &mut status_message,
                            )?;
                            if provider_form.is_some() {
                                selected = 0;
                            }
                        } else {
                            status_message = Some(
                                "Press Ctrl+D to remove the selected provider credential."
                                    .to_string(),
                            );
                        }
                        continue;
                    }
                    let add_custom_index = items.len().saturating_sub(1);
                    if selected == add_custom_index {
                        input_buffer = Some(TextBuffer::from_text(""));
                        status_message = None;
                    } else {
                        let msg = set_configure_default(
                            active_tab,
                            selected,
                            &mut default_directory,
                            &global_enabled,
                            &local_enabled,
                            &custom_global,
                            &custom_local,
                        );
                        status_message = Some(msg);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Build the list of [`MenuItemUi`] for a configure tab.
fn build_configure_items(
    active_tab: usize,
    global_enabled: &[bool],
    local_enabled: &[bool],
    custom_global: &[String],
    custom_local: &[String],
    default_directory: &str,
) -> Vec<MenuItemUi> {
    use crate::config::{KNOWN_GLOBAL_DIRS, KNOWN_LOCAL_DIRS};

    let dirs: &[&str] = if active_tab == 0 {
        KNOWN_GLOBAL_DIRS
    } else {
        KNOWN_LOCAL_DIRS
    };
    let enabled: &[bool] = if active_tab == 0 {
        global_enabled
    } else {
        local_enabled
    };
    let customs: &[String] = if active_tab == 0 {
        custom_global
    } else {
        custom_local
    };

    let mut items = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        let label = configure_dir_label(dir);
        let preview = configure_dir_preview(dir, active_tab == 0);
        let checkbox = if *dir == default_directory {
            "[\u{2605}]"
        } else if enabled[i] {
            "[\u{2713}]"
        } else {
            "[ ]"
        };
        items.push(MenuItemUi {
            label: format!("{checkbox} {label}"),
            subtitle: None,
            preview_lines: vec![preview],
            status: if enabled[i] {
                MenuItemStatus::Installed
            } else {
                MenuItemStatus::Default
            },
            selectable: true,
            filter_text: None,
        });
    }
    if !customs.is_empty() {
        items.push(MenuItemUi {
            label: "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"
                .to_string(),
            subtitle: None,
            preview_lines: vec![],
            status: MenuItemStatus::Disabled,
            selectable: false,
            filter_text: None,
        });
        for dir in customs {
            let preview = configure_dir_preview(dir, active_tab == 0);
            let checkbox = if *dir == default_directory {
                "[\u{2605}]"
            } else {
                "[\u{2713}]"
            };
            items.push(MenuItemUi {
                label: format!("{checkbox} {dir}"),
                subtitle: None,
                preview_lines: vec![preview],
                status: MenuItemStatus::Installed,
                selectable: true,
                filter_text: None,
            });
        }
    }
    items.push(MenuItemUi {
        label: "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"
            .to_string(),
        subtitle: None,
        preview_lines: vec![],
        status: MenuItemStatus::Disabled,
        selectable: false,
        filter_text: None,
    });
    items.push(MenuItemUi {
        label: "Add custom...".to_string(),
        subtitle: None,
        preview_lines: vec![if active_tab == 0 {
            "Enter an absolute path (e.g. /opt/skills or ~/skills).".to_string()
        } else {
            "Enter a relative path (e.g. .agents/skills).".to_string()
        }],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    });
    items
}

#[derive(Debug, Clone)]
struct ProviderCredentialForm {
    provider: crate::core::RepositoryProvider,
    url: TextBuffer,
    token: TextBuffer,
    focus: usize,
}

impl Default for ProviderCredentialForm {
    fn default() -> Self {
        Self {
            provider: crate::core::RepositoryProvider::GitHub,
            url: TextBuffer::from_text("https://github.com"),
            token: TextBuffer::from_text(""),
            focus: 0,
        }
    }
}

fn build_provider_credential_items(
    credentials: &[crate::config::ProviderCredential],
    form: Option<&ProviderCredentialForm>,
) -> Vec<MenuItemUi> {
    if let Some(form) = form {
        return vec![
            provider_form_item(
                "Provider",
                form.provider.as_str().to_string(),
                "Use Up/Down to select GitHub, Bitbucket Cloud, or Bitbucket Data Center.",
            ),
            provider_form_item(
                "Base URL",
                form.url.text(),
                "Type the provider base URL. Data Center may include its reverse-proxy path.",
            ),
            provider_form_item(
                "Token",
                if form.token.text().is_empty() {
                    "<required>".to_string()
                } else {
                    "•".repeat(form.token.text().chars().count())
                },
                "Repository-read token. Its value is never shown after entry.",
            ),
            MenuItemUi {
                label: "Save provider".to_string(),
                subtitle: None,
                preview_lines: vec![
                    "Enter saves this credential and returns to the list.".to_string(),
                ],
                status: MenuItemStatus::Installed,
                selectable: true,
                filter_text: None,
            },
            MenuItemUi {
                label: "Cancel".to_string(),
                subtitle: None,
                preview_lines: vec![
                    "Discard this provider form and return to the list.".to_string(),
                ],
                status: MenuItemStatus::Default,
                selectable: true,
                filter_text: None,
            },
        ];
    }
    let mut items = credentials
        .iter()
        .map(|credential| MenuItemUi {
            label: format!("{} — {}", credential.provider.as_str(), credential.url),
            subtitle: None,
            preview_lines: vec![
                format!("Provider: {}", credential.provider.as_str()),
                format!("Base URL: {}", credential.url),
                "Token: stored (hidden)".to_string(),
                "Press Ctrl+D to remove this credential.".to_string(),
            ],
            status: MenuItemStatus::Installed,
            selectable: true,
            filter_text: None,
        })
        .collect::<Vec<_>>();
    items.push(MenuItemUi {
        label: "(Add provider)".to_string(),
        subtitle: None,
        preview_lines: vec![
            "Provide a provider type, base URL, and repository-read token.".to_string(),
            "The token is hidden in this UI and redacted from `configure --show`.".to_string(),
        ],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    });
    items
}

fn provider_form_item(label: &str, value: String, preview: &str) -> MenuItemUi {
    MenuItemUi {
        label: format!("{label:<12} │ {value}"),
        subtitle: None,
        preview_lines: vec![
            preview.to_string(),
            "Use Up/Down to select a field.".to_string(),
        ],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    }
}

fn provider_selection_preview(form: &ProviderCredentialForm) -> Vec<Line<'static>> {
    let choices = [
        (crate::core::RepositoryProvider::GitHub, "GitHub"),
        (
            crate::core::RepositoryProvider::BitbucketCloud,
            "Bitbucket Cloud",
        ),
        (
            crate::core::RepositoryProvider::BitbucketDataCenter,
            "Bitbucket Data Center",
        ),
    ];
    let mut choices_line = vec![Span::styled("←  ", Style::default().fg(Color::Gray))];
    for (index, (provider, label)) in choices.iter().enumerate() {
        if index > 0 {
            choices_line.push(Span::raw("   "));
        }
        let style = if *provider == form.provider {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        choices_line.push(Span::styled(*label, style));
    }
    choices_line.push(Span::styled("  →", Style::default().fg(Color::Gray)));
    vec![
        Line::from(Span::styled(
            "Provider type",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(choices_line),
        Line::from(""),
        Line::from("Use Left/Right to choose the provider. Use Down to edit the base URL."),
    ]
}

fn configure_help_text(active_tab: usize, provider_form_open: bool) -> &'static str {
    if provider_form_open {
        return "↑↓ field | ←→ provider | type URL/token | Enter save/cancel | ^S save configuration | Esc discard";
    }
    match active_tab {
        0 | 1 => {
            "↑↓ select | Tab switch tab | Space toggle/add | Enter set default | ^S save | Esc cancel"
        }
        2 => "↑↓ select | Tab switch tab | Space/Enter add | Ctrl+D remove | ^S save | Esc cancel",
        _ => "Esc cancel",
    }
}

fn manage_provider_credential(
    credentials: &mut Vec<crate::config::ProviderCredential>,
    form: &mut Option<ProviderCredentialForm>,
    selected: usize,
    status_message: &mut Option<String>,
) -> Result<()> {
    if selected < credentials.len() {
        let removed = credentials.remove(selected);
        *status_message = Some(format!(
            "Removed {} credential for {}",
            removed.provider.as_str(),
            removed.url
        ));
        return Ok(());
    }
    *form = Some(ProviderCredentialForm::default());
    *status_message =
        Some("Configure the provider, URL, and token; Enter or Ctrl+S saves.".to_string());
    Ok(())
}

fn save_provider_credential(
    config: &crate::config::SkillyConfig,
    form: &ProviderCredentialForm,
    credentials: &mut Vec<crate::config::ProviderCredential>,
    form_state: &mut Option<ProviderCredentialForm>,
) -> Result<()> {
    let credential = crate::config::ProviderCredential::new(
        form.provider,
        &form.url.text(),
        &form.token.text(),
    )?;
    let mut updated_credentials = credentials.clone();
    updated_credentials.retain(|existing| {
        existing.provider != credential.provider || existing.url != credential.url
    });
    updated_credentials.push(credential);

    let mut persisted_config = config.clone();
    persisted_config.repositories.providers = updated_credentials.clone();
    persisted_config.save()?;

    *credentials = updated_credentials;
    *form_state = None;
    Ok(())
}

fn handle_provider_form_key(
    key: KeyEvent,
    config: &crate::config::SkillyConfig,
    form_state: &mut Option<ProviderCredentialForm>,
    credentials: &mut Vec<crate::config::ProviderCredential>,
    status_message: &mut Option<String>,
) -> Result<()> {
    let Some(form) = form_state.as_mut() else {
        return Ok(());
    };
    match key.code {
        KeyCode::Esc => {
            *form_state = None;
            *status_message = Some("Provider creation cancelled".to_string());
        }
        KeyCode::Up => form.focus = (form.focus + 4) % 5,
        KeyCode::Down => form.focus = (form.focus + 1) % 5,
        KeyCode::Left if form.focus == 0 => {
            form.provider = previous_repository_provider(form.provider)
        }
        KeyCode::Right if form.focus == 0 => {
            form.provider = next_repository_provider(form.provider)
        }
        KeyCode::Enter if form.focus == 3 => {
            let snapshot = form.clone();
            match save_provider_credential(config, &snapshot, credentials, form_state) {
                Ok(()) => *status_message = Some("Provider credential saved.".to_string()),
                Err(error) => *status_message = Some(error.to_string()),
            }
        }
        KeyCode::Enter if form.focus == 4 => {
            *form_state = None;
            *status_message = Some("Provider creation cancelled".to_string());
        }
        KeyCode::Backspace if form.focus == 1 => form.url.backspace(),
        KeyCode::Backspace if form.focus == 2 => form.token.backspace(),
        KeyCode::Char(character) if form.focus == 1 => form.url.insert_char(character),
        KeyCode::Char(character) if form.focus == 2 => form.token.insert_char(character),
        _ => {}
    }
    Ok(())
}

fn previous_repository_provider(
    provider: crate::core::RepositoryProvider,
) -> crate::core::RepositoryProvider {
    match provider {
        crate::core::RepositoryProvider::GitHub => {
            crate::core::RepositoryProvider::BitbucketDataCenter
        }
        crate::core::RepositoryProvider::BitbucketCloud => crate::core::RepositoryProvider::GitHub,
        crate::core::RepositoryProvider::BitbucketDataCenter => {
            crate::core::RepositoryProvider::BitbucketCloud
        }
    }
}

fn next_repository_provider(
    provider: crate::core::RepositoryProvider,
) -> crate::core::RepositoryProvider {
    previous_repository_provider(previous_repository_provider(provider))
}

/// Toggle a checkbox in the configure TUI. For known entries this flips
/// enabled state, for custom entries this removes the entry. If the entry
/// being removed is the default directory, the operation is blocked.
fn toggle_configure_selection(
    active_tab: usize,
    selected: usize,
    global_enabled: &mut [bool],
    local_enabled: &mut [bool],
    custom_global: &mut Vec<String>,
    custom_local: &mut Vec<String>,
    default_directory: &str,
) -> Option<String> {
    use crate::config::{KNOWN_GLOBAL_DIRS, KNOWN_LOCAL_DIRS};

    let known: &[&str] = if active_tab == 0 {
        KNOWN_GLOBAL_DIRS
    } else {
        KNOWN_LOCAL_DIRS
    };
    let enabled: &mut [bool] = if active_tab == 0 {
        global_enabled
    } else {
        local_enabled
    };
    let customs: &mut Vec<String> = if active_tab == 0 {
        custom_global
    } else {
        custom_local
    };

    if selected < known.len() {
        if known[selected] == default_directory && enabled[selected] {
            return Some(
                "Cannot disable the default directory. Set a different default first.".to_string(),
            );
        }
        enabled[selected] = !enabled[selected];
        None
    } else {
        let custom_start = known.len() + 1;
        let custom_end = custom_start + customs.len();
        if selected >= custom_start && selected < custom_end {
            let idx = selected - custom_start;
            if customs[idx] == default_directory {
                return Some(
                    "Cannot remove the default directory. Set a different default first."
                        .to_string(),
                );
            }
            let removed = customs.remove(idx);
            Some(format!("Directory removed: {removed}"))
        } else {
            None
        }
    }
}

/// Set the default directory from the currently selected item.
fn set_configure_default(
    active_tab: usize,
    selected: usize,
    default_directory: &mut String,
    global_enabled: &[bool],
    local_enabled: &[bool],
    custom_global: &[String],
    custom_local: &[String],
) -> String {
    use crate::config::{KNOWN_GLOBAL_DIRS, KNOWN_LOCAL_DIRS};

    let known: &[&str] = if active_tab == 0 {
        KNOWN_GLOBAL_DIRS
    } else {
        KNOWN_LOCAL_DIRS
    };
    let enabled: &[bool] = if active_tab == 0 {
        global_enabled
    } else {
        local_enabled
    };
    let customs: &[String] = if active_tab == 0 {
        custom_global
    } else {
        custom_local
    };

    let new_default = if selected < known.len() {
        if enabled[selected] {
            known[selected].to_string()
        } else {
            return "Enable this directory first before setting it as default.".to_string();
        }
    } else {
        let custom_start = known.len() + 1;
        let custom_end = custom_start + customs.len();
        if selected >= custom_start && selected < custom_end {
            customs[selected - custom_start].clone()
        } else {
            return String::new();
        }
    };

    *default_directory = new_default.clone();
    format!("Default directory set to: {new_default}")
}

/// Human-readable label for a directory path (used in the configure TUI list).
fn configure_dir_label(dir: &str) -> String {
    use crate::cli::args::detect_flavor_from_path;
    use crate::core::SkillDirectoryFlavor;
    match detect_flavor_from_path(dir) {
        Some(SkillDirectoryFlavor::Agents) => format!("agents ({dir})"),
        Some(SkillDirectoryFlavor::Claude) => format!("claude ({dir})"),
        Some(SkillDirectoryFlavor::Codex) => format!("codex ({dir})"),
        Some(SkillDirectoryFlavor::Copilot) => format!("copilot ({dir})"),
        None => dir.to_string(),
    }
}

/// Preview line showing the resolved path for a directory entry.
fn configure_dir_preview(dir: &str, global: bool) -> String {
    use crate::core::absolute_path;
    if global {
        match absolute_path(Path::new(dir)) {
            Ok(path) => format!("Resolves to: {}", path.display()),
            Err(e) => format!("Error resolving: {e}"),
        }
    } else {
        format!("Path (relative to cwd): {dir}")
    }
}
