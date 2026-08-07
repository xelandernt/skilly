use super::*;

pub(crate) fn render_menu_tabs(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    tabs: &[MenuTabUi],
    active: usize,
) {
    let line = Line::from(
        tabs.iter()
            .enumerate()
            .flat_map(|(index, tab)| {
                let mut style = Style::default().fg(if tab.dimmed {
                    Color::DarkGray
                } else {
                    tab.color
                });
                if index == active {
                    style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                }
                let mut spans = vec![Span::styled(format!("[{}]", tab.label), style)];
                if index + 1 < tabs.len() {
                    spans.push(Span::raw(" "));
                }
                spans
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(Paragraph::new(line), area);
}

pub(crate) fn menu_item_style(item: &MenuItemUi) -> Style {
    match item.status {
        MenuItemStatus::Default => Style::default(),
        MenuItemStatus::Installable => Style::default().fg(Color::Blue),
        MenuItemStatus::Installed => Style::default().fg(Color::Green),
        MenuItemStatus::Checking => Style::default().fg(Color::Cyan),
        MenuItemStatus::UpToDate => Style::default().fg(Color::Green),
        MenuItemStatus::Updatable => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        MenuItemStatus::CheckFailed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        MenuItemStatus::Disabled => Style::default().fg(Color::DarkGray),
    }
}

pub(crate) fn menu_status_glyph(
    status: MenuItemStatus,
    frame_index: usize,
) -> Option<&'static str> {
    match status {
        MenuItemStatus::Installable => Some("+"),
        MenuItemStatus::Installed | MenuItemStatus::UpToDate => Some("✓"),
        MenuItemStatus::Checking => {
            Some(super::LOADING_FRAMES[frame_index % super::LOADING_FRAMES.len()])
        }
        MenuItemStatus::Updatable => Some("↑"),
        MenuItemStatus::CheckFailed => Some("!"),
        MenuItemStatus::Default | MenuItemStatus::Disabled => None,
    }
}

fn menu_status_glyph_style(status: MenuItemStatus) -> Style {
    match status {
        MenuItemStatus::Installable => Style::default().fg(Color::Blue),
        MenuItemStatus::Installed | MenuItemStatus::UpToDate => Style::default().fg(Color::Green),
        MenuItemStatus::Checking => Style::default().fg(Color::Cyan),
        MenuItemStatus::Updatable => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        MenuItemStatus::CheckFailed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        MenuItemStatus::Default | MenuItemStatus::Disabled => Style::default(),
    }
}

pub(crate) fn menu_item_lines(
    item: &MenuItemUi,
    query: &str,
    base_style: Style,
    max_width: usize,
    subtitle_indent: usize,
    frame_index: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![styled_menu_label(
        &fit_menu_label(&item.label, item.status, max_width),
        query,
        base_style,
        item.status,
    )];
    let Some(subtitle) = item.subtitle.as_deref() else {
        return lines;
    };
    let glyph = menu_status_glyph(item.status, frame_index);
    let glyph_width = glyph.map_or(0, |value| value.chars().count());
    let glyph_separator_width = glyph.map_or(0, |_| " · ".chars().count());
    let text_width = max_width.saturating_sub(glyph_width + glyph_separator_width);
    let subtitle = fit_menu_text(subtitle, text_width);
    let subtitle_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    let mut spans = vec![Span::raw(" ".repeat(subtitle_indent))];
    spans.push(Span::styled(subtitle, subtitle_style));
    if let Some(glyph) = glyph {
        spans.push(Span::styled(" · ", subtitle_style));
        spans.push(Span::styled(glyph, menu_status_glyph_style(item.status)));
    }
    lines.push(Line::from(spans));
    lines
}

fn read_menu_event(ticking: bool) -> Result<Option<Event>> {
    if ticking && !event::poll(Duration::from_millis(LOADING_POLL_INTERVAL_MS))? {
        return Ok(None);
    }
    Ok(Some(event::read()?))
}

pub(crate) fn select_menu(session: &mut TerminalSession, menu: MenuUi) -> Result<SelectMenuResult> {
    select_menu_with_tick(session, menu, |_, _| false)
}

pub(crate) fn select_menu_with_tick<F>(
    session: &mut TerminalSession,
    mut menu: MenuUi,
    mut on_tick: F,
) -> Result<SelectMenuResult>
where
    F: FnMut(&mut MenuUi, usize) -> bool,
{
    if menu.items.is_empty() {
        return Ok(SelectMenuResult::Cancel);
    }

    let mut state = ListState::default();
    let mut selected = menu.default.min(menu.items.len().saturating_sub(1));
    if !menu.items[selected].selectable {
        selected = crate::cli::args::first_selectable_index(&menu.items);
    }

    let has_filterable = menu.items.iter().any(|i| i.filter_text.is_some());
    let mut filter_text = String::new();
    let mut filter_active = false;

    let mut visible = if filter_active {
        build_visible_indices(&menu.items, &filter_text)
    } else {
        (0..menu.items.len()).collect()
    };
    let vis_pos = visible_position(&visible, selected).unwrap_or(0);
    state.select(Some(vis_pos));
    let mut frame_index = 0usize;

    loop {
        let ticking = on_tick(&mut menu, frame_index);
        session.terminal.draw(|frame| {
            let status_height = if menu.status.is_some() { 1 } else { 0 };
            let tab_height = if menu.tabs.len() > 1 { 1 } else { 0 };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(tab_height),
                    Constraint::Min(3),
                    Constraint::Length(status_height),
                    Constraint::Length(1),
                ])
                .split(frame.area());
            let title_area = layout[0];
            let tabs_area = layout[1];
            let content_area = layout[2];
            let status_area = layout[3];
            let help_area = layout[4];

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content_area);

            let render_indices: Vec<usize> = if filter_active {
                visible.clone()
            } else {
                (0..menu.items.len()).collect()
            };
            let items = render_indices
                .iter()
                .map(|&i| {
                    let item = &menu.items[i];
                    let style = menu_item_style(item);
                    let query = if filter_active { &filter_text } else { "" };
                    ListItem::new(menu_item_lines(
                        item,
                        query,
                        style,
                        panes[0].width.saturating_sub(4) as usize,
                        2,
                        frame_index,
                    ))
                })
                .collect::<Vec<_>>();
            let list_title = if filter_active {
                let shown = visible
                    .iter()
                    .filter(|&&i| menu.items[i].filter_text.is_some())
                    .count();
                let total = filterable_count(&menu.items);
                format!("Options (filter: \"{}\", {}/{})", filter_text, shown, total)
            } else {
                "Options".to_string()
            };
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("\u{276f} ");
            frame.render_widget(
                Paragraph::new(menu.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                title_area,
            );
            if tab_height == 1 {
                render_menu_tabs(frame, tabs_area, &menu.tabs, menu.active_tab);
            }
            frame.render_stateful_widget(list, panes[0], &mut state);

            let preview_lines = if filter_active && visible.is_empty() {
                Vec::new()
            } else {
                menu.items[selected].preview_lines.clone()
            };
            let preview_text = if preview_lines.is_empty() {
                menu.empty_preview.clone()
            } else {
                preview_lines.join("\n")
            };
            frame.render_widget(
                Paragraph::new(preview_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(menu.preview_title.clone()),
                    )
                    .wrap(Wrap { trim: false }),
                panes[1],
            );

            if let Some(status) = menu.status.as_ref() {
                frame.render_widget(
                    Paragraph::new(status.clone()).style(Style::default().fg(Color::Green)),
                    status_area,
                );
            }
            let help = if has_filterable {
                format!("{} | / filter", menu.help_text)
            } else {
                menu.help_text.clone()
            };
            frame.render_widget(
                Paragraph::new(help).style(Style::default().fg(Color::Gray)),
                help_area,
            );
        })?;

        let Some(terminal_event) = read_menu_event(ticking)? else {
            frame_index = frame_index.wrapping_add(1);
            continue;
        };

        if let Event::Key(key) = terminal_event {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(SelectMenuResult::Cancel);
            }

            if !filter_active
                && key.code == KeyCode::Char('/')
                && key.modifiers.is_empty()
                && has_filterable
            {
                filter_active = true;
                filter_text.clear();
                visible = build_visible_indices(&menu.items, &filter_text);
                let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                state.select(Some(vis_pos));
                continue;
            }

            if filter_active {
                match key.code {
                    KeyCode::Esc => {
                        filter_text.clear();
                        filter_active = false;
                        visible = (0..menu.items.len()).collect();
                        let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                        state.select(Some(vis_pos));
                        continue;
                    }
                    KeyCode::Backspace => {
                        filter_text.pop();
                        visible = build_visible_indices(&menu.items, &filter_text);
                        let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                        if (!menu.items[selected].selectable || !visible.contains(&selected))
                            && let Some(&first) = visible.first()
                        {
                            selected = first;
                        }
                        state.select(Some(vis_pos));
                        continue;
                    }
                    KeyCode::Char(c) if key.modifiers.is_empty() => {
                        filter_text.push(c);
                        visible = build_visible_indices(&menu.items, &filter_text);
                        let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                        if !visible.contains(&selected)
                            && let Some(&first) = visible.first()
                        {
                            selected = first;
                        }
                        state.select(Some(vis_pos));
                        continue;
                    }
                    _ => {}
                }
            }

            match menu_action(key) {
                Some(MenuAction::MoveUp) => {
                    if filter_active {
                        if visible.is_empty() {
                            // no matches — stay put
                        } else {
                            let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                            let new_pos = vis_pos.saturating_sub(1);
                            selected = visible[new_pos];
                            state.select(Some(new_pos));
                        }
                    } else {
                        selected =
                            crate::cli::args::previous_selectable_index(&menu.items, selected);
                        let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                        state.select(Some(vis_pos));
                    }
                }
                Some(MenuAction::MoveDown) => {
                    if filter_active {
                        if visible.is_empty() {
                            // no matches — stay put
                        } else {
                            let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                            let new_pos = (vis_pos + 1).min(visible.len().saturating_sub(1));
                            selected = visible[new_pos];
                            state.select(Some(new_pos));
                        }
                    } else {
                        selected = crate::cli::args::next_selectable_index(&menu.items, selected);
                        let vis_pos = visible_position(&visible, selected).unwrap_or(0);
                        state.select(Some(vis_pos));
                    }
                }
                Some(MenuAction::Select) => {
                    if (filter_active && visible.is_empty()) || !menu.items[selected].selectable {
                        // no matches — ignore, must Esc first
                    } else {
                        return Ok(SelectMenuResult::Selected(selected));
                    }
                }
                Some(MenuAction::Cancel) => return Ok(SelectMenuResult::Cancel),
                Some(MenuAction::NextTab) => return Ok(SelectMenuResult::NextTab),
                Some(MenuAction::PreviousTab) => return Ok(SelectMenuResult::PreviousTab),
                None => {}
            }
        }
    }
}

pub(crate) fn multi_select_menu(
    session: &mut TerminalSession,
    menu: MenuUi,
    selectable_count: usize,
    initially_checked: &[usize],
) -> Result<MultiSelectMenuResult> {
    multi_select_menu_with_tick(
        session,
        menu,
        selectable_count,
        initially_checked,
        |_, _| false,
    )
}

pub(crate) fn multi_select_menu_with_tick<F>(
    session: &mut TerminalSession,
    mut menu: MenuUi,
    selectable_count: usize,
    initially_checked: &[usize],
    mut on_tick: F,
) -> Result<MultiSelectMenuResult>
where
    F: FnMut(&mut MenuUi, usize) -> bool,
{
    if menu.items.is_empty() {
        return Ok(MultiSelectMenuResult::Cancel);
    }

    let mut state = ListState::default();
    let mut focused = menu.default.min(menu.items.len().saturating_sub(1));
    if !menu.items[focused].selectable {
        focused = crate::cli::args::first_selectable_index(&menu.items);
    }
    let mut checked: HashSet<usize> = initially_checked
        .iter()
        .copied()
        .filter(|index| *index < selectable_count)
        .collect();

    let has_filterable = menu.items.iter().any(|i| i.filter_text.is_some());
    let mut filter_text = String::new();
    let mut filter_active = false;

    let mut visible = if filter_active {
        build_visible_indices(&menu.items, &filter_text)
    } else {
        (0..menu.items.len()).collect()
    };
    let vis_pos = visible_position(&visible, focused).unwrap_or(0);
    state.select(Some(vis_pos));
    let mut frame_index = 0usize;

    loop {
        let ticking = on_tick(&mut menu, frame_index);
        session.terminal.draw(|frame| {
            let status_height = if menu.status.is_some() { 1 } else { 0 };
            let tab_height = if menu.tabs.len() > 1 { 1 } else { 0 };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(tab_height),
                    Constraint::Min(3),
                    Constraint::Length(status_height),
                    Constraint::Length(1),
                ])
                .split(frame.area());
            let title_area = layout[0];
            let tabs_area = layout[1];
            let content_area = layout[2];
            let status_area = layout[3];
            let help_area = layout[4];

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content_area);

            let render_indices: Vec<usize> = if filter_active {
                visible.clone()
            } else {
                (0..menu.items.len()).collect()
            };

            let selected_count = checked.len();
            let items = render_indices
                .iter()
                .map(|&i| {
                    let item = &menu.items[i];
                    let checkbox = if i < selectable_count {
                        let chk = if checked.contains(&i) {
                            "[\u{2713}] "
                        } else {
                            "[ ] "
                        };
                        Some(chk)
                    } else {
                        None
                    };
                    let label_width = panes[0]
                        .width
                        .saturating_sub(4 + checkbox.map_or(0, |value| value.len() as u16))
                        as usize;
                    let base_style = if i < selectable_count && checked.contains(&i) {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        menu_item_style(item)
                    };
                    let query = if filter_active { &filter_text } else { "" };
                    let mut spans = Vec::new();
                    if let Some(chk) = checkbox {
                        spans.push(Span::styled(chk.to_string(), base_style));
                    }
                    let mut lines = menu_item_lines(
                        item,
                        query,
                        base_style,
                        label_width,
                        checkbox.map_or(0, str::len),
                        frame_index,
                    );
                    lines[0].spans.splice(0..0, spans);
                    ListItem::new(lines)
                })
                .collect::<Vec<_>>();

            let list_title = if filter_active {
                let shown = visible
                    .iter()
                    .filter(|&&i| menu.items[i].filter_text.is_some())
                    .count();
                let total = filterable_count(&menu.items);
                format!(
                    "Options (filter: \"{}\", {}/{}, {} selected)",
                    filter_text, shown, total, selected_count
                )
            } else if selected_count > 0 {
                format!("Options ({} selected)", selected_count)
            } else {
                "Options".to_string()
            };
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("\u{276f} ");

            frame.render_widget(
                Paragraph::new(menu.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                title_area,
            );
            if tab_height == 1 {
                render_menu_tabs(frame, tabs_area, &menu.tabs, menu.active_tab);
            }
            frame.render_stateful_widget(list, panes[0], &mut state);

            let preview_lines = if filter_active && visible.is_empty() {
                Vec::new()
            } else {
                menu.items[focused].preview_lines.clone()
            };
            let preview_text = if preview_lines.is_empty() {
                menu.empty_preview.clone()
            } else {
                preview_lines.join("\n")
            };
            frame.render_widget(
                Paragraph::new(preview_text)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(menu.preview_title.clone()),
                    )
                    .wrap(Wrap { trim: false }),
                panes[1],
            );

            if let Some(status) = menu.status.as_ref() {
                frame.render_widget(
                    Paragraph::new(status.clone()).style(Style::default().fg(Color::Green)),
                    status_area,
                );
            }
            let help = if has_filterable {
                format!("{} | / filter", menu.help_text)
            } else {
                menu.help_text.clone()
            };
            frame.render_widget(
                Paragraph::new(help).style(Style::default().fg(Color::Gray)),
                help_area,
            );
        })?;

        let Some(terminal_event) = read_menu_event(ticking)? else {
            frame_index = frame_index.wrapping_add(1);
            continue;
        };

        if let Event::Key(key) = terminal_event {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(MultiSelectMenuResult::Cancel);
            }

            if !filter_active
                && key.code == KeyCode::Char('/')
                && key.modifiers.is_empty()
                && has_filterable
            {
                filter_active = true;
                filter_text.clear();
                visible = build_visible_indices(&menu.items, &filter_text);
                let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                state.select(Some(vis_pos));
                continue;
            }

            if filter_active {
                match key.code {
                    KeyCode::Esc => {
                        filter_text.clear();
                        filter_active = false;
                        visible = (0..menu.items.len()).collect();
                        let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                        state.select(Some(vis_pos));
                        continue;
                    }
                    KeyCode::Backspace => {
                        filter_text.pop();
                        visible = build_visible_indices(&menu.items, &filter_text);
                        adjust_focused_on_filter(&visible, &mut focused);
                        let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                        state.select(Some(vis_pos));
                        continue;
                    }
                    KeyCode::Char(c) if key.modifiers.is_empty() => {
                        filter_text.push(c);
                        visible = build_visible_indices(&menu.items, &filter_text);
                        adjust_focused_on_filter(&visible, &mut focused);
                        let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                        state.select(Some(vis_pos));
                        continue;
                    }
                    _ => {}
                }
            }

            match multi_select_action(key) {
                Some(MultiSelectMenuAction::MoveUp) => {
                    if filter_active {
                        if visible.is_empty() {
                            // no matches — stay put
                        } else {
                            let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                            let new_pos = vis_pos.saturating_sub(1);
                            focused = visible[new_pos];
                            state.select(Some(new_pos));
                        }
                    } else {
                        focused = crate::cli::args::previous_selectable_index(&menu.items, focused);
                        let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                        state.select(Some(vis_pos));
                    }
                }
                Some(MultiSelectMenuAction::MoveDown) => {
                    if filter_active {
                        if visible.is_empty() {
                            // no matches — stay put
                        } else {
                            let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                            let new_pos = (vis_pos + 1).min(visible.len().saturating_sub(1));
                            focused = visible[new_pos];
                            state.select(Some(new_pos));
                        }
                    } else {
                        focused = crate::cli::args::next_selectable_index(&menu.items, focused);
                        let vis_pos = visible_position(&visible, focused).unwrap_or(0);
                        state.select(Some(vis_pos));
                    }
                }
                Some(MultiSelectMenuAction::ToggleSelect) if focused < selectable_count => {
                    if filter_active && visible.is_empty() {
                        // no matches — ignore
                    } else if checked.contains(&focused) {
                        checked.remove(&focused);
                    } else {
                        checked.insert(focused);
                    }
                }
                Some(MultiSelectMenuAction::ToggleSelect) => {}
                Some(MultiSelectMenuAction::SelectAll) => {
                    if filter_active {
                        let all_visible_selected = visible
                            .iter()
                            .filter(|&&i| i < selectable_count)
                            .all(|&i| checked.contains(&i));
                        if all_visible_selected {
                            for &i in &visible {
                                checked.remove(&i);
                            }
                        } else {
                            for &i in &visible {
                                if i < selectable_count {
                                    checked.insert(i);
                                }
                            }
                        }
                    } else {
                        let all_selected = (0..selectable_count).all(|i| checked.contains(&i));
                        if all_selected {
                            checked.clear();
                        } else {
                            checked.extend(0..selectable_count);
                        }
                    }
                }
                Some(MultiSelectMenuAction::Confirm) => {
                    if filter_active && visible.is_empty() {
                        // no matches — ignore, must Esc first
                    } else if checked.is_empty() {
                        return Ok(MultiSelectMenuResult::Selection(MultiSelectResult::Single(
                            focused,
                        )));
                    } else {
                        let mut indices: Vec<usize> = checked.iter().copied().collect();
                        indices.sort_unstable();
                        return Ok(MultiSelectMenuResult::Selection(MultiSelectResult::Bulk(
                            indices,
                        )));
                    }
                }
                Some(MultiSelectMenuAction::Cancel) => return Ok(MultiSelectMenuResult::Cancel),
                Some(MultiSelectMenuAction::NextTab) => {
                    return Ok(MultiSelectMenuResult::NextTab);
                }
                Some(MultiSelectMenuAction::PreviousTab) => {
                    return Ok(MultiSelectMenuResult::PreviousTab);
                }
                None => {}
            }
        }
    }
}

pub(crate) fn filter_matches(filter_text: &str, item: &MenuItemUi) -> bool {
    match &item.filter_text {
        None => filter_text.is_empty(),
        Some(name) => name.to_lowercase().contains(&filter_text.to_lowercase()),
    }
}

pub(crate) fn filterable_count(items: &[MenuItemUi]) -> usize {
    items.iter().filter(|i| i.filter_text.is_some()).count()
}

pub(crate) fn build_visible_indices(items: &[MenuItemUi], filter_text: &str) -> Vec<usize> {
    if filter_text.is_empty() {
        return (0..items.len()).collect();
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| filter_matches(filter_text, item))
        .map(|(i, _)| i)
        .collect()
}

pub(crate) fn visible_position(visible: &[usize], original: usize) -> Option<usize> {
    visible.iter().position(|&i| i == original)
}

pub(crate) fn adjust_focused_on_filter(visible: &[usize], focused: &mut usize) {
    if !visible.contains(focused)
        && let Some(&first) = visible.first()
    {
        *focused = first;
    }
}

pub(crate) fn fit_menu_label(label: &str, status: MenuItemStatus, max_width: usize) -> String {
    if label.chars().count() <= max_width {
        return label.to_string();
    }

    let Some((base, suffix)) = menu_status_suffix(label, status) else {
        return fit_menu_text(label, max_width);
    };
    let suffix_width = suffix.chars().count();
    let reserved_suffix_width = if status == MenuItemStatus::Checking {
        " ...".chars().count()
    } else {
        suffix_width
    };
    if max_width <= reserved_suffix_width {
        return suffix
            .trim_start()
            .chars()
            .take(max_width)
            .collect::<String>();
    }

    let base_width = max_width - reserved_suffix_width;
    let mut fitted = base
        .chars()
        .take(base_width.saturating_sub(1))
        .collect::<String>();
    fitted.push('…');
    fitted.push_str(suffix);
    fitted
}

fn fit_menu_text(value: &str, max_width: usize) -> String {
    if value.chars().count() <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut fitted = value.chars().take(max_width - 1).collect::<String>();
    fitted.push('…');
    fitted
}

fn menu_status_suffix(label: &str, status: MenuItemStatus) -> Option<(&str, &str)> {
    let suffix = match status {
        MenuItemStatus::Checking => {
            let suffix = label.rsplit_once(' ')?.1;
            if !matches!(suffix, "." | ".." | "...") {
                return None;
            }
            suffix
        }
        MenuItemStatus::UpToDate => "(up to date)",
        MenuItemStatus::Updatable => "(updatable)",
        MenuItemStatus::CheckFailed => "(check failed)",
        _ => return None,
    };
    let base = label.strip_suffix(suffix)?.strip_suffix(' ')?;
    Some((base, &label[base.len()..]))
}

pub(crate) fn styled_menu_label(
    label: &str,
    query: &str,
    base_style: Style,
    status: MenuItemStatus,
) -> Line<'static> {
    let Some((base, suffix)) = menu_status_suffix(label, status) else {
        return highlighted_line(label, query, base_style);
    };
    let mut spans = highlighted_line(base, query, base_style).spans;
    let suffix_style = if status == MenuItemStatus::Checking {
        base_style
    } else {
        base_style.add_modifier(Modifier::ITALIC)
    };
    spans.push(Span::styled(suffix.to_string(), suffix_style));
    Line::from(spans)
}

pub(crate) fn highlighted_line(text: &str, query: &str, base_style: Style) -> Line<'static> {
    if query.is_empty() {
        return Line::from(Span::styled(text.to_string(), base_style));
    }

    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();

    if let Some(start) = lower_text.find(&lower_query) {
        let end = start + query.len();
        let match_style = base_style.add_modifier(Modifier::UNDERLINED);

        let mut spans = Vec::new();
        if start > 0 {
            spans.push(Span::styled(text[..start].to_string(), base_style));
        }
        spans.push(Span::styled(text[start..end].to_string(), match_style));
        if end < text.len() {
            spans.push(Span::styled(text[end..].to_string(), base_style));
        }
        Line::from(spans)
    } else {
        Line::from(Span::styled(text.to_string(), base_style))
    }
}

pub(crate) fn menu_action(key: KeyEvent) -> Option<MenuAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Up => Some(MenuAction::MoveUp),
        KeyCode::Down => Some(MenuAction::MoveDown),
        KeyCode::Enter => Some(MenuAction::Select),
        KeyCode::Tab => Some(MenuAction::NextTab),
        KeyCode::BackTab => Some(MenuAction::PreviousTab),
        KeyCode::Esc => Some(MenuAction::Cancel),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(MenuAction::Cancel)
        }
        _ => None,
    }
}

pub(crate) fn multi_select_action(key: KeyEvent) -> Option<MultiSelectMenuAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Up => Some(MultiSelectMenuAction::MoveUp),
        KeyCode::Down => Some(MultiSelectMenuAction::MoveDown),
        KeyCode::Tab => Some(MultiSelectMenuAction::NextTab),
        KeyCode::BackTab => Some(MultiSelectMenuAction::PreviousTab),
        KeyCode::Char(' ') => Some(MultiSelectMenuAction::ToggleSelect),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(MultiSelectMenuAction::SelectAll),
        KeyCode::Enter => Some(MultiSelectMenuAction::Confirm),
        KeyCode::Esc => Some(MultiSelectMenuAction::Cancel),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(MultiSelectMenuAction::Cancel)
        }
        _ => None,
    }
}
