use crate::config::SkillyConfig;
use crate::core::{
    NamedSelection, RepositoryProvider, ScanDependencySelection, SkillDirectoryFlavor,
    absolute_path, resolve_default_directory, skills_directory,
};
use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use ratatui::style::Color;
use std::path::{Path, PathBuf};

use super::tui::{MenuItemUi, MenuTabUi};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DestinationScope {
    Local,
    Global,
}

impl From<bool> for DestinationScope {
    fn from(global: bool) -> Self {
        if global { Self::Global } else { Self::Local }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedDestination {
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) color: Color,
}

#[derive(Parser, Debug)]
#[command(name = "skilly", version, about = "Manage agent skills.")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
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
    #[command(
        about = "Scan dependency-provided skills from pyproject.toml/.venv, package.json/node_modules, and pom.xml."
    )]
    Scan {
        #[command(flatten)]
        destination: DestinationArgs,
        #[command(flatten)]
        dependencies: ScanDependencyArgs,
    },
    #[command(about = "Download one or more skills from a supported repository URL.")]
    Download {
        #[arg(help = "Repository, revision, or skill URL to download from.")]
        repository_url: String,
        #[arg(
            long,
            value_name = "PROVIDER",
            help = "Repository provider: github, bitbucket-cloud, or bitbucket-data-center. GitHub and Bitbucket Cloud URLs are auto-detected; Data Center requires this option."
        )]
        provider: Option<RepositoryProvider>,
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            help = "Select a specific skill when the repository URL resolves to multiple skills."
        )]
        skill_name: Option<String>,
        #[arg(long, help = "Download every skill found at the repository URL.")]
        all: bool,
        #[arg(
            long,
            help = "Overwrite existing files when installing the downloaded skill."
        )]
        overwrite: bool,
        #[arg(
            long,
            value_name = "TOKEN",
            help = "Token for the selected or detected repository provider."
        )]
        token: Option<String>,
    },
    #[command(about = "Browse, update, or remove installed skills.")]
    List {
        #[command(flatten)]
        destination: DestinationArgs,
        #[arg(
            long,
            value_name = "TOKEN",
            help = "Token used when checking repository-backed skill updates."
        )]
        token: Option<String>,
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
            value_name = "TOKEN",
            help = "Token used when checking repository-backed skill updates."
        )]
        token: Option<String>,
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
    #[command(about = "Configure directories and repository credentials.")]
    Configure {
        #[arg(
            long,
            short = 's',
            alias = "list",
            help = "Print the current configuration as TOML and exit."
        )]
        show: bool,
        #[arg(
            long,
            help = "Restore the default configuration (agents global and local directories only)."
        )]
        reset: bool,
        #[arg(
            long,
            value_name = "PATH",
            help = "Add an absolute path (or ~-prefixed) as a custom global directory."
        )]
        add_global: Vec<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Remove a custom global directory by its path."
        )]
        remove_global: Vec<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Add a relative path as a custom local directory."
        )]
        add_local: Vec<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Remove a custom local directory by its path."
        )]
        remove_local: Vec<String>,
        #[arg(
            long,
            value_name = "PROVIDER",
            help = "Store or replace a credential for github, bitbucket-cloud, or bitbucket-data-center. Requires --provider-url and --provider-token."
        )]
        add_provider: Option<RepositoryProvider>,
        #[arg(
            long,
            value_name = "URL",
            help = "Repository provider base URL used with --add-provider or --remove-provider."
        )]
        provider_url: Option<String>,
        #[arg(
            long,
            value_name = "TOKEN",
            help = "Repository token stored with --add-provider."
        )]
        provider_token: Option<String>,
        #[arg(
            long,
            value_name = "PROVIDER",
            help = "Remove a stored provider credential. Requires --provider-url."
        )]
        remove_provider: Option<RepositoryProvider>,
    },
}

#[derive(Args, Debug)]
pub(crate) struct SkillsMpCommands {
    #[command(subcommand)]
    pub(crate) command: SkillsMpSubcommand,
}

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct ScanDependencyArgs {
    #[arg(long, help = "Ignore [project].dependencies while scanning.")]
    pub(crate) no_project_dependencies: bool,
    #[arg(
        long = "group",
        help = "Include only the named [dependency-groups] entry.",
        value_name = "NAME"
    )]
    pub(crate) groups: Vec<String>,
    #[arg(
        long = "exclude-group",
        help = "Exclude the named [dependency-groups] entry.",
        value_name = "NAME"
    )]
    pub(crate) exclude_groups: Vec<String>,
    #[arg(
        long = "extra",
        help = "Include only the named [project.optional-dependencies] extra.",
        value_name = "NAME"
    )]
    pub(crate) extras: Vec<String>,
    #[arg(
        long = "exclude-extra",
        help = "Exclude the named [project.optional-dependencies] extra.",
        value_name = "NAME"
    )]
    pub(crate) exclude_extras: Vec<String>,
}

impl ScanDependencyArgs {
    pub(crate) fn selection(self) -> Result<ScanDependencySelection> {
        Ok(ScanDependencySelection {
            include_project_dependencies: !self.no_project_dependencies,
            dependency_groups: NamedSelection::new(
                (!self.groups.is_empty()).then_some(self.groups),
                (!self.exclude_groups.is_empty()).then_some(self.exclude_groups),
            )?,
            optional_dependencies: NamedSelection::new(
                (!self.extras.is_empty()).then_some(self.extras),
                (!self.exclude_extras.is_empty()).then_some(self.exclude_extras),
            )?,
            include_node_dependencies: true,
            include_node_dev_dependencies: true,
            include_node_optional_dependencies: true,
        })
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum SkillsMpSubcommand {
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
    },
}

#[derive(Args, Debug, Default)]
pub(crate) struct DestinationArgs {
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
    fn validate(&self) -> Result<()> {
        if usize::from(self.local) + usize::from(self.global) > 1 {
            bail!("use either --local or --global");
        }
        if usize::from(self.claude) + usize::from(self.codex) + usize::from(self.copilot) > 1 {
            bail!("use only one of --claude, --codex, or --copilot");
        }
        Ok(())
    }

    pub(crate) fn resolve(&self) -> Result<PathBuf> {
        if let Some(directory) = self.directory.as_ref() {
            return absolute_path(directory);
        }
        self.validate()?;
        let flavor = if self.claude {
            SkillDirectoryFlavor::Claude
        } else if self.codex {
            SkillDirectoryFlavor::Codex
        } else if self.copilot {
            SkillDirectoryFlavor::Copilot
        } else {
            return crate::core::default_skills_directory();
        };
        absolute_path(&skills_directory(flavor, self.global)?)
    }

    pub(crate) fn resolve_interactive_destinations(
        &self,
        config: &SkillyConfig,
    ) -> Result<Vec<ResolvedDestination>> {
        if let Some(directory) = self.directory.as_ref() {
            return Ok(vec![ResolvedDestination {
                label: "custom".to_string(),
                path: absolute_path(directory)?,
                color: Color::White,
            }]);
        }
        self.validate()?;
        if self.local || self.global || self.claude || self.codex || self.copilot {
            return Ok(vec![resolved_destination(
                self.selected_flavor(),
                self.selected_scope(),
            )?]);
        }
        all_interactive_destinations(config)
    }

    fn selected_flavor(&self) -> SkillDirectoryFlavor {
        if self.claude {
            SkillDirectoryFlavor::Claude
        } else if self.codex {
            SkillDirectoryFlavor::Codex
        } else if self.copilot {
            SkillDirectoryFlavor::Copilot
        } else {
            SkillDirectoryFlavor::Agents
        }
    }

    fn selected_scope(&self) -> DestinationScope {
        if self.global {
            DestinationScope::Global
        } else {
            DestinationScope::Local
        }
    }
}

pub(crate) fn all_interactive_destinations(
    config: &SkillyConfig,
) -> Result<Vec<ResolvedDestination>> {
    let effective_default = resolve_default_directory(Some(&config.default_directory));
    let mut destinations = Vec::new();
    let mut default_index = None;
    for (i, dir) in config.global.directories.iter().enumerate() {
        let dest = destination_from_dir(dir, true)?;
        if dir == &effective_default && default_index.is_none() {
            default_index = Some(i);
        }
        destinations.push(dest);
    }
    for dir in config.local.directories.iter() {
        if dir == &effective_default && default_index.is_none() {
            default_index = Some(destinations.len());
        }
        destinations.push(destination_from_dir(dir, false)?);
    }
    if let Some(pos) = default_index
        && pos > 0
    {
        let default = destinations.remove(pos);
        destinations.insert(0, default);
    }
    Ok(destinations)
}

/// Create a [`ResolvedDestination`] from a directory path, detecting the
/// flavor and computing a human-readable label with appropriate color.
pub(crate) fn destination_from_dir(path: &str, global: bool) -> Result<ResolvedDestination> {
    let flavor = detect_flavor_from_path(path);
    let label = if let Some(flavor) = flavor {
        let name = match flavor {
            SkillDirectoryFlavor::Agents => "agents",
            SkillDirectoryFlavor::Claude => "claude",
            SkillDirectoryFlavor::Codex => "codex",
            SkillDirectoryFlavor::Copilot => "copilot",
        };
        format!(
            "{name} {scope}",
            scope = if global { "global" } else { "local" }
        )
    } else {
        let basename = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        format!(
            "{basename} ({scope})",
            scope = if global { "global" } else { "local" }
        )
    };
    let color = match flavor {
        Some(SkillDirectoryFlavor::Agents) => Color::Green,
        Some(SkillDirectoryFlavor::Claude) => Color::Yellow,
        Some(SkillDirectoryFlavor::Codex) => Color::Cyan,
        Some(SkillDirectoryFlavor::Copilot) => Color::Blue,
        None => Color::White,
    };
    let resolved = if global {
        absolute_path(Path::new(path))?
    } else {
        PathBuf::from(path)
    };
    Ok(ResolvedDestination {
        label,
        path: resolved,
        color,
    })
}

/// Heuristically detect the [`SkillDirectoryFlavor`] from a path string.
pub(crate) fn detect_flavor_from_path(path: &str) -> Option<SkillDirectoryFlavor> {
    let normalized = path.trim_start_matches("~/");
    if normalized.contains(".agents/")
        || normalized.ends_with(".agents/skills")
        || normalized == ".agents/skills"
    {
        Some(SkillDirectoryFlavor::Agents)
    } else if normalized.contains(".claude/") || normalized.ends_with(".claude/skills") {
        Some(SkillDirectoryFlavor::Claude)
    } else if normalized.contains(".codex/") || normalized.ends_with(".codex/skills") {
        Some(SkillDirectoryFlavor::Codex)
    } else if normalized.contains(".copilot/")
        || normalized.ends_with(".copilot/skills")
        || normalized.ends_with(".github/skills")
        || normalized.contains(".github/skills")
    {
        Some(SkillDirectoryFlavor::Copilot)
    } else {
        None
    }
}

fn resolved_destination(
    flavor: SkillDirectoryFlavor,
    scope: DestinationScope,
) -> Result<ResolvedDestination> {
    let global = scope == DestinationScope::Global;
    let label = match (flavor, scope) {
        (SkillDirectoryFlavor::Agents, DestinationScope::Local) => "agents local",
        (SkillDirectoryFlavor::Agents, DestinationScope::Global) => "agents global",
        (SkillDirectoryFlavor::Claude, DestinationScope::Local) => "claude local",
        (SkillDirectoryFlavor::Claude, DestinationScope::Global) => "claude global",
        (SkillDirectoryFlavor::Codex, DestinationScope::Local) => "codex local",
        (SkillDirectoryFlavor::Codex, DestinationScope::Global) => "codex global",
        (SkillDirectoryFlavor::Copilot, DestinationScope::Local) => "copilot local",
        (SkillDirectoryFlavor::Copilot, DestinationScope::Global) => "copilot global",
    }
    .to_string();
    let color = match flavor {
        SkillDirectoryFlavor::Agents => Color::Green,
        SkillDirectoryFlavor::Claude => Color::Yellow,
        SkillDirectoryFlavor::Codex => Color::Cyan,
        SkillDirectoryFlavor::Copilot => Color::Blue,
    };
    Ok(ResolvedDestination {
        label,
        path: absolute_path(&skills_directory(flavor, global)?)?,
        color,
    })
}

pub(crate) fn next_tab_index(current: usize, len: usize) -> usize {
    if len <= 1 {
        return current;
    }
    (current + 1) % len
}

pub(crate) fn next_non_empty_tab_index(current: usize, empty_flags: &[bool]) -> usize {
    let len = empty_flags.len();
    if len <= 1 {
        return current;
    }
    for offset in 1..=len {
        let candidate = (current + offset) % len;
        if !empty_flags.get(candidate).copied().unwrap_or(false) {
            return candidate;
        }
    }
    current
}

pub(crate) fn previous_tab_index(current: usize, len: usize) -> usize {
    if len <= 1 {
        return current;
    }
    (current + len - 1) % len
}

pub(crate) fn previous_non_empty_tab_index(current: usize, empty_flags: &[bool]) -> usize {
    let len = empty_flags.len();
    if len <= 1 {
        return current;
    }
    for offset in 1..=len {
        let candidate = (current + len - offset) % len;
        if !empty_flags.get(candidate).copied().unwrap_or(false) {
            return candidate;
        }
    }
    current
}

pub(crate) fn destination_tabs(
    destinations: &[ResolvedDestination],
    empty_flags: &[bool],
) -> Vec<MenuTabUi> {
    destinations
        .iter()
        .enumerate()
        .map(|(index, destination)| MenuTabUi {
            label: destination.label.clone(),
            color: destination.color,
            dimmed: empty_flags.get(index).copied().unwrap_or(false),
        })
        .collect()
}

pub(crate) fn first_non_empty_tab(empty_flags: &[bool]) -> usize {
    empty_flags
        .iter()
        .position(|is_empty| !is_empty)
        .unwrap_or(0)
}

pub(crate) fn first_selectable_index(items: &[MenuItemUi]) -> usize {
    items.iter().position(|item| item.selectable).unwrap_or(0)
}

pub(crate) fn next_selectable_index(items: &[MenuItemUi], current: usize) -> usize {
    let start = current.saturating_add(1);
    items
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, item)| item.selectable.then_some(index))
        .unwrap_or(current)
}

pub(crate) fn previous_selectable_index(items: &[MenuItemUi], current: usize) -> usize {
    items
        .iter()
        .enumerate()
        .take(current)
        .rev()
        .find_map(|(index, item)| item.selectable.then_some(index))
        .unwrap_or(current)
}

#[derive(Args, Debug)]
pub(crate) struct UtilCommands {
    #[command(subcommand)]
    pub(crate) command: UtilSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum UtilSubcommand {
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

#[derive(Debug)]
pub(crate) struct CreateOptions {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) license: Option<String>,
    pub(crate) compatibility: Option<String>,
    pub(crate) metadata: Vec<String>,
    pub(crate) allowed_tools: Option<String>,
    pub(crate) with_scripts: bool,
    pub(crate) with_references: bool,
    pub(crate) with_assets: bool,
    pub(crate) overwrite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateField {
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

pub(crate) const CREATE_FIELDS: [CreateField; 11] = [
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
pub(crate) enum CreateAction {
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
