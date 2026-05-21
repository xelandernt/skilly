use crate::client::{ClientConfig, SkillsMpClient, SkillsMpSearchQuery, SkillsMpSkill};
use crate::core::{
    DEFAULT_SKILLS_PATH, ProjectEnvironment, SKILLY_SOURCE_GITHUB, SKILLY_SOURCE_SKILLSMP,
    STATUS_INSTALLABLE, STATUS_INSTALLED, STATUS_UPDATABLE, SkillData, SkillMatchData,
    available_dependency_skill_in, dependency_updates_in, discover_github_skills,
    discover_installed_skills, github_versions_match, project_requirements, remove_skill,
    scan_match_status, scan_project_in,
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
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::fs;
use std::io::{self, Stdout};
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
const DEFAULT_HELP_TEXT: &str = "Up/Down move | Enter select | Esc cancel";
const DEFAULT_EMPTY_PREVIEW: &str = "No details available.";
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
    #[command(about = "Scan dependency-provided skills from pyproject.toml and .venv.")]
    Scan {
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory where skilly installs managed skills."
        )]
        directory: PathBuf,
        #[arg(long, help = "Include development dependencies while scanning.")]
        dev: bool,
    },
    #[command(about = "Download one or more skills from a GitHub repository URL.")]
    Download {
        #[arg(help = "GitHub repository, tree, or skill URL to download from.")]
        github_url: String,
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory where downloaded skills are installed."
        )]
        directory: PathBuf,
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
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory containing installed skills."
        )]
        directory: PathBuf,
        #[arg(
            long,
            help = "GitHub token used when checking for updates to GitHub-backed skills."
        )]
        github_token: Option<String>,
    },
    #[command(
        about = "Show or apply dependency skill updates discovered from the current project."
    )]
    Update {
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory containing installed skills."
        )]
        directory: PathBuf,
        #[arg(
            long,
            help = "Apply the discovered updates instead of only printing them."
        )]
        force: bool,
    },
    #[command(about = "Remove one installed skill by directory name.")]
    Remove {
        #[arg(help = "Installed skill directory name to remove.")]
        name: String,
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory containing installed skills."
        )]
        directory: PathBuf,
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

#[derive(Subcommand, Debug)]
enum SkillsMpSubcommand {
    #[command(about = "Search SkillsMP and install a selected result.")]
    Search {
        #[arg(help = "Search query sent to SkillsMP.")]
        query: String,
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory where installed skills are stored."
        )]
        directory: PathBuf,
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
        #[arg(
            long,
            default_value = DEFAULT_SKILLS_PATH,
            help = "Directory containing installed skills."
        )]
        directory: PathBuf,
        #[arg(
            long,
            help = "GitHub token used when checking for updates to SkillsMP-installed skills."
        )]
        github_token: Option<String>,
    },
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

pub fn run(args: Vec<String>) -> Result<i32> {
    let cli =
        match Cli::try_parse_from(std::iter::once("skilly".to_string()).chain(args.into_iter())) {
            Ok(cli) => cli,
            Err(error) => {
                error.print()?;
                return Ok(if error.use_stderr() { 2 } else { 0 });
            }
        };

    match cli.command {
        Commands::Scan { directory, dev } => run_scan(&directory, dev)?,
        Commands::Download {
            github_url,
            directory,
            skill_name,
            all,
            overwrite,
            github_token,
        } => run_download(
            &github_url,
            &directory,
            skill_name.as_deref(),
            all,
            overwrite,
            client_config(None, None, github_token, None),
        )?,
        Commands::List {
            directory,
            github_token,
        } => run_list(&directory, client_config(None, None, github_token, None))?,
        Commands::Update { directory, force } => run_update(&directory, force)?,
        Commands::Remove { name, directory } => {
            let removed = remove_skill(&name, &directory)?;
            println!("Removed {}", skill_directory_name(&removed));
        }
        Commands::Skillsmp(skillsmp) => match skillsmp.command {
            SkillsMpSubcommand::Search {
                query,
                directory,
                overwrite,
                github_token,
            } => run_skillsmp_search(
                &query,
                &directory,
                overwrite,
                client_config(None, None, github_token, None),
            )?,
            SkillsMpSubcommand::List {
                directory,
                github_token,
            } => run_skillsmp_list(&directory, client_config(None, None, github_token, None))?,
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

fn select_menu(menu: MenuUi) -> Result<Option<usize>> {
    if menu.items.is_empty() {
        return Ok(None);
    }

    let mut session = TerminalSession::new()?;
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

fn show_loading_message<T, F>(title: &str, message: &str, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
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

fn run_scan(directory: &Path, include_dev: bool) -> Result<()> {
    let environment = ProjectEnvironment::with_paths(
        directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        include_dev,
        &[],
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

    let mut messages = Vec::new();
    let mut status_message = None;
    while !actionable.is_empty() {
        let mut items = actionable
            .iter()
            .map(|item| MenuItemUi {
                label: scan_choice_label(item),
                preview_lines: scan_match_preview_lines(item),
                installed: false,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit scan"));

        let Some(index) = select_menu(MenuUi {
            title: "Select dependency skill to install".to_string(),
            items,
            default: 0,
            preview_title: "Dependency skill".to_string(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            break;
        };
        if index == actionable.len() {
            break;
        }

        let selected = actionable[index].clone();
        let actions = scan_skill_actions(&selected);
        let Some(action_index) = select_menu(MenuUi {
            title: format!("Choose an action for {}", selected.available.name),
            items: actions
                .iter()
                .map(|item| MenuItemUi {
                    label: (*item).to_string(),
                    preview_lines: scan_match_preview_lines(&selected),
                    installed: false,
                })
                .collect(),
            default: 0,
            preview_title: selected.available.name.clone(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            continue;
        };
        let action = actions[action_index];
        match action {
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
    let mut messages = Vec::new();
    let mut status_message = None;
    loop {
        let matches = downloadable_skill_matches(skills, &discover_installed_skills(directory)?);
        let mut items = matches
            .iter()
            .map(|item| MenuItemUi {
                label: downloadable_skill_label(item),
                preview_lines: downloadable_skill_preview_lines(item),
                installed: item.status() == STATUS_INSTALLED,
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit download"));
        let Some(index) = select_menu(MenuUi {
            title: "Select a skill to download".to_string(),
            items,
            default: 0,
            preview_title: "Downloadable skill".to_string(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            break;
        };
        if index == matches.len() {
            break;
        }
        let selected = matches[index].clone();
        let actions = downloadable_skill_actions(&selected);
        let Some(action_index) = select_menu(MenuUi {
            title: format!(
                "Choose an action for {}",
                skill_directory_name(&selected.available)
            ),
            items: actions
                .iter()
                .map(|item| MenuItemUi {
                    label: (*item).to_string(),
                    preview_lines: downloadable_skill_preview_lines(&selected),
                    installed: false,
                })
                .collect(),
            default: 0,
            preview_title: skill_directory_name(&selected.available),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            continue;
        };
        match actions[action_index] {
            BACK_CHOICE => continue,
            EXIT_CHOICE => break,
            INSTALL_CHOICE => {
                let installed = selected.available.install_to(directory, None, overwrite)?;
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

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_list(directory: &Path, config: ClientConfig) -> Result<()> {
    let client = SkillsMpClient::new(config)?;
    let mut messages = Vec::new();
    let mut status_message = None;
    loop {
        let skills = discover_installed_skills(directory)?;
        if skills.is_empty() {
            if messages.is_empty() {
                println!("No installed skills found in {}", directory.display());
            }
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
        let Some(index) = select_menu(MenuUi {
            title: "Select an installed skill".to_string(),
            items,
            default: 0,
            preview_title: "Installed skill".to_string(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            break;
        };
        if index == skills.len() {
            break;
        }
        let selected = skills[index].clone();
        let actions = installed_skill_actions(&selected, REMOVE_CHOICE);
        let Some(action_index) = select_menu(MenuUi {
            title: format!("Choose an action for {}", skill_directory_name(&selected)),
            items: actions
                .iter()
                .map(|item| MenuItemUi {
                    label: (*item).to_string(),
                    preview_lines: installed_skill_preview_lines(&selected),
                    installed: false,
                })
                .collect(),
            default: 0,
            preview_title: skill_directory_name(&selected),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
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

    if !messages.is_empty() {
        println!("{}", messages.join("\n"));
    }
    Ok(())
}

fn run_update(directory: &Path, force: bool) -> Result<()> {
    let environment = ProjectEnvironment::with_paths(
        directory,
        Path::new("pyproject.toml"),
        Path::new(".venv"),
        false,
        &[],
    );
    let matches = dependency_updates_in(&environment)?;
    if matches.is_empty() {
        println!("No dependency skill updates available");
        return Ok(());
    }
    for item in &matches {
        let installed = item.installed.as_ref().context("Missing installed skill")?;
        println!(
            "{}: {} {} -> {}",
            skill_directory_name(installed),
            item.available.package_name.as_deref().unwrap_or("unknown"),
            installed.package_version.as_deref().unwrap_or("unknown"),
            item.available
                .package_version
                .as_deref()
                .unwrap_or("unknown")
        );
    }
    if !force {
        println!("Run with --force to apply these updates");
        return Ok(());
    }
    for item in &matches {
        let installed = item.installed.as_ref().context("Missing installed skill")?;
        let updated = install_available_skill(
            directory,
            &item.available,
            Some(&skill_directory_name(installed)),
            true,
        )?;
        println!(
            "Updated {} to {}",
            skill_directory_name(&updated),
            updated.package_version.as_deref().unwrap_or("unknown")
        );
    }
    Ok(())
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

    let mut cache = std::collections::BTreeMap::<String, SkillData>::new();
    let mut messages = Vec::new();
    let mut status_message = None;
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
                preview_lines: skillsmp_search_preview_lines(skill, installed.as_ref()),
                installed: installed.is_some(),
            })
            .collect::<Vec<_>>();
        items.push(exit_menu_item("Exit search"));
        let Some(index) = select_menu(MenuUi {
            title: format!("Select a skill for \"{query}\""),
            items,
            default: 0,
            preview_title: "SkillsMP result".to_string(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            break;
        };
        if index == response.data.skills.len() {
            break;
        }
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
        let Some(action_index) = select_menu(MenuUi {
            title: format!("Choose an action for {}", skill.name),
            items: actions
                .iter()
                .map(|item| MenuItemUi {
                    label: (*item).to_string(),
                    preview_lines: skillsmp_installable_preview_lines(skill, &downloadable_match),
                    installed: false,
                })
                .collect(),
            default: 0,
            preview_title: skill.name.clone(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
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
    let client = SkillsMpClient::new(config)?;
    let mut messages = Vec::new();
    let mut status_message = None;
    loop {
        let skills = discover_installed_skills(directory)?
            .into_iter()
            .filter(|skill| skill.is_skillsmp())
            .collect::<Vec<_>>();
        if skills.is_empty() {
            if messages.is_empty() {
                println!(
                    "No SkillsMP-installed skills found in {}",
                    directory.display()
                );
            }
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
        let Some(index) = select_menu(MenuUi {
            title: "Select an installed SkillsMP skill".to_string(),
            items,
            default: 0,
            preview_title: "Installed skill".to_string(),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
        else {
            break;
        };
        if index == skills.len() {
            break;
        }
        let selected = skills[index].clone();
        let actions = installed_skill_actions(&selected, DELETE_CHOICE);
        let Some(action_index) = select_menu(MenuUi {
            title: format!("Choose an action for {}", skill_directory_name(&selected)),
            items: actions
                .iter()
                .map(|item| MenuItemUi {
                    label: (*item).to_string(),
                    preview_lines: installed_skill_preview_lines(&selected),
                    installed: false,
                })
                .collect(),
            default: 0,
            preview_title: skill_directory_name(&selected),
            status: status_message.clone(),
            help_text: DEFAULT_HELP_TEXT.to_string(),
            empty_preview: DEFAULT_EMPTY_PREVIEW.to_string(),
        })?
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
        let remove_name = skill_name.unwrap_or(&skill.name);
        let remove_path = directory.join(remove_name);
        if remove_path.exists() {
            fs::remove_dir_all(remove_path)?;
        }
    }
    skill.install_to(directory, skill_name, true)
}

fn update_skill(directory: &Path, skill: &SkillData, client: &SkillsMpClient) -> Result<String> {
    if skill.is_dependency() {
        let environment = ProjectEnvironment::with_paths(
            directory,
            Path::new("pyproject.toml"),
            Path::new(".venv"),
            false,
            &[],
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
        "{} [{}] [{}]",
        item.available.name,
        item.available
            .package_reference()
            .unwrap_or_else(|| "unknown".to_string()),
        scan_match_status(&item.available, item.installed.as_ref())
    )
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

fn installed_skill_actions<'a>(skill: &'a SkillData, remove_choice: &'a str) -> Vec<&'a str> {
    let mut actions = vec![remove_choice, BACK_CHOICE, EXIT_CHOICE];
    if skill.can_update() {
        actions.insert(0, UPDATE_CHOICE);
    }
    actions
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
    if item.status() == STATUS_INSTALLABLE {
        vec![INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE]
    } else {
        vec![UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
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
    if let Some(installed) = item.installed.as_ref() {
        extra.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
    }
    skill_preview_lines(&item.available, &extra)
}

fn installed_skill_preview_lines(skill: &SkillData) -> Vec<String> {
    skill_preview_lines(skill, &[])
}

fn skillsmp_search_preview_lines(
    skill: &SkillsMpSkill,
    installed: Option<&SkillData>,
) -> Vec<String> {
    let mut lines = vec![
        format!("Name: {}", skill.name),
        format!("Description: {}", skill.description),
        format!("Author: {}", skill.author),
        format!("Status: {}", skillsmp_search_status(installed)),
        format!("SkillsMP Url: {}", skill.skill_url),
        format!("GitHub Url: {}", skill.github_url),
        format!("SkillsMP Id: {}", skill.id),
    ];
    if let Some(installed) = installed {
        lines.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
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
) -> Vec<String> {
    let mut lines = skillsmp_search_preview_lines(skill, download_match.installed.as_ref());
    lines.push(format!("Resolved Status: {}", download_match.status()));
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
        MenuAction, installed_skillsmp_match, menu_action, search_skill_label,
        skillsmp_search_preview_lines, skillsmp_search_status,
    };
    use crate::client::SkillsMpSkill;
    use crate::core::{SKILLY_SOURCE_GITHUB, SKILLY_SOURCE_SKILLSMP, SkillData};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use serde_json::Value as JsonValue;
    use std::collections::BTreeMap;

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
        let preview = skillsmp_search_preview_lines(&search_result(), Some(&matched));

        assert_eq!(label, "python-production [idossha] (skill-1) [installed]");
        assert!(preview.iter().any(|line| line == "Status: installed"));
        assert!(
            preview
                .iter()
                .any(|line| line == "Installed Directory: python")
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
}

fn downloadable_skill_preview_lines(item: &DownloadableSkillMatch) -> Vec<String> {
    let mut extra = vec![format!("Status: {}", item.status())];
    if let Some(installed) = item.installed.as_ref() {
        extra.push(format!(
            "Installed Directory: {}",
            skill_directory_name(installed)
        ));
    }
    skill_preview_lines(&item.available, &extra)
}
