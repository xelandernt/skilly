use super::args::{CREATE_FIELDS, CreateAction, CreateField, CreateOptions};
use super::{
    CREATE_HELP_TEXT, DEFAULT_CREATE_INSTRUCTIONS, LOADING_FRAMES, LOADING_POLL_INTERVAL_MS,
};
use crate::core::{SKILLY_UNKNOWN_SOURCE, SkillData};
use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::collections::{BTreeMap, HashSet};
use std::io::{self, IsTerminal, Stdout};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct MenuItemUi {
    pub(crate) label: String,
    pub(crate) preview_lines: Vec<String>,
    pub(crate) status: MenuItemStatus,
    pub(crate) selectable: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MenuUi {
    pub(crate) title: String,
    pub(crate) items: Vec<MenuItemUi>,
    pub(crate) default: usize,
    pub(crate) preview_title: String,
    pub(crate) status: Option<String>,
    pub(crate) help_text: String,
    pub(crate) empty_preview: String,
    pub(crate) tabs: Vec<MenuTabUi>,
    pub(crate) active_tab: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MenuTabUi {
    pub(crate) label: String,
    pub(crate) color: Color,
    pub(crate) dimmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuItemStatus {
    Default,
    Installed,
    Updatable,
    Disabled,
}

#[derive(Debug, Clone)]
pub(crate) struct DownloadableSkillMatch {
    pub(crate) available: SkillData,
    pub(crate) installed: Option<SkillData>,
}

#[derive(Debug, Clone)]
pub(crate) struct InvalidInstalledSkill {
    pub(crate) directory_name: String,
    pub(crate) path: PathBuf,
    pub(crate) error: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InstalledSkillDiscoveryReport {
    pub(crate) valid_skills: Vec<SkillData>,
    pub(crate) invalid_skills: Vec<InvalidInstalledSkill>,
}

#[derive(Debug, Clone)]
pub(crate) enum ListedSkillEntry {
    Valid(Box<SkillData>),
    Invalid(InvalidInstalledSkill),
}

impl DownloadableSkillMatch {
    pub(crate) fn status(&self) -> &'static str {
        match self.installed.as_ref() {
            None => "installable",
            Some(installed) if crate::core::github_versions_match(installed, &self.available) => {
                "installed"
            }
            Some(_) => "updatable",
        }
    }
}

pub(crate) struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    pub(crate) fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    MoveUp,
    MoveDown,
    Select,
    Cancel,
    NextTab,
    PreviousTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiSelectMenuAction {
    MoveUp,
    MoveDown,
    ToggleSelect,
    SelectAll,
    Confirm,
    Cancel,
    NextTab,
    PreviousTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MultiSelectResult {
    Single(usize),
    Bulk(Vec<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectMenuResult {
    Selected(usize),
    Cancel,
    NextTab,
    PreviousTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MultiSelectMenuResult {
    Selection(MultiSelectResult),
    Cancel,
    NextTab,
    PreviousTab,
}

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

    fn backspace(&mut self) {
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
    with_scripts: bool,
    with_references: bool,
    with_assets: bool,
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
            content: self.instructions.text(),
            license: empty_to_none(self.license.text()),
            compatibility: empty_to_none(self.compatibility.text()),
            metadata: crate::cli::tui::parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
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
            content: self.instructions.text(),
            license: empty_to_none(self.license.text()),
            compatibility: empty_to_none(self.compatibility.text()),
            metadata: crate::cli::tui::parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            github_url: None,
            github_commit_sha: None,
            skillsmp_id: None,
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
        MenuItemStatus::Installed => Style::default().fg(Color::Green),
        MenuItemStatus::Updatable => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        MenuItemStatus::Disabled => Style::default().fg(Color::DarkGray),
    }
}

pub(crate) fn select_menu(session: &mut TerminalSession, menu: MenuUi) -> Result<SelectMenuResult> {
    if menu.items.is_empty() {
        return Ok(SelectMenuResult::Cancel);
    }

    let mut state = ListState::default();
    let mut selected = menu.default.min(menu.items.len().saturating_sub(1));
    if !menu.items[selected].selectable {
        selected = crate::cli::args::first_selectable_index(&menu.items);
    }
    state.select(Some(selected));

    loop {
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

            let items = menu
                .items
                .iter()
                .map(|item| {
                    ListItem::new(Line::from(item.label.clone())).style(menu_item_style(item))
                })
                .collect::<Vec<_>>();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Options"))
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

            let preview_lines = menu.items[selected].preview_lines.clone();
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
            frame.render_widget(
                Paragraph::new(menu.help_text.clone()).style(Style::default().fg(Color::Gray)),
                help_area,
            );
        })?;

        if let Event::Key(key) = event::read()? {
            match menu_action(key) {
                Some(MenuAction::MoveUp) => {
                    selected = crate::cli::args::previous_selectable_index(&menu.items, selected);
                    state.select(Some(selected));
                }
                Some(MenuAction::MoveDown) => {
                    selected = crate::cli::args::next_selectable_index(&menu.items, selected);
                    state.select(Some(selected));
                }
                Some(MenuAction::Select) => return Ok(SelectMenuResult::Selected(selected)),
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
    if menu.items.is_empty() {
        return Ok(MultiSelectMenuResult::Cancel);
    }

    let mut state = ListState::default();
    let mut focused = menu.default.min(menu.items.len().saturating_sub(1));
    if !menu.items[focused].selectable {
        focused = crate::cli::args::first_selectable_index(&menu.items);
    }
    state.select(Some(focused));
    let mut checked: HashSet<usize> = initially_checked
        .iter()
        .copied()
        .filter(|index| *index < selectable_count)
        .collect();

    loop {
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

            let items = menu
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let label = if i < selectable_count {
                        let checkbox = if checked.contains(&i) {
                            "[\u{2713}] "
                        } else {
                            "[ ] "
                        };
                        format!("{checkbox}{}", item.label)
                    } else {
                        item.label.clone()
                    };
                    let style = if i < selectable_count && checked.contains(&i) {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        menu_item_style(item)
                    };
                    ListItem::new(Line::from(label)).style(style)
                })
                .collect::<Vec<_>>();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Options"))
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

            let preview_lines = menu.items[focused].preview_lines.clone();
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
            frame.render_widget(
                Paragraph::new(menu.help_text.clone()).style(Style::default().fg(Color::Gray)),
                help_area,
            );
        })?;

        if let Event::Key(key) = event::read()? {
            match multi_select_action(key) {
                Some(MultiSelectMenuAction::MoveUp) => {
                    focused = crate::cli::args::previous_selectable_index(&menu.items, focused);
                    state.select(Some(focused));
                }
                Some(MultiSelectMenuAction::MoveDown) => {
                    focused = crate::cli::args::next_selectable_index(&menu.items, focused);
                    state.select(Some(focused));
                }
                Some(MultiSelectMenuAction::ToggleSelect) if focused < selectable_count => {
                    if checked.contains(&focused) {
                        checked.remove(&focused);
                    } else {
                        checked.insert(focused);
                    }
                }
                Some(MultiSelectMenuAction::ToggleSelect) => {}
                Some(MultiSelectMenuAction::SelectAll) => {
                    let all_selected = (0..selectable_count).all(|i| checked.contains(&i));
                    if all_selected {
                        checked.clear();
                    } else {
                        checked.extend(0..selectable_count);
                    }
                }
                Some(MultiSelectMenuAction::Confirm) => {
                    if checked.is_empty() {
                        return Ok(MultiSelectMenuResult::Selection(MultiSelectResult::Single(
                            focused,
                        )));
                    }
                    let mut indices: Vec<usize> = checked.iter().copied().collect();
                    indices.sort_unstable();
                    return Ok(MultiSelectMenuResult::Selection(MultiSelectResult::Bulk(
                        indices,
                    )));
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

pub(crate) fn create_action(key: KeyEvent) -> Option<CreateAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => Some(CreateAction::Save),
            KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Char('c') => {
                Some(CreateAction::Cancel)
            }
            _ => None,
        };
    }

    match key.code {
        KeyCode::Tab => Some(CreateAction::NextField),
        KeyCode::BackTab => Some(CreateAction::PreviousField),
        KeyCode::F(2) => Some(CreateAction::Save),
        KeyCode::F(10) => Some(CreateAction::Cancel),
        KeyCode::Backspace => Some(CreateAction::Backspace),
        KeyCode::Delete => Some(CreateAction::Delete),
        KeyCode::Left => Some(CreateAction::MoveLeft),
        KeyCode::Right => Some(CreateAction::MoveRight),
        KeyCode::Up => Some(CreateAction::MoveUp),
        KeyCode::Down => Some(CreateAction::MoveDown),
        KeyCode::Home => Some(CreateAction::MoveHome),
        KeyCode::End => Some(CreateAction::MoveEnd),
        KeyCode::Enter => Some(CreateAction::NewLine),
        KeyCode::Esc => Some(CreateAction::Cancel),
        KeyCode::Char(value)
            if !key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            Some(CreateAction::Insert(value))
        }
        _ => None,
    }
}

pub(crate) fn create_field_label(field: CreateField) -> &'static str {
    match field {
        CreateField::Name => "Name",
        CreateField::Description => "Description",
        CreateField::Instructions => "Instructions",
        CreateField::License => "License",
        CreateField::Compatibility => "Compatibility",
        CreateField::Metadata => "Metadata",
        CreateField::AllowedTools => "Allowed Tools",
        CreateField::WithScripts => "scripts/",
        CreateField::WithReferences => "references/",
        CreateField::WithAssets => "assets/",
        CreateField::Overwrite => "Overwrite",
    }
}

pub(crate) fn requested_directories(form: &CreateFormState) -> Vec<&'static str> {
    let mut directories = Vec::new();
    if form.with_scripts {
        directories.push("scripts/");
    }
    if form.with_references {
        directories.push("references/");
    }
    if form.with_assets {
        directories.push("assets/");
    }
    directories
}

pub(crate) fn char_to_byte_index(value: &str, offset: usize) -> usize {
    value
        .char_indices()
        .nth(offset)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

pub(crate) fn line_len(value: &str) -> usize {
    value.chars().count()
}

pub(crate) fn crop_text(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

pub(crate) fn summarize_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    truncate_summary(trimmed)
}

pub(crate) fn summarize_multiline(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    if trimmed.contains('\n') {
        return format!("{} lines", trimmed.lines().count());
    }
    truncate_summary(trimmed)
}

pub(crate) fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

pub(crate) fn truncate_summary(value: &str) -> String {
    let max_len = 24usize;
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    format!(
        "{}\u{2026}",
        value
            .chars()
            .take(max_len.saturating_sub(1))
            .collect::<String>()
    )
}

pub(crate) fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn placeholder_if_empty(value: String, placeholder: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        placeholder.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn show_loading_message<T, F>(title: &str, message: &str, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    if !is_interactive_terminal() {
        return work();
    }
    let mut session = TerminalSession::new()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });

    let mut frame_index = 0usize;
    loop {
        session.terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                ])
                .split(frame.area());
            let spinner = LOADING_FRAMES[frame_index % LOADING_FRAMES.len()];
            frame.render_widget(
                Paragraph::new(title.to_string())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                layout[0],
            );
            frame.render_widget(
                Paragraph::new(format!("{spinner} {message}"))
                    .block(Block::default().borders(Borders::ALL).title("Loading"))
                    .wrap(Wrap { trim: false }),
                layout[1],
            );
            frame.render_widget(
                Paragraph::new("Please wait...").style(
                    Style::default()
                        .fg(Color::Gray)
                        .add_modifier(Modifier::ITALIC),
                ),
                layout[2],
            );
        })?;

        match receiver.recv_timeout(Duration::from_millis(LOADING_POLL_INTERVAL_MS)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                frame_index = frame_index.wrapping_add(1);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("loading task ended unexpectedly");
            }
        }
    }
}

/// Interactive configuration TUI that lets users select which directories skilly
/// should manage. Two tabs: Global Directories and Local Directories.
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

    let mut session = TerminalSession::new()?;
    let mut active_tab: usize = 0; // 0 = Global, 1 = Local
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
    ];

    let help_text = "\u{2191}\u{2193} move | Tab switch | Space toggle | Enter set default | ^S save | Esc cancel";

    loop {
        let items = build_configure_items(
            active_tab,
            &global_enabled,
            &local_enabled,
            &custom_global,
            &custom_local,
            &default_directory,
        );
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
                Paragraph::new("Configure — skilly directories")
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

            let list = List::new(display_items)
                .block(Block::default().borders(Borders::ALL).title(format!(
                    "Directories  (default: {})",
                    configure_dir_label(&default_directory)
                )))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("");
            frame.render_stateful_widget(list, panes[0], &mut list_state);

            let preview_lines: Vec<Line<'_>> = items
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
                .unwrap_or_else(|| vec![Line::from("No item selected.")]);

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
                let mut help = vec![Span::styled(help_text, Style::default().fg(Color::Gray))];
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
                new_config.save()?;
                return Ok(Some(crate::config::SkillyConfig::config_path()?));
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
                KeyCode::Char(' ') => {
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
            preview_lines: vec![preview],
            status: if enabled[i] {
                MenuItemStatus::Installed
            } else {
                MenuItemStatus::Default
            },
            selectable: true,
        });
    }
    if !customs.is_empty() {
        items.push(MenuItemUi {
            label: "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"
                .to_string(),
            preview_lines: vec![],
            status: MenuItemStatus::Disabled,
            selectable: false,
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
                preview_lines: vec![preview],
                status: MenuItemStatus::Installed,
                selectable: true,
            });
        }
    }
    items.push(MenuItemUi {
        label: "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"
            .to_string(),
        preview_lines: vec![],
        status: MenuItemStatus::Disabled,
        selectable: false,
    });
    items.push(MenuItemUi {
        label: "Add custom...".to_string(),
        preview_lines: vec![if active_tab == 0 {
            "Enter an absolute path (e.g. /opt/skills or ~/skills).".to_string()
        } else {
            "Enter a relative path (e.g. .agents/skills).".to_string()
        }],
        status: MenuItemStatus::Default,
        selectable: true,
    });
    items
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
// ============================================================
// File viewer: data model
// ============================================================

#[derive(Debug, Clone)]
pub(crate) struct FileViewEntry {
    pub(crate) name: String,
    pub(crate) relative_path: String,
    pub(crate) content: String,
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

    entries.push(FileViewEntry {
        name: "SKILL.md".to_string(),
        relative_path: "SKILL.md".to_string(),
        content: skill.content.clone(),
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
                    content: String::new(),
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
                content: resource.content.clone(),
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

fn entry_tree_label(entry: &FileViewEntry, collapsed: &HashSet<String>) -> String {
    let indent = "  ".repeat(entry.depth as usize);
    if entry.is_dir {
        let prefix = if collapsed.contains(entry.relative_path.as_str()) {
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
            let label = entry_tree_label(&ctx.entries[i], ctx.collapsed);
            ListItem::new(Line::from(label))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Files: {}", ctx.title)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{276f} ");
    frame.render_stateful_widget(list, panes[0], &mut list_state);

    // --- Right pane: content ---
    let selected_name = ctx.entries[ctx.selected_index].name.clone();
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

    let help = format!(
        "{}\u{2191}\u{2193} navigate  Enter/Space toggle dir  L line numbers  PgUp/PgDn scroll  Home/End  Esc back{}",
        scroll_pct,
        if scroll_pct.is_empty() { "" } else { "  " }
    );
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
    let title = skill.name.clone();

    let max_lines = entries
        .iter()
        .map(|e| e.content.lines().count())
        .max()
        .unwrap_or(0);
    let line_number_digits = if max_lines == 0 {
        1usize
    } else {
        max_lines.ilog10() as usize + 1
    };

    loop {
        let visible = compute_visible(&entries, &collapsed);
        if visible.is_empty() {
            break;
        }

        if !visible.contains(&selected_index) {
            selected_index = *visible.first().unwrap();
        }

        let content_lines: Vec<String> = entries[selected_index]
            .content
            .lines()
            .map(str::to_string)
            .collect();

        session.terminal.draw(|frame| {
            let mut ctx = FileViewerRenderCtx {
                entries: &entries,
                visible: &visible,
                selected_index,
                collapsed: &collapsed,
                scroll: &mut scroll,
                content_lines: &content_lines,
                title: &title,
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
                if entry.is_dir {
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
