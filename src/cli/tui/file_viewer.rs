use super::*;

#[derive(Debug, Clone)]
pub(crate) struct FileViewEntry {
    pub(crate) name: String,
    pub(crate) relative_path: String,
    pub(crate) content: Vec<u8>,
    pub(crate) depth: u32,
    pub(crate) is_dir: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ScrollState {
    offset: usize,
    viewport_height: usize,
}

impl ScrollState {
    fn new() -> Self {
        Self {
            offset: 0,
            viewport_height: 1,
        }
    }

    fn page_down(&mut self, total_lines: usize) {
        let max = total_lines.saturating_sub(self.viewport_height);
        self.offset = (self.offset + self.viewport_height).min(max);
    }

    fn page_up(&mut self) {
        self.offset = self.offset.saturating_sub(self.viewport_height);
    }

    fn scroll_to_top(&mut self) {
        self.offset = 0;
    }

    fn scroll_to_bottom(&mut self, total_lines: usize) {
        self.offset = total_lines.saturating_sub(self.viewport_height);
    }

    fn reset(&mut self) {
        self.offset = 0;
    }
}

pub(crate) fn build_file_tree(skill: &SkillData) -> Vec<FileViewEntry> {
    let mut entries = Vec::new();

    let skill_markdown = if skill.raw.is_empty() {
        skill.render(None).into_bytes()
    } else {
        skill.raw.clone()
    };
    entries.push(FileViewEntry {
        name: "SKILL.md".to_string(),
        relative_path: "SKILL.md".to_string(),
        content: skill_markdown,
        depth: 0,
        is_dir: false,
    });

    if skill.resources.is_empty() {
        return entries;
    }

    let mut seen_dirs = HashSet::new();
    let mut paths: Vec<&str> = skill
        .resources
        .iter()
        .map(|r| r.relative_path.as_str())
        .collect();
    paths.sort();

    for path in &paths {
        let parts: Vec<&str> = path.split('/').collect();

        for i in 0..parts.len() - 1 {
            let dir_path = format!("{}/", parts[..=i].join("/"));
            if seen_dirs.insert(dir_path.clone()) {
                entries.push(FileViewEntry {
                    name: parts[i].to_string(),
                    relative_path: dir_path,
                    content: Vec::new(),
                    depth: i as u32,
                    is_dir: true,
                });
            }
        }

        if let Some(resource) = skill.resources.iter().find(|r| r.relative_path == *path) {
            let file_depth = parts.len() - 1;
            entries.push(FileViewEntry {
                name: parts[file_depth].to_string(),
                relative_path: path.to_string(),
                content: resource.raw.clone(),
                depth: file_depth as u32,
                is_dir: false,
            });
        }
    }

    entries
}

pub(crate) fn compute_visible(
    entries: &[FileViewEntry],
    collapsed: &HashSet<String>,
) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut dir_stack: Vec<(u32, &str)> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        while let Some(&(d, _)) = dir_stack.last() {
            if d >= entry.depth {
                dir_stack.pop();
            } else {
                break;
            }
        }

        let hidden = dir_stack.iter().any(|(_, path)| collapsed.contains(*path));

        if !hidden {
            visible.push(i);
        }

        if entry.is_dir {
            dir_stack.push((entry.depth, &entry.relative_path));
        }
    }

    visible
}

pub(crate) fn compute_filtered_visible(
    entries: &[FileViewEntry],
    collapsed: &HashSet<String>,
    filter_text: &str,
) -> Vec<usize> {
    if filter_text.is_empty() {
        return compute_visible(entries, collapsed);
    }

    let matching_files: Vec<&FileViewEntry> = entries
        .iter()
        .filter(|entry| file_filter_matches(filter_text, entry))
        .collect();

    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            (file_filter_matches(filter_text, entry)
                || (entry.is_dir
                    && matching_files
                        .iter()
                        .any(|file| file.relative_path.starts_with(&entry.relative_path))))
            .then_some(index)
        })
        .collect()
}

pub(crate) fn file_filter_matches(filter_text: &str, entry: &FileViewEntry) -> bool {
    !entry.is_dir
        && entry
            .name
            .to_lowercase()
            .contains(&filter_text.to_lowercase())
}

pub(crate) fn file_viewer_move_selection_up(visible: &[usize], current: usize) -> usize {
    if let Some(pos) = visible.iter().position(|&i| i == current) {
        if pos > 0 { visible[pos - 1] } else { current }
    } else {
        visible
            .iter()
            .rev()
            .find(|&&i| i < current)
            .copied()
            .unwrap_or(visible[0])
    }
}

pub(crate) fn file_viewer_move_selection_down(visible: &[usize], current: usize) -> usize {
    if let Some(pos) = visible.iter().position(|&i| i == current) {
        if pos + 1 < visible.len() {
            visible[pos + 1]
        } else {
            current
        }
    } else {
        visible
            .iter()
            .find(|&&i| i > current)
            .copied()
            .unwrap_or(*visible.last().unwrap_or(&0))
    }
}

fn entry_tree_label(entry: &FileViewEntry, collapsed: &HashSet<String>, filtering: bool) -> String {
    let indent = "  ".repeat(entry.depth as usize);
    if entry.is_dir {
        let prefix = if !filtering && collapsed.contains(entry.relative_path.as_str()) {
            "\u{25b6}"
        } else {
            "\u{25bc}"
        };
        format!("{indent}{prefix} {}", entry.name)
    } else {
        format!("{indent}  {}", entry.name)
    }
}

struct FileViewerRenderCtx<'a> {
    entries: &'a [FileViewEntry],
    visible: &'a [usize],
    selected_index: usize,
    collapsed: &'a HashSet<String>,
    scroll: &'a mut ScrollState,
    content_lines: &'a [String],
    title: &'a str,
    filter_active: bool,
    filter_text: &'a str,
    show_line_numbers: bool,
    line_number_digits: usize,
}
fn render_file_viewer(frame: &mut ratatui::Frame<'_>, ctx: &mut FileViewerRenderCtx<'_>) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[0]);

    // --- Left pane: file tree ---
    let mut list_state = ListState::default();
    if let Some(visible_pos) = ctx.visible.iter().position(|&i| i == ctx.selected_index) {
        list_state.select(Some(visible_pos));
    }

    let items: Vec<ListItem> = ctx
        .visible
        .iter()
        .map(|&i| {
            let label = entry_tree_label(
                &ctx.entries[i],
                ctx.collapsed,
                ctx.filter_active && !ctx.filter_text.is_empty(),
            );
            ListItem::new(Line::from(label))
        })
        .collect();

    let total_files = ctx.entries.iter().filter(|entry| !entry.is_dir).count();
    let matching_files = if ctx.filter_text.is_empty() {
        total_files
    } else {
        ctx.visible
            .iter()
            .filter(|&&index| !ctx.entries[index].is_dir)
            .count()
    };
    let list_title = if ctx.filter_active {
        format!(
            "Files: {} (filter: \"{}\", {matching_files}/{total_files})",
            ctx.title, ctx.filter_text
        )
    } else {
        format!("Files: {}", ctx.title)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{276f} ");
    frame.render_stateful_widget(list, panes[0], &mut list_state);

    // --- Right pane: content ---
    let selected_name = if ctx.visible.is_empty() {
        "No matching files".to_string()
    } else {
        ctx.entries[ctx.selected_index].name.clone()
    };
    let content_block = Block::default().borders(Borders::ALL).title(selected_name);

    let content_inner = content_block.inner(panes[1]);
    frame.render_widget(content_block, panes[1]);

    ctx.scroll.viewport_height = (content_inner.height as usize).max(1);

    if content_inner.width > 0 && content_inner.height > 0 {
        let visible_height = content_inner.height as usize;
        let has_overflow = ctx.content_lines.len() > visible_height;
        let max_offset = ctx.content_lines.len().saturating_sub(visible_height);
        if ctx.scroll.offset > max_offset {
            ctx.scroll.offset = max_offset;
        }

        let gutter_width = if ctx.show_line_numbers {
            ctx.line_number_digits + 2
        } else {
            0
        };

        let content_width = content_inner.width.saturating_sub(gutter_width as u16) as usize;

        let visible_lines: Vec<Line> = ctx
            .content_lines
            .iter()
            .enumerate()
            .skip(ctx.scroll.offset)
            .take(visible_height)
            .map(|(i, line)| {
                let line_num = i + 1;
                if ctx.show_line_numbers && content_inner.width > gutter_width as u16 {
                    let num_str = format!("{:>digits$}", line_num, digits = ctx.line_number_digits);
                    let prefix = format!("{num_str} \u{2502} ");
                    let body: String = line.chars().take(content_width).collect();
                    Line::from(vec![
                        Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                        Span::raw(body),
                    ])
                } else if content_inner.width <= 1 {
                    Line::from("")
                } else {
                    let cropped: String = line.chars().take(content_inner.width as usize).collect();
                    Line::from(cropped)
                }
            })
            .collect();

        frame.render_widget(Paragraph::new(visible_lines), content_inner);

        if has_overflow && ctx.scroll.offset > 0 {
            frame.render_widget(
                Paragraph::new("\u{22ee}").style(Style::default().fg(Color::DarkGray)),
                Rect {
                    x: content_inner.x,
                    y: content_inner.y,
                    width: content_inner.width,
                    height: 1,
                },
            );
        }
        if has_overflow && ctx.scroll.offset < max_offset {
            frame.render_widget(
                Paragraph::new("\u{22ee}").style(Style::default().fg(Color::DarkGray)),
                Rect {
                    x: content_inner.x,
                    y: content_inner.y + content_inner.height - 1,
                    width: content_inner.width,
                    height: 1,
                },
            );
        }
    }

    // --- Help bar ---
    let scroll_pct = if !ctx.content_lines.is_empty() && ctx.scroll.offset > 0 {
        format!(
            " {:.0}% ",
            (ctx.scroll.offset as f64 / ctx.content_lines.len() as f64 * 100.0).min(99.0)
        )
    } else if !ctx.content_lines.is_empty() {
        " 0% ".to_string()
    } else {
        String::new()
    };

    let help = if ctx.filter_active {
        format!(
            "Filter: \"{}\"  type to search  Backspace edit  Esc clear filter",
            ctx.filter_text
        )
    } else {
        format!(
            "{}\u{2191}\u{2193} navigate  / filter  Enter/Space toggle dir  L line numbers  PgUp/PgDn scroll  Home/End  Esc back{}",
            scroll_pct,
            if scroll_pct.is_empty() { "" } else { "  " }
        )
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::Gray)),
        layout[1],
    );
}

pub(crate) fn run_file_viewer(session: &mut TerminalSession, skill: &SkillData) -> Result<()> {
    let entries = build_file_tree(skill);
    if entries.is_empty() {
        return Ok(());
    }

    let mut collapsed = HashSet::new();
    let mut selected_index = 0usize;
    let mut scroll = ScrollState::new();
    let mut show_line_numbers = false;
    let mut filter_active = false;
    let mut filter_text = String::new();
    let title = skill.name.clone();

    let max_lines = entries
        .iter()
        .map(|e| String::from_utf8_lossy(&e.content).lines().count())
        .max()
        .unwrap_or(0);
    let line_number_digits = if max_lines == 0 {
        1usize
    } else {
        max_lines.ilog10() as usize + 1
    };

    loop {
        let visible = if filter_active {
            compute_filtered_visible(&entries, &collapsed, &filter_text)
        } else {
            compute_visible(&entries, &collapsed)
        };

        if !visible.is_empty() && !visible.contains(&selected_index) {
            selected_index = *visible.first().unwrap();
        }

        let content_lines = if visible.is_empty() {
            vec![format!("No files match \"{filter_text}\".")]
        } else {
            String::from_utf8_lossy(&entries[selected_index].content)
                .lines()
                .map(str::to_string)
                .collect()
        };

        session.terminal.draw(|frame| {
            let mut ctx = FileViewerRenderCtx {
                entries: &entries,
                visible: &visible,
                selected_index,
                collapsed: &collapsed,
                scroll: &mut scroll,
                content_lines: &content_lines,
                title: &title,
                filter_active,
                filter_text: &filter_text,
                show_line_numbers,
                line_number_digits,
            };
            render_file_viewer(frame, &mut ctx);
        })?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if !filter_active && key.code == KeyCode::Char('/') && key.modifiers.is_empty() {
            filter_active = true;
            filter_text.clear();
            scroll.reset();
            continue;
        }

        if filter_active {
            match key.code {
                KeyCode::Esc => {
                    filter_active = false;
                    filter_text.clear();
                    scroll.reset();
                    continue;
                }
                KeyCode::Backspace => {
                    filter_text.pop();
                    scroll.reset();
                    continue;
                }
                KeyCode::Char(value) if key.modifiers.is_empty() => {
                    filter_text.push(value);
                    scroll.reset();
                    continue;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Up => {
                selected_index = file_viewer_move_selection_up(&visible, selected_index);
                scroll.reset();
            }
            KeyCode::Down => {
                selected_index = file_viewer_move_selection_down(&visible, selected_index);
                scroll.reset();
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                show_line_numbers = !show_line_numbers;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let entry = &entries[selected_index];
                if !visible.is_empty() && entry.is_dir {
                    if collapsed.contains(&entry.relative_path) {
                        collapsed.remove(&entry.relative_path);
                    } else {
                        collapsed.insert(entry.relative_path.clone());
                    }
                }
            }
            KeyCode::PageUp => {
                scroll.page_up();
            }
            KeyCode::PageDown => {
                scroll.page_down(content_lines.len());
            }
            KeyCode::Home => {
                scroll.scroll_to_top();
            }
            KeyCode::End => {
                scroll.scroll_to_bottom(content_lines.len());
            }
            KeyCode::Esc => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
