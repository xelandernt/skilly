use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextBuffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl TextBuffer {
    pub(crate) fn from_text(value: &str) -> Self {
        let mut lines = value.split('\n').map(str::to_string).collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let cursor_row = lines.len().saturating_sub(1);
        let cursor_col = crate::cli::tui::line_len(&lines[cursor_row]);
        Self {
            lines,
            cursor_row,
            cursor_col,
        }
    }

    pub(crate) fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn current_line_len(&self) -> usize {
        crate::cli::tui::line_len(&self.lines[self.cursor_row])
    }

    fn clamp_cursor(&mut self) {
        self.cursor_row = self.cursor_row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.current_line_len());
    }

    pub(crate) fn insert_char(&mut self, value: char) {
        let line = &mut self.lines[self.cursor_row];
        let byte_index = crate::cli::tui::char_to_byte_index(line, self.cursor_col);
        line.insert(byte_index, value);
        self.cursor_col += 1;
    }

    pub(crate) fn insert_newline(&mut self) {
        let current = self.lines[self.cursor_row].clone();
        let byte_index = crate::cli::tui::char_to_byte_index(&current, self.cursor_col);
        let (prefix, suffix) = current.split_at(byte_index);
        self.lines[self.cursor_row] = prefix.to_string();
        self.lines.insert(self.cursor_row + 1, suffix.to_string());
        self.cursor_row += 1;
        self.cursor_col = 0;
    }

    pub(super) fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let end = crate::cli::tui::char_to_byte_index(line, self.cursor_col);
            let start = crate::cli::tui::char_to_byte_index(line, self.cursor_col - 1);
            line.replace_range(start..end, "");
            self.cursor_col -= 1;
            return;
        }
        if self.cursor_row == 0 {
            return;
        }
        let removed = self.lines.remove(self.cursor_row);
        self.cursor_row -= 1;
        self.cursor_col = self.current_line_len();
        self.lines[self.cursor_row].push_str(&removed);
    }

    fn delete(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_row];
            let start = crate::cli::tui::char_to_byte_index(line, self.cursor_col);
            let end = crate::cli::tui::char_to_byte_index(line, self.cursor_col + 1);
            line.replace_range(start..end, "");
            return;
        }
        if self.cursor_row + 1 >= self.lines.len() {
            return;
        }
        let next = self.lines.remove(self.cursor_row + 1);
        self.lines[self.cursor_row].push_str(&next);
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            return;
        }
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
        }
    }

    fn move_right(&mut self) {
        if self.cursor_col < self.current_line_len() {
            self.cursor_col += 1;
            return;
        }
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub(crate) fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor();
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_cursor();
        }
    }

    fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub(crate) fn move_end(&mut self) {
        self.cursor_col = self.current_line_len();
    }

    pub(crate) fn render_lines(&self, area: Rect) -> (Vec<Line<'static>>, Option<(u16, u16)>) {
        if area.width == 0 || area.height == 0 {
            return (Vec::new(), None);
        }
        let visible_height = usize::from(area.height);
        let start_row = self
            .cursor_row
            .saturating_sub(visible_height.saturating_sub(1));
        let width = usize::from(area.width);
        let lines = self
            .lines
            .iter()
            .skip(start_row)
            .take(visible_height)
            .map(|line| crate::cli::tui::crop_text(line, width))
            .map(Line::from)
            .collect::<Vec<_>>();
        let cursor_row = self.cursor_row.saturating_sub(start_row);
        let cursor_col = self.cursor_col.min(width.saturating_sub(1));
        (
            lines,
            Some((area.x + cursor_col as u16, area.y + cursor_row as u16)),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CreateFormState {
    active_index: usize,
    name: TextBuffer,
    description: TextBuffer,
    instructions: TextBuffer,
    license: TextBuffer,
    compatibility: TextBuffer,
    metadata: TextBuffer,
    allowed_tools: TextBuffer,
    pub(super) with_scripts: bool,
    pub(super) with_references: bool,
    pub(super) with_assets: bool,
    overwrite: bool,
    pub(crate) status_message: Option<String>,
}

impl CreateFormState {
    pub(crate) fn new(options: CreateOptions) -> Self {
        Self {
            active_index: 0,
            name: TextBuffer::from_text(options.name.as_deref().unwrap_or("")),
            description: TextBuffer::from_text(options.description.as_deref().unwrap_or("")),
            instructions: TextBuffer::from_text(
                options
                    .instructions
                    .as_deref()
                    .unwrap_or(DEFAULT_CREATE_INSTRUCTIONS),
            ),
            license: TextBuffer::from_text(options.license.as_deref().unwrap_or("")),
            compatibility: TextBuffer::from_text(options.compatibility.as_deref().unwrap_or("")),
            metadata: TextBuffer::from_text(&options.metadata.join("\n")),
            allowed_tools: TextBuffer::from_text(options.allowed_tools.as_deref().unwrap_or("")),
            with_scripts: options.with_scripts,
            with_references: options.with_references,
            with_assets: options.with_assets,
            overwrite: options.overwrite,
            status_message: None,
        }
    }

    fn active_field(&self) -> CreateField {
        CREATE_FIELDS[self.active_index]
    }

    fn next_field(&mut self) {
        self.active_index = (self.active_index + 1) % CREATE_FIELDS.len();
    }

    fn previous_field(&mut self) {
        self.active_index = if self.active_index == 0 {
            CREATE_FIELDS.len().saturating_sub(1)
        } else {
            self.active_index - 1
        };
    }

    fn active_buffer(&self) -> Option<&TextBuffer> {
        match self.active_field() {
            CreateField::Name => Some(&self.name),
            CreateField::Description => Some(&self.description),
            CreateField::Instructions => Some(&self.instructions),
            CreateField::License => Some(&self.license),
            CreateField::Compatibility => Some(&self.compatibility),
            CreateField::Metadata => Some(&self.metadata),
            CreateField::AllowedTools => Some(&self.allowed_tools),
            _ => None,
        }
    }

    fn active_buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        match self.active_field() {
            CreateField::Name => Some(&mut self.name),
            CreateField::Description => Some(&mut self.description),
            CreateField::Instructions => Some(&mut self.instructions),
            CreateField::License => Some(&mut self.license),
            CreateField::Compatibility => Some(&mut self.compatibility),
            CreateField::Metadata => Some(&mut self.metadata),
            CreateField::AllowedTools => Some(&mut self.allowed_tools),
            _ => None,
        }
    }

    fn active_field_is_multiline(&self) -> bool {
        matches!(
            self.active_field(),
            CreateField::Instructions | CreateField::Metadata
        )
    }

    fn active_field_is_toggle(&self) -> bool {
        matches!(
            self.active_field(),
            CreateField::WithScripts
                | CreateField::WithReferences
                | CreateField::WithAssets
                | CreateField::Overwrite
        )
    }

    fn toggle_active(&mut self) {
        match self.active_field() {
            CreateField::WithScripts => self.with_scripts = !self.with_scripts,
            CreateField::WithReferences => self.with_references = !self.with_references,
            CreateField::WithAssets => self.with_assets = !self.with_assets,
            CreateField::Overwrite => self.overwrite = !self.overwrite,
            _ => {}
        }
    }

    pub(crate) fn apply(&mut self, action: CreateAction) {
        self.status_message = None;
        match action {
            CreateAction::NextField => self.next_field(),
            CreateAction::PreviousField => self.previous_field(),
            CreateAction::Insert(value) if self.active_field_is_toggle() && value == ' ' => {
                self.toggle_active();
            }
            CreateAction::NewLine if self.active_field_is_toggle() => self.toggle_active(),
            CreateAction::Insert(value) => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.insert_char(value);
                }
            }
            CreateAction::Backspace => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.backspace();
                }
            }
            CreateAction::Delete => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.delete();
                }
            }
            CreateAction::MoveLeft => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_left();
                }
            }
            CreateAction::MoveRight => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_right();
                }
            }
            CreateAction::MoveUp => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_up();
                }
            }
            CreateAction::MoveDown => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_down();
                }
            }
            CreateAction::MoveHome => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_home();
                }
            }
            CreateAction::MoveEnd => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.move_end();
                }
            }
            CreateAction::NewLine if self.active_field_is_multiline() => {
                if let Some(buffer) = self.active_buffer_mut() {
                    buffer.insert_newline();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn target_path(&self, directory: &Path) -> PathBuf {
        let name = self.name.text();
        let trimmed = name.trim();
        directory.join(if trimmed.is_empty() {
            "<name>"
        } else {
            trimmed
        })
    }

    pub(crate) fn metadata_lines(&self) -> Vec<String> {
        self.metadata
            .text()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    pub(crate) fn build_skill(&self, directory: &Path) -> Result<SkillData> {
        let skill = SkillData {
            name: self.name.text().trim().to_string(),
            description: self.description.text().trim().to_string(),
            path: None,
            body: self.instructions.text(),
            raw: Vec::new(),
            license: empty_to_none(self.license.text()),
            compatibility: empty_to_none(self.compatibility.text()),
            metadata: crate::cli::tui::parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem: None,
        };
        skill.validate()?;
        if !self.overwrite && self.target_path(directory).exists() {
            bail!("skill directory already exists; enable overwrite to replace it");
        }
        Ok(skill)
    }

    pub(crate) fn preview_lines(&self, directory: &Path) -> Vec<String> {
        let mut lines = vec![format!("Target: {}", self.target_path(directory).display())];
        let requested_directories = requested_directories(self);
        if requested_directories.is_empty() {
            lines.push("Directories: none".to_string());
        } else {
            lines.push(format!("Directories: {}", requested_directories.join(", ")));
        }
        lines.push(String::new());

        match self.preview_skill() {
            Ok(skill) => lines.extend(skill.render(None).lines().map(str::to_string)),
            Err(error) => lines.push(format!("Preview unavailable: {error}")),
        }
        lines
    }

    fn preview_skill(&self) -> Result<SkillData> {
        Ok(SkillData {
            name: placeholder_if_empty(self.name.text(), "skill-name"),
            description: placeholder_if_empty(self.description.text(), "Skill description."),
            path: None,
            body: self.instructions.text(),
            raw: Vec::new(),
            license: empty_to_none(self.license.text()),
            compatibility: empty_to_none(self.compatibility.text()),
            metadata: crate::cli::tui::parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem: None,
        })
    }

    pub(crate) fn field_summary(&self, field: CreateField) -> String {
        match field {
            CreateField::Name => summarize_text(&self.name.text()),
            CreateField::Description => summarize_text(&self.description.text()),
            CreateField::Instructions => summarize_multiline(&self.instructions.text()),
            CreateField::License => summarize_text(&self.license.text()),
            CreateField::Compatibility => summarize_text(&self.compatibility.text()),
            CreateField::Metadata => summarize_multiline(&self.metadata.text()),
            CreateField::AllowedTools => summarize_text(&self.allowed_tools.text()),
            CreateField::WithScripts => on_off(self.with_scripts).to_string(),
            CreateField::WithReferences => on_off(self.with_references).to_string(),
            CreateField::WithAssets => on_off(self.with_assets).to_string(),
            CreateField::Overwrite => on_off(self.overwrite).to_string(),
        }
    }

    pub(crate) fn active_title(&self) -> &'static str {
        match self.active_field() {
            CreateField::Name => "Name",
            CreateField::Description => "Description",
            CreateField::Instructions => "Instructions",
            CreateField::License => "License",
            CreateField::Compatibility => "Compatibility",
            CreateField::Metadata => "Metadata",
            CreateField::AllowedTools => "Allowed Tools",
            CreateField::WithScripts => "Create scripts/",
            CreateField::WithReferences => "Create references/",
            CreateField::WithAssets => "Create assets/",
            CreateField::Overwrite => "Overwrite existing skill",
        }
    }

    pub(crate) fn active_help(&self) -> &'static str {
        match self.active_field() {
            CreateField::Name => "Required. 1-64 lowercase letters, numbers, and single hyphens.",
            CreateField::Description => {
                "Required. Describe what the skill does and when to use it."
            }
            CreateField::Instructions => "Markdown body. Enter inserts line breaks here.",
            CreateField::License => "Optional bundled license reference or plain license name.",
            CreateField::Compatibility => "Optional compatibility note for environments or models.",
            CreateField::Metadata => "Optional frontmatter. One KEY=VALUE entry per line.",
            CreateField::AllowedTools => "Optional space-separated tools string.",
            CreateField::WithScripts => "Space toggles creation of an empty scripts/ directory.",
            CreateField::WithReferences => {
                "Space toggles creation of an empty references/ directory."
            }
            CreateField::WithAssets => "Space toggles creation of an empty assets/ directory.",
            CreateField::Overwrite => "Space toggles atomic replacement of an existing skill.",
        }
    }
}

pub(crate) fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub(crate) fn parse_metadata(values: &[String]) -> Result<BTreeMap<String, String>> {
    values
        .iter()
        .map(|value| {
            let (key, value) = value
                .split_once('=')
                .filter(|(key, _)| !key.is_empty())
                .ok_or_else(|| anyhow::anyhow!("metadata must use KEY=VALUE: {value}"))?;
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

pub(crate) fn run_create_tui(
    directory: &Path,
    options: CreateOptions,
) -> Result<Option<CreateOptions>> {
    let mut session = TerminalSession::new()?;
    let mut form = CreateFormState::new(options);
    loop {
        let mut cursor = None;
        session.terminal.draw(|frame| {
            cursor = render_create_form(frame, &form, directory);
        })?;

        if let Some((x, y)) = cursor {
            session.terminal.show_cursor()?;
            session.terminal.set_cursor_position((x, y))?;
        } else {
            session.terminal.hide_cursor()?;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        let Some(action) = create_action(key) else {
            continue;
        };
        match action {
            CreateAction::Cancel => return Ok(None),
            CreateAction::Save => match form.build_skill(directory) {
                Ok(_) => {
                    return Ok(Some(CreateOptions {
                        name: Some(form.name.text().trim().to_string()),
                        description: Some(form.description.text().trim().to_string()),
                        instructions: Some(form.instructions.text()),
                        license: empty_to_none(form.license.text()),
                        compatibility: empty_to_none(form.compatibility.text()),
                        metadata: form.metadata_lines(),
                        allowed_tools: empty_to_none(form.allowed_tools.text()),
                        with_scripts: form.with_scripts,
                        with_references: form.with_references,
                        with_assets: form.with_assets,
                        overwrite: form.overwrite,
                    }));
                }
                Err(error) => form.status_message = Some(error.to_string()),
            },
            action => form.apply(action),
        }
    }
}

pub(crate) fn render_create_form(
    frame: &mut ratatui::Frame<'_>,
    form: &CreateFormState,
    directory: &Path,
) -> Option<(u16, u16)> {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(frame.area());
    frame.render_widget(
        Paragraph::new(format!("Create skill in {}", directory.display()))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        layout[0],
    );

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(40)])
        .split(layout[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(panes[1]);

    let mut list_state = ListState::default();
    list_state.select(Some(form.active_index));
    let items = CREATE_FIELDS
        .iter()
        .map(|field| {
            ListItem::new(format!(
                "{}: {}",
                create_field_label(*field),
                form.field_summary(*field)
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Fields"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{276f} ");
    frame.render_stateful_widget(list, panes[0], &mut list_state);

    let editor_block = Block::default()
        .borders(Borders::ALL)
        .title(form.active_title());
    let editor_inner = editor_block.inner(right[0]);
    frame.render_widget(editor_block, right[0]);

    let mut cursor = None;
    if let Some(buffer) = form.active_buffer() {
        let (lines, position) = buffer.render_lines(editor_inner);
        frame.render_widget(Paragraph::new(lines), editor_inner);
        cursor = position;
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                "Current value: {}\n\n{}",
                form.field_summary(form.active_field()),
                form.active_help()
            ))
            .wrap(Wrap { trim: false }),
            editor_inner,
        );
    }

    let preview = Paragraph::new(
        form.preview_lines(directory)
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>(),
    )
    .block(Block::default().borders(Borders::ALL).title("Preview"))
    .wrap(Wrap { trim: false });
    frame.render_widget(preview, right[1]);

    let status = form
        .status_message
        .clone()
        .unwrap_or_else(|| form.active_help().to_string());
    frame.render_widget(
        Paragraph::new(CREATE_HELP_TEXT).style(Style::default().fg(Color::Gray)),
        layout[2],
    );
    let status_style = if form.status_message.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    };
    let status_area = Rect {
        x: layout[2].x,
        y: layout[2].y.saturating_add(1),
        width: layout[2].width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(status).style(status_style), status_area);
    cursor
}
