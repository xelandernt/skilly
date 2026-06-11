use crate::client::{ClientConfig, SkillsMpClient, SkillsMpSearchQuery, SkillsMpSkill};
use crate::core::{
    ProjectDependencyOrigin, ProjectEnvironment, SKILLY_SOURCE_GITHUB, SKILLY_SOURCE_SKILLSMP,
    SKILLY_UNKNOWN_SOURCE, STATUS_INSTALLABLE, STATUS_INSTALLED, STATUS_UPDATABLE,
    ScanDependencySelection, SkillData, SkillDirectoryFlavor, SkillMatchData,
    available_dependency_skill_in, dependency_updates_in, discover_github_skills,
    discover_installed_skills, github_versions_match, project_requirements, remove_skill,
    scan_match_status, scan_project_in, skills_directory,
};
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, IsTerminal, Stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const BACK_CHOICE: &str = "back";
const DELETE_CHOICE: &str = "delete";
const EXIT_CHOICE: &str = "exit";
const INSTALL_CHOICE: &str = "install";
const REMOVE_CHOICE: &str = "remove";
const UPDATE_CHOICE: &str = "update";
const APPLY_ALL_CHOICE: &str = "apply selected";
const INSTALL_ALL_CHOICE: &str = "install selected";
const UPDATE_ALL_CHOICE: &str = "update selected";
const REMOVE_ALL_CHOICE: &str = "remove selected";
const DEFAULT_HELP_TEXT: &str = "Up/Down move | Enter select | Esc cancel";
const MULTI_SELECT_HELP_TEXT: &str = "↑↓ move | Space select | A all | Enter action | Esc cancel";
const DEFAULT_EMPTY_PREVIEW: &str = "No details available.";
const DEFAULT_CREATE_INSTRUCTIONS: &str =
    "# Instructions\n\nDescribe the procedure this skill should follow.";
const CREATE_HELP_TEXT: &str = "^S create | F2 create | ^X cancel | F10 cancel | Tab next field";
const LOADING_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

#[derive(Debug, Clone)]
struct MenuItemUi {
    label: String,
    preview_lines: Vec<String>,
    installed: bool,
}

#[derive(Debug, Clone)]
struct MenuUi {
    title: String,
    items: Vec<MenuItemUi>,
    default: usize,
    preview_title: String,
    status: Option<String>,
    help_text: String,
    empty_preview: String,
}

#[derive(Debug, Clone)]
struct DownloadableSkillMatch {
    available: SkillData,
    installed: Option<SkillData>,
}

impl DownloadableSkillMatch {
    fn status(&self) -> &'static str {
        match self.installed.as_ref() {
            None => STATUS_INSTALLABLE,
            Some(installed) if github_versions_match(installed, &self.available) => {
                STATUS_INSTALLED
            }
            Some(_) => STATUS_UPDATABLE,
        }
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    MoveUp,
    MoveDown,
    Select,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultiSelectMenuAction {
    MoveUp,
    MoveDown,
    ToggleSelect,
    SelectAll,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MultiSelectResult {
    Single(usize),
    Bulk(Vec<usize>),
}

impl TerminalSession {
    fn new() -> Result<Self> {
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

#[derive(Parser, Debug)]
#[command(name = "skilly", about = "Manage agent skills.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Create a specification-compliant skill.")]
    Create {
        #[arg(help = "Skill name. Required outside an interactive terminal.")]
        name: Option<String>,
        #[arg(long, help = "Describe what the skill does and when to use it.")]
        description: Option<String>,
        #[arg(long, help = "Markdown instructions for the SKILL.md body.")]
        instructions: Option<String>,
        #[arg(long, help = "License name or bundled license reference.")]
        license: Option<String>,
        #[arg(long, help = "Environment requirements for the skill.")]
        compatibility: Option<String>,
        #[arg(long, value_name = "KEY=VALUE", help = "Add frontmatter metadata.")]
        metadata: Vec<String>,
        #[arg(long, help = "Space-separated pre-approved tools.")]
        allowed_tools: Option<String>,
        #[arg(long, help = "Create an empty scripts directory.")]
        with_scripts: bool,
        #[arg(long, help = "Create an empty references directory.")]
        with_references: bool,
        #[arg(long, help = "Create an empty assets directory.")]
        with_assets: bool,
        #[arg(long, help = "Replace an existing skill atomically.")]
        overwrite: bool,
        #[arg(long, short = 'y', help = "Create without confirmation.")]
        yes: bool,
        #[command(flatten)]
        destination: DestinationArgs,
    },
    #[command(about = "Scan dependency-provided skills from pyproject.toml and .venv.")]
    Scan {
        #[command(flatten)]
        destination: DestinationArgs,
        #[command(flatten)]
        dependencies: ScanDependencyArgs,
    },
    #[command(about = "Download one or more skills from a GitHub repository URL.")]
    Download {
        #[arg(help = "GitHub repository, tree, or skill URL to download from.")]
        github_url: String,
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            help = "Select a specific skill when the GitHub URL resolves to multiple skills."
        )]
        skill_name: Option<String>,
        #[arg(long, help = "Download every skill found at the GitHub URL.")]
        all: bool,
        #[arg(
            long,
            help = "Overwrite existing files when installing the downloaded skill."
        )]
        overwrite: bool,
        #[arg(long, help = "GitHub token used for GitHub API requests.")]
        github_token: Option<String>,
    },
    #[command(about = "Browse, update, or remove installed skills.")]
    List {
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            help = "GitHub token used when checking for updates to GitHub-backed skills."
        )]
        github_token: Option<String>,
    },
    #[command(
        about = "Check installed skill updates in bulk and optionally apply them; use `skilly list` to review or update one skill at a time."
    )]
    Update {
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            short = 'y',
            help = "Apply every discovered update without asking for confirmation."
        )]
        yes: bool,
        #[arg(
            long,
            help = "GitHub token used when checking for updates to GitHub-backed skills."
        )]
        github_token: Option<String>,
    },
    #[command(about = "Remove one installed skill by directory name.")]
    Remove {
        #[arg(help = "Installed skill directory name to remove.")]
        name: String,
        #[command(flatten)]
        destination: DestinationArgs,
    },
    #[command(about = "Search and manage SkillsMP-backed skills.")]
    Skillsmp(SkillsMpCommands),
    #[command(about = "Utility commands for dependency and virtual environment inspection.")]
    Util(UtilCommands),
}

#[derive(Args, Debug)]
struct SkillsMpCommands {
    #[command(subcommand)]
    command: SkillsMpSubcommand,
}

#[derive(Args, Debug, Clone, Copy, Default)]
struct ScanDependencyArgs {
    #[arg(long, help = "Ignore [project].dependencies while scanning.")]
    no_project_dependencies: bool,
    #[arg(long, help = "Ignore [dependency-groups] while scanning.")]
    no_dependency_groups: bool,
    #[arg(long, help = "Ignore [project.optional-dependencies] while scanning.")]
    no_optional_dependencies: bool,
}

impl ScanDependencyArgs {
    fn selection(self) -> ScanDependencySelection {
        ScanDependencySelection {
            include_project_dependencies: !self.no_project_dependencies,
            include_dependency_groups: !self.no_dependency_groups,
            include_optional_dependencies: !self.no_optional_dependencies,
        }
    }
}

#[derive(Subcommand, Debug)]
enum SkillsMpSubcommand {
    #[command(about = "Search SkillsMP and install a selected result.")]
    Search {
        #[arg(help = "Search query sent to SkillsMP.")]
        query: String,
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            help = "Overwrite existing files when installing the selected skill."
        )]
        overwrite: bool,
        #[arg(
            long,
            help = "GitHub token used for GitHub API requests while resolving skill contents."
        )]
        github_token: Option<String>,
    },
    #[command(about = "Browse installed SkillsMP skills and manage updates.")]
    List {
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            help = "GitHub token used when checking for updates to SkillsMP-installed skills."
        )]
        github_token: Option<String>,
    },
}

#[derive(Args, Debug, Default)]
struct DestinationArgs {
    #[arg(
        long,
        help = "Directory where skilly installs managed skills; expands ~ and resolves to an absolute path before install."
    )]
    directory: Option<PathBuf>,
    #[arg(long, help = "Use the project-local skills directory.")]
    local: bool,
    #[arg(long, help = "Use the user-global skills directory.")]
    global: bool,
    #[arg(long, help = "Use the Claude skills directory.")]
    claude: bool,
    #[arg(long, help = "Use the Codex skills directory.")]
    codex: bool,
    #[arg(long, help = "Use the GitHub Copilot skills directory.")]
    copilot: bool,
}

impl DestinationArgs {
    fn resolve(&self) -> Result<PathBuf> {
        if let Some(directory) = self.directory.as_ref() {
            return absolute_path(directory);
        }
        if usize::from(self.local) + usize::from(self.global) > 1 {
            bail!("Use either --local or --global");
        }
        if usize::from(self.claude) + usize::from(self.codex) + usize::from(self.copilot) > 1 {
            bail!("Use only one of --claude, --codex, or --copilot");
        }
        let flavor = if self.claude {
            SkillDirectoryFlavor::Claude
        } else if self.codex {
            SkillDirectoryFlavor::Codex
        } else if self.copilot {
            SkillDirectoryFlavor::Copilot
        } else {
            SkillDirectoryFlavor::Agents
        };
        absolute_path(&skills_directory(flavor, self.global)?)
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let expanded = expand_home_path(path)?;
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    Ok(std::env::current_dir()?.join(expanded))
}

fn expand_home_path(path: &Path) -> Result<PathBuf> {
    let value = path.to_string_lossy();
    if value == "~" || value.starts_with("~/") {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        let home = PathBuf::from(home);
        if value == "~" {
            return Ok(home);
        }
        return Ok(home.join(value.trim_start_matches("~/")));
    }
    Ok(path.to_path_buf())
}

#[derive(Args, Debug)]
struct UtilCommands {
    #[command(subcommand)]
    command: UtilSubcommand,
}

#[derive(Subcommand, Debug)]
enum UtilSubcommand {
    #[command(about = "Print dependency names resolved from pyproject.toml.")]
    Dependencies {
        #[arg(
            long,
            default_value = "pyproject.toml",
            help = "Path to the pyproject.toml file to inspect."
        )]
        file: PathBuf,
        #[arg(long, help = "Include development dependencies.")]
        dev: bool,
        #[arg(long, help = "Include dependencies from the given optional extras.")]
        extras: Vec<String>,
    },
    #[command(about = "List skills discovered inside a virtual environment.")]
    Venv {
        #[arg(
            long,
            default_value = ".venv",
            help = "Virtual environment path to inspect."
        )]
        path: PathBuf,
        #[arg(
            long,
            help = "Include bundled resource details for each discovered skill."
        )]
        detailed: bool,
    },
}

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
    let cli =
        match Cli::try_parse_from(std::iter::once("skilly".to_string()).chain(args.into_iter())) {
            Ok(cli) => cli,
            Err(error) => {
                error.print()?;
                return Ok(if error.use_stderr() { 2 } else { 0 });
            }
        };

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
        } => run_scan(&destination.resolve()?, dependencies.selection())?,
        Commands::Download {
            github_url,
            destination,
            skill_name,
            all,
            overwrite,
            github_token,
        } => run_download(
            &github_url,
            &destination.resolve()?,
            skill_name.as_deref(),
            all,
            overwrite,
            client_config(None, None, github_token, None),
        )?,
        Commands::List {
            destination,
            github_token,
        } => run_list(
            &destination.resolve()?,
            client_config(None, None, github_token, None),
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
                &destination.resolve()?,
                overwrite,
                client_config(None, None, github_token, None),
            )?,
            SkillsMpSubcommand::List {
                destination,
                github_token,
            } => run_skillsmp_list(
                &destination.resolve()?,
                client_config(None, None, github_token, None),
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
    }

    Ok(0)
}

fn client_config(
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> ClientConfig {
    ClientConfig {
        base_url,
        api_key,
        github_token,
        proxy,
    }
}

#[derive(Debug)]
struct CreateOptions {
    name: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    license: Option<String>,
    compatibility: Option<String>,
    metadata: Vec<String>,
    allowed_tools: Option<String>,
    with_scripts: bool,
    with_references: bool,
    with_assets: bool,
    overwrite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateField {
    Name,
    Description,
    Instructions,
    License,
    Compatibility,
    Metadata,
    AllowedTools,
    WithScripts,
    WithReferences,
    WithAssets,
    Overwrite,
}

const CREATE_FIELDS: [CreateField; 11] = [
    CreateField::Name,
    CreateField::Description,
    CreateField::Instructions,
    CreateField::License,
    CreateField::Compatibility,
    CreateField::Metadata,
    CreateField::AllowedTools,
    CreateField::WithScripts,
    CreateField::WithReferences,
    CreateField::WithAssets,
    CreateField::Overwrite,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateAction {
    NextField,
    PreviousField,
    Save,
    Cancel,
    Insert(char),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveHome,
    MoveEnd,
    NewLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextBuffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl TextBuffer {
    fn from_text(value: &str) -> Self {
        let mut lines = value.split('\n').map(str::to_string).collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let cursor_row = lines.len().saturating_sub(1);
        let cursor_col = line_len(&lines[cursor_row]);
        Self {
            lines,
            cursor_row,
            cursor_col,
        }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn current_line_len(&self) -> usize {
        line_len(&self.lines[self.cursor_row])
    }

    fn clamp_cursor(&mut self) {
        self.cursor_row = self.cursor_row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.current_line_len());
    }

    fn insert_char(&mut self, value: char) {
        let line = &mut self.lines[self.cursor_row];
        let byte_index = char_to_byte_index(line, self.cursor_col);
        line.insert(byte_index, value);
        self.cursor_col += 1;
    }

    fn insert_newline(&mut self) {
        let current = self.lines[self.cursor_row].clone();
        let byte_index = char_to_byte_index(&current, self.cursor_col);
        let (prefix, suffix) = current.split_at(byte_index);
        self.lines[self.cursor_row] = prefix.to_string();
        self.lines.insert(self.cursor_row + 1, suffix.to_string());
        self.cursor_row += 1;
        self.cursor_col = 0;
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let end = char_to_byte_index(line, self.cursor_col);
            let start = char_to_byte_index(line, self.cursor_col - 1);
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
            let start = char_to_byte_index(line, self.cursor_col);
            let end = char_to_byte_index(line, self.cursor_col + 1);
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

    fn move_up(&mut self) {
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

    fn move_end(&mut self) {
        self.cursor_col = self.current_line_len();
    }

    fn render_lines(&self, area: Rect) -> (Vec<Line<'static>>, Option<(u16, u16)>) {
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
            .map(|line| crop_text(line, width))
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
struct CreateFormState {
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
    status_message: Option<String>,
}

impl CreateFormState {
    fn new(options: CreateOptions) -> Self {
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

    fn apply(&mut self, action: CreateAction) {
        self.status_message = None;
        match action {
            CreateAction::NextField => self.next_field(),
            CreateAction::PreviousField => self.previous_field(),
            CreateAction::Insert(value) if self.active_field_is_toggle() => {
                if value == ' ' {
                    self.toggle_active();
                }
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

    fn target_path(&self, directory: &Path) -> PathBuf {
        let name = self.name.text();
        let trimmed = name.trim();
        directory.join(if trimmed.is_empty() {
            "<name>"
        } else {
            trimmed
        })
    }

    fn metadata_lines(&self) -> Vec<String> {
        self.metadata
            .text()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn build_skill(&self, directory: &Path) -> Result<SkillData> {
        let skill = SkillData {
            name: self.name.text().trim().to_string(),
            description: self.description.text().trim().to_string(),
            path: None,
            content: self.instructions.text(),
            license: empty_to_none(self.license.text()),
            compatibility: empty_to_none(self.compatibility.text()),
            metadata: parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            github_url: None,
            github_commit_sha: None,
            skillsmp_id: None,
        };
        skill.validate()?;
        if !self.overwrite && self.target_path(directory).exists() {
            bail!("Skill directory already exists; enable overwrite to replace it");
        }
        Ok(skill)
    }

    fn preview_lines(&self, directory: &Path) -> Vec<String> {
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
            metadata: parse_metadata(&self.metadata_lines())?,
            allowed_tools: empty_to_none(self.allowed_tools.text()),
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_UNKNOWN_SOURCE.to_string(),
            package_name: None,
            package_version: None,
            github_url: None,
            github_commit_sha: None,
            skillsmp_id: None,
        })
    }

    fn field_summary(&self, field: CreateField) -> String {
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

    fn active_title(&self) -> &'static str {
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

    fn active_help(&self) -> &'static str {
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

fn interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn parse_metadata(values: &[String]) -> Result<BTreeMap<String, String>> {
    values
        .iter()
        .map(|value| {
            let (key, value) = value
                .split_once('=')
                .filter(|(key, _)| !key.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Metadata must use KEY=VALUE: {value}"))?;
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn run_create(directory: &Path, mut options: CreateOptions) -> Result<()> {
    let interactive = interactive_terminal();
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

fn run_create_tui(directory: &Path, options: CreateOptions) -> Result<Option<CreateOptions>> {
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

fn render_create_form(
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
        .highlight_symbol("❯ ");
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

fn select_menu(session: &mut TerminalSession, menu: MenuUi) -> Result<Option<usize>> {
    if menu.items.is_empty() {
        return Ok(None);
    }

    let mut state = ListState::default();
    let mut selected = menu.default.min(menu.items.len().saturating_sub(1));
    state.select(Some(selected));

    loop {
        session.terminal.draw(|frame| {
            let status_height = if menu.status.is_some() { 1 } else { 0 };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(status_height),
                    Constraint::Length(1),
                ])
                .split(frame.area());

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[1]);

            let items = menu
                .items
                .iter()
                .map(|item| {
                    let style = if item.installed {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(item.label.clone())).style(style)
                })
                .collect::<Vec<_>>();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Options"))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("❯ ");
            frame.render_widget(
                Paragraph::new(menu.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                layout[0],
            );
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
                    layout[2],
                );
            }
            frame.render_widget(
                Paragraph::new(menu.help_text.clone()).style(Style::default().fg(Color::Gray)),
                layout[3],
            );
        })?;

        if let Event::Key(key) = event::read()? {
            match menu_action(key) {
                Some(MenuAction::MoveUp) => {
                    selected = selected.saturating_sub(1);
                    state.select(Some(selected));
                }
                Some(MenuAction::MoveDown) => {
                    selected = (selected + 1).min(menu.items.len().saturating_sub(1));
                    state.select(Some(selected));
                }
                Some(MenuAction::Select) => return Ok(Some(selected)),
                Some(MenuAction::Cancel) => return Ok(None),
                None => {}
            }
        }
    }
}

fn multi_select_menu(
    session: &mut TerminalSession,
    menu: MenuUi,
    selectable_count: usize,
    initially_checked: &[usize],
) -> Result<Option<MultiSelectResult>> {
    if menu.items.is_empty() {
        return Ok(None);
    }

    let mut state = ListState::default();
    let mut focused = menu.default.min(menu.items.len().saturating_sub(1));
    state.select(Some(focused));
    let mut checked: HashSet<usize> = initially_checked
        .iter()
        .copied()
        .filter(|index| *index < selectable_count)
        .collect();

    loop {
        session.terminal.draw(|frame| {
            let status_height = if menu.status.is_some() { 1 } else { 0 };
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(status_height),
                    Constraint::Length(1),
                ])
                .split(frame.area());

            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[1]);

            let items = menu
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let label = if i < selectable_count {
                        let checkbox = if checked.contains(&i) {
                            "[✓] "
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
                    } else if item.installed {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
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
                .highlight_symbol("❯ ");

            frame.render_widget(
                Paragraph::new(menu.title.clone())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                layout[0],
            );
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
                    layout[2],
                );
            }
            frame.render_widget(
                Paragraph::new(menu.help_text.clone()).style(Style::default().fg(Color::Gray)),
                layout[3],
            );
        })?;

        if let Event::Key(key) = event::read()? {
            match multi_select_action(key) {
                Some(MultiSelectMenuAction::MoveUp) => {
                    focused = focused.saturating_sub(1);
                    state.select(Some(focused));
                }
                Some(MultiSelectMenuAction::MoveDown) => {
                    focused = (focused + 1).min(menu.items.len().saturating_sub(1));
                    state.select(Some(focused));
                }
                Some(MultiSelectMenuAction::ToggleSelect) => {
                    if focused < selectable_count {
                        if checked.contains(&focused) {
                            checked.remove(&focused);
                        } else {
                            checked.insert(focused);
                        }
                    }
                }
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
                        return Ok(Some(MultiSelectResult::Single(focused)));
                    }
                    let mut indices: Vec<usize> = checked.iter().copied().collect();
                    indices.sort_unstable();
                    return Ok(Some(MultiSelectResult::Bulk(indices)));
                }
                Some(MultiSelectMenuAction::Cancel) => return Ok(None),
                None => {}
            }
        }
    }
}

fn menu_action(key: KeyEvent) -> Option<MenuAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Up => Some(MenuAction::MoveUp),
        KeyCode::Down => Some(MenuAction::MoveDown),
        KeyCode::Enter => Some(MenuAction::Select),
        KeyCode::Esc => Some(MenuAction::Cancel),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(MenuAction::Cancel)
        }
        _ => None,
    }
}

fn multi_select_action(key: KeyEvent) -> Option<MultiSelectMenuAction> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Up => Some(MultiSelectMenuAction::MoveUp),
        KeyCode::Down => Some(MultiSelectMenuAction::MoveDown),
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

fn create_action(key: KeyEvent) -> Option<CreateAction> {
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

fn create_field_label(field: CreateField) -> &'static str {
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

fn requested_directories(form: &CreateFormState) -> Vec<&'static str> {
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

fn char_to_byte_index(value: &str, offset: usize) -> usize {
    value
        .char_indices()
        .nth(offset)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn line_len(value: &str) -> usize {
    value.chars().count()
}

fn crop_text(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

fn summarize_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    truncate_summary(trimmed)
}

fn summarize_multiline(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    if trimmed.contains('\n') {
        return format!("{} lines", trimmed.lines().count());
    }
    truncate_summary(trimmed)
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn truncate_summary(value: &str) -> String {
    let max_len = 24usize;
    let mut chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_len {
        return value.to_string();
    }
    chars.truncate(max_len.saturating_sub(1));
    format!("{}…", chars.into_iter().collect::<String>())
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn placeholder_if_empty(value: String, placeholder: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        placeholder.to_string()
    } else {
        trimmed.to_string()
    }
}

fn show_loading_message<T, F>(title: &str, message: &str, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    if !interactive_terminal() {
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

        match receiver.recv_timeout(Duration::from_millis(120)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                frame_index = frame_index.wrapping_add(1);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Loading task ended unexpectedly");
            }
        }
    }
}

fn run_scan(directory: &Path, dependency_selection: ScanDependencySelection) -> Result<()> {
    let environment = ProjectEnvironment::with_paths(
        directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        dependency_selection,
    );
    let matches = scan_project_in(&environment)?;
    if matches.is_empty() {
        println!("No dependency skills found in pyproject.toml and .venv");
        return Ok(());
    }

    let mut actionable = matches
        .into_iter()
        .filter(|item| {
            scan_match_status(&item.available, item.installed.as_ref()) != STATUS_INSTALLED
        })
        .collect::<Vec<_>>();
    if actionable.is_empty() {
        println!("All discovered dependency skills are already installed");
        return Ok(());
    }
    if !interactive_terminal() {
        println!("Dependency skills requiring action:");
        for item in actionable {
            println!("{}", scan_choice_label(&item));
        }
        return Ok(());
    }

    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_index = 0;
    let mut checked_indices = Vec::new();
    while !actionable.is_empty() {
        let selectable_count = actionable.len();
        let mut items = actionable
            .iter()
            .map(|item| MenuItemUi {
                label: scan_choice_label(item),
                preview_lines: scan_match_preview_lines(item),
                installed: false,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit scan"));

        let Some(result) = multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    "Select dependency skills to install".to_string(),
                    directory,
                ),
                items,
                default: selected_index,
                preview_title: "Dependency skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
            selectable_count,
            &checked_indices,
        )?
        else {
            break;
        };

        match result {
            MultiSelectResult::Single(index) => {
                checked_indices.clear();
                if index == selectable_count {
                    break;
                }
                selected_index = index;
                let selected = actionable[index].clone();
                let actions = scan_skill_actions(&selected);
                let Some(action_index) = select_menu(
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
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: selected.available.name.clone(),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?
                else {
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
                        actionable.retain(|item| !item.available.matches(&selected.available));
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
                selected_index = indices.first().copied().unwrap_or(selected_index);
                let selected: Vec<SkillMatchData> =
                    indices.iter().map(|&i| actionable[i].clone()).collect();
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
                                preview_lines: preview_names.lines().map(str::to_owned).collect(),
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: "Selected skills".to_string(),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?;
                checked_indices = retained_multi_select_indices(
                    action_index.map(|index| actions[index]),
                    &indices,
                );
                let Some(action_index) = action_index else {
                    continue;
                };
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
                        for item in &selected {
                            actionable.retain(|a| !a.available.matches(&item.available));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_download(
    github_url: &str,
    directory: &Path,
    skill_name: Option<&str>,
    all: bool,
    overwrite: bool,
    config: ClientConfig,
) -> Result<()> {
    let client = SkillsMpClient::new(config)?;
    let mut skills = discover_github_skills(&client, github_url, SKILLY_SOURCE_GITHUB, None)?;
    if all && skill_name.is_some() && skills.len() != 1 {
        bail!("Use either --skill-name or --all when downloading multiple skills");
    }
    if skills.len() > 1 && !all && skill_name.is_none() {
        if !interactive_terminal() {
            bail!("Multiple skills found; use --skill-name <name> or --all");
        }
        return download_selected_skills(&client, directory, overwrite, &skills);
    }
    if let Some(skill_name) = skill_name {
        if skills.len() != 1 && all {
            bail!("Custom skill names can only be used when downloading a single skill");
        }
        if skills.len() != 1 {
            skills = vec![select_download_skill(&skills, skill_name)?];
        }
    }

    let installed = skills
        .iter()
        .map(|skill| {
            skill.install_to(
                directory,
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
    directory: &Path,
    overwrite: bool,
    skills: &[SkillData],
) -> Result<()> {
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_index = 0;
    let mut checked_indices = Vec::new();
    loop {
        let matches = downloadable_skill_matches(skills, &discover_installed_skills(directory)?);
        let selectable_count = matches.len();
        let mut items = matches
            .iter()
            .map(|item| MenuItemUi {
                label: downloadable_skill_label(item),
                preview_lines: downloadable_skill_preview_lines(item, directory),
                installed: item.status() == STATUS_INSTALLED,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit download"));
        let Some(result) = multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    "Select skills to download".to_string(),
                    directory,
                ),
                items,
                default: selected_index,
                preview_title: "Downloadable skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
            selectable_count,
            &checked_indices,
        )?
        else {
            break;
        };

        match result {
            MultiSelectResult::Single(index) => {
                checked_indices.clear();
                if index == selectable_count {
                    break;
                }
                selected_index = index;
                let selected = matches[index].clone();
                let actions = downloadable_skill_actions(&selected);
                let Some(action_index) = select_menu(
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
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: skill_directory_name(&selected.available),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?
                else {
                    continue;
                };
                match actions[action_index] {
                    BACK_CHOICE => continue,
                    EXIT_CHOICE => break,
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
            MultiSelectResult::Bulk(indices) => {
                selected_index = indices.first().copied().unwrap_or(selected_index);
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
                                preview_lines: preview_names.lines().map(str::to_owned).collect(),
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: "Selected skills".to_string(),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?;
                checked_indices = retained_multi_select_indices(
                    action_index.map(|index| actions[index]),
                    &indices,
                );
                let Some(action_index) = action_index else {
                    continue;
                };
                match actions[action_index] {
                    BACK_CHOICE => continue,
                    EXIT_CHOICE => break,
                    INSTALL_ALL_CHOICE => {
                        for m in selected.iter().filter(|m| m.status() == STATUS_INSTALLABLE) {
                            let installed = m.available.install_to(directory, None, overwrite)?;
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
                            let removed =
                                remove_skill(&skill_directory_name(installed_skill), directory)?;
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
        }
    }

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_list(directory: &Path, config: ClientConfig) -> Result<()> {
    let skills = discover_installed_skills(directory)?;
    if skills.is_empty() {
        println!("{}", no_skills_found_message(directory));
        return Ok(());
    }

    if !interactive_terminal() {
        for skill in skills {
            println!("{}", installed_skill_label(&skill));
        }
        return Ok(());
    }
    let client = SkillsMpClient::new(config)?;
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_index = 0;
    let mut checked_indices = Vec::new();
    loop {
        let skills = discover_installed_skills(directory)?;
        if skills.is_empty() {
            break;
        }
        let selectable_count = skills.len();
        let mut items = skills
            .iter()
            .map(|skill| MenuItemUi {
                label: installed_skill_label(skill),
                preview_lines: installed_skill_preview_lines(skill),
                installed: false,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit list"));
        let Some(result) = multi_select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory("Select installed skills".to_string(), directory),
                items,
                default: selected_index,
                preview_title: "Installed skill".to_string(),
                status: status_message.clone(),
                help_text: MULTI_SELECT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
            selectable_count,
            &checked_indices,
        )?
        else {
            break;
        };

        match result {
            MultiSelectResult::Single(index) => {
                checked_indices.clear();
                if index == selectable_count {
                    break;
                }
                selected_index = index;
                let selected = skills[index].clone();
                let update_available = update_available_or_remember_error(
                    directory,
                    &selected,
                    &client,
                    &mut status_message,
                );
                let actions = installed_skill_actions(update_available, REMOVE_CHOICE);
                let Some(action_index) = select_menu(
                    &mut session,
                    MenuUi {
                        title: menu_title_with_directory(
                            format!("Choose an action for {}", skill_directory_name(&selected)),
                            directory,
                        ),
                        items: actions
                            .iter()
                            .map(|item| MenuItemUi {
                                label: (*item).to_string(),
                                preview_lines: installed_skill_preview_lines(&selected),
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: skill_directory_name(&selected),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?
                else {
                    continue;
                };
                match actions[action_index] {
                    BACK_CHOICE => continue,
                    EXIT_CHOICE => break,
                    UPDATE_CHOICE => {
                        remember_status(
                            &mut messages,
                            &mut status_message,
                            update_skill(directory, &selected, &client)?,
                        );
                    }
                    REMOVE_CHOICE => {
                        let removed = remove_skill(&skill_directory_name(&selected), directory)?;
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
                selected_index = indices.first().copied().unwrap_or(selected_index);
                let selected: Vec<SkillData> = indices.iter().map(|&i| skills[i].clone()).collect();
                let preview_names = selected
                    .iter()
                    .map(skill_directory_name)
                    .collect::<Vec<_>>()
                    .join("\n");
                let actions = vec![
                    REMOVE_ALL_CHOICE,
                    UPDATE_ALL_CHOICE,
                    BACK_CHOICE,
                    EXIT_CHOICE,
                ];
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
                                preview_lines: preview_names.lines().map(str::to_owned).collect(),
                                installed: false,
                            })
                            .collect(),
                        default: action_menu_default(&actions),
                        preview_title: "Selected skills".to_string(),
                        status: status_message.clone(),
                        help_text: DEFAULT_HELP_TEXT.to_string(),
                        empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
                    },
                )?;
                checked_indices = retained_multi_select_indices(
                    action_index.map(|index| actions[index]),
                    &indices,
                );
                let Some(action_index) = action_index else {
                    continue;
                };
                match actions[action_index] {
                    BACK_CHOICE => continue,
                    EXIT_CHOICE => break,
                    REMOVE_ALL_CHOICE => {
                        for skill in &selected {
                            let removed = remove_skill(&skill_directory_name(skill), directory)?;
                            remember_status(
                                &mut messages,
                                &mut status_message,
                                format!("Removed {}", skill_directory_name(&removed)),
                            );
                        }
                    }
                    UPDATE_ALL_CHOICE => {
                        for skill in &selected {
                            match update_skill(directory, skill, &client) {
                                Ok(msg) if !msg.contains("already up to date") => {
                                    remember_status(&mut messages, &mut status_message, msg);
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    remember_status(
                                        &mut messages,
                                        &mut status_message,
                                        format!(
                                            "Failed to update {}: {e}",
                                            skill_directory_name(skill)
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
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
    let installed_skills = discover_installed_skills(directory)?;
    let environment = ProjectEnvironment::with_paths(
        directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        ScanDependencySelection::default(),
    );

    let mut updates = Vec::new();
    for item in dependency_updates_in(&environment)? {
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
    directory: &Path,
    overwrite: bool,
    config: ClientConfig,
) -> Result<()> {
    let client = SkillsMpClient::new(config.clone())?;
    let response = client.search(&SkillsMpSearchQuery::new(query))?;
    if response.data.skills.is_empty() {
        println!("No SkillsMP skills found for {query}");
        return Ok(());
    }
    if !interactive_terminal() {
        for skill in response.data.skills {
            println!("{} [{}] ({})", skill.name, skill.author, skill.id);
        }
        return Ok(());
    }

    let mut session = TerminalSession::new()?;
    let mut cache = std::collections::BTreeMap::<String, SkillData>::new();
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_index = 0;
    loop {
        let installed_skills = discover_installed_skills(directory)?;
        let search_matches = response
            .data
            .skills
            .iter()
            .map(|skill| (skill, installed_skillsmp_match(skill, &installed_skills)))
            .collect::<Vec<_>>();
        let mut items = response
            .data
            .skills
            .iter()
            .zip(search_matches.iter())
            .map(|(skill, (_matched_skill, installed))| MenuItemUi {
                label: search_skill_label(skill, installed.as_ref()),
                preview_lines: skillsmp_search_preview_lines(skill, installed.as_ref(), directory),
                installed: installed.is_some(),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit search"));
        let Some(index) = select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    format!("Select a skill for \"{query}\""),
                    directory,
                ),
                items,
                default: selected_index,
                preview_title: "SkillsMP result".to_string(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
        )?
        else {
            break;
        };
        if index == response.data.skills.len() {
            break;
        }
        selected_index = index;
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
        let Some(action_index) = select_menu(
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
                        installed: false,
                    })
                    .collect(),
                default: action_menu_default(&actions),
                preview_title: skill.name.clone(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
        )?
        else {
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

fn run_skillsmp_list(directory: &Path, config: ClientConfig) -> Result<()> {
    if !interactive_terminal() {
        let skills = discover_installed_skills(directory)?
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
    let client = SkillsMpClient::new(config)?;
    let skills = discover_installed_skills(directory)?
        .into_iter()
        .filter(|skill| skill.is_skillsmp())
        .collect::<Vec<_>>();
    if skills.is_empty() {
        println!(
            "No SkillsMP-installed skills found in {}",
            directory.display()
        );
        return Ok(());
    }
    let mut session = TerminalSession::new()?;
    let mut messages = Vec::new();
    let mut status_message = None;
    let mut selected_index = 0;
    loop {
        let skills = discover_installed_skills(directory)?
            .into_iter()
            .filter(|skill| skill.is_skillsmp())
            .collect::<Vec<_>>();
        if skills.is_empty() {
            break;
        }
        let mut items = skills
            .iter()
            .map(|skill| MenuItemUi {
                label: installed_skill_label(skill),
                preview_lines: installed_skill_preview_lines(skill),
                installed: false,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit list"));
        let Some(index) = select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    "Select an installed SkillsMP skill".to_string(),
                    directory,
                ),
                items,
                default: selected_index,
                preview_title: "Installed skill".to_string(),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
        )?
        else {
            break;
        };
        if index == skills.len() {
            break;
        }
        selected_index = index;
        let selected = skills[index].clone();
        let update_available =
            update_available_or_remember_error(directory, &selected, &client, &mut status_message);
        let actions = installed_skill_actions(update_available, DELETE_CHOICE);
        let Some(action_index) = select_menu(
            &mut session,
            MenuUi {
                title: menu_title_with_directory(
                    format!("Choose an action for {}", skill_directory_name(&selected)),
                    directory,
                ),
                items: actions
                    .iter()
                    .map(|item| MenuItemUi {
                        label: (*item).to_string(),
                        preview_lines: installed_skill_preview_lines(&selected),
                        installed: false,
                    })
                    .collect(),
                default: action_menu_default(&actions),
                preview_title: skill_directory_name(&selected),
                status: status_message.clone(),
                help_text: DEFAULT_HELP_TEXT.to_string(),
                empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
            },
        )?
        else {
            continue;
        };
        match actions[action_index] {
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
                let removed = remove_skill(&skill_directory_name(&selected), directory)?;
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

fn scan_primary_action(item: &SkillMatchData) -> &'static str {
    if item.installed.is_some() {
        UPDATE_CHOICE
    } else {
        INSTALL_CHOICE
    }
}

fn scan_skill_actions(item: &SkillMatchData) -> [&'static str; 3] {
    [scan_primary_action(item), BACK_CHOICE, EXIT_CHOICE]
}

fn installed_skill_actions(update_available: bool, remove_choice: &str) -> Vec<&str> {
    let mut actions = vec![remove_choice, BACK_CHOICE, EXIT_CHOICE];
    if update_available {
        actions.insert(0, UPDATE_CHOICE);
    }
    actions
}

fn action_menu_default(actions: &[&str]) -> usize {
    actions
        .iter()
        .position(|action| {
            matches!(
                *action,
                INSTALL_CHOICE
                    | UPDATE_CHOICE
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

fn absolute_skill_path(skill: &SkillData) -> Option<PathBuf> {
    skill.path.as_deref().map(PathBuf::from)
}

fn target_skill_path(skill: &SkillData, directory: &Path) -> PathBuf {
    directory.join(skill_directory_name(skill))
}

fn no_skills_found_message(directory: &Path) -> String {
    format!("No skills found in directory {}", directory.display())
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
        STATUS_INSTALLABLE => vec![INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        STATUS_UPDATABLE => vec![UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        _ => vec![REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
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
            "Downloadable skill not found: {}. Available: {}",
            skill_name,
            skills
                .iter()
                .map(skill_directory_name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => bail!("Multiple downloadable skills match name: {skill_name}"),
    }
}

fn exit_menu_item(label: &str) -> MenuItemUi {
    MenuItemUi {
        label: EXIT_CHOICE.to_string(),
        preview_lines: vec![label.to_string()],
        installed: false,
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

#[cfg(test)]
mod tests {
    use super::{
        APPLY_ALL_CHOICE, BACK_CHOICE, Cli, Commands, CreateAction, DownloadableSkillMatch,
        EXIT_CHOICE, INSTALL_ALL_CHOICE, INSTALL_CHOICE, MenuAction, PendingSkillUpdate,
        REMOVE_CHOICE, ScanDependencyArgs, TextBuffer, UPDATE_ALL_CHOICE, UPDATE_CHOICE,
        absolute_path, action_menu_default, create_action, downloadable_skill_actions,
        downloadable_skill_preview_lines, format_pending_update, installed_skill_actions,
        installed_skill_preview_lines, installed_skillsmp_match, menu_action,
        retained_multi_select_indices, scan_choice_label, scan_match_preview_lines,
        search_skill_label, skillsmp_search_preview_lines, skillsmp_search_status,
    };
    use crate::client::SkillsMpSkill;
    use crate::core::{
        ProjectDependencyOrigin, SKILLY_SOURCE_GITHUB, SKILLY_SOURCE_SKILLSMP, SkillData,
        SkillMatchData,
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
        }
    }

    fn dependency_match(origins: Vec<ProjectDependencyOrigin>) -> SkillMatchData {
        SkillMatchData {
            available: SkillData {
                name: "python".to_string(),
                description: "Dependency skill".to_string(),
                path: None,
                content: "Body".to_string(),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools: None,
                resources: Vec::new(),
                resource_warnings: Vec::new(),
                source: "dependency".to_string(),
                package_name: Some("sample-pkg".to_string()),
                package_version: Some("1.2.4".to_string()),
                github_url: None,
                github_commit_sha: None,
                skillsmp_id: None,
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
            absolute_path(Path::new("custom")).expect("relative path should resolve"),
            std::env::current_dir()
                .expect("current directory should resolve")
                .join("custom")
        );
        assert_eq!(
            absolute_path(Path::new("~/.copilot")).expect("home path should resolve"),
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
            vec![REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
        );
        assert_eq!(
            installed_skill_actions(true, REMOVE_CHOICE),
            vec![UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
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
    fn downloadable_skill_actions_omit_update_when_versions_match() {
        let installed = installed_skill(None, Some("https://github.com/example/repo"));
        let available = installed.clone();
        let matched = DownloadableSkillMatch {
            available,
            installed: Some(installed),
        };

        assert_eq!(
            downloadable_skill_actions(&matched),
            vec![REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
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
        let selection = ScanDependencyArgs::default().selection();

        assert!(selection.include_project_dependencies);
        assert!(selection.include_dependency_groups);
        assert!(selection.include_optional_dependencies);
    }

    #[test]
    fn scan_choice_label_and_preview_include_dependency_origins() {
        let item = dependency_match(vec![
            ProjectDependencyOrigin::Project,
            ProjectDependencyOrigin::DependencyGroup {
                group: "dev".to_string(),
            },
            ProjectDependencyOrigin::OptionalDependency {
                extra: "docs".to_string(),
            },
        ]);

        let label = scan_choice_label(&item);
        let preview = scan_match_preview_lines(&item);

        assert_eq!(
            label,
            "python [sample-pkg==1.2.4] [project, group:dev, extra:docs] [installable]"
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "Dependency Sources: project, group:dev, extra:docs")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "  - dependency group: dev")
        );
        assert!(
            preview
                .iter()
                .any(|line| line == "  - optional dependency: docs")
        );
    }
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
