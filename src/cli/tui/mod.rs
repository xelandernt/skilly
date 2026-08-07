mod configure;
mod create;
mod file_viewer;
mod input;
mod loading;
mod menu;

pub(crate) use configure::run_configure_tui;
pub(crate) use create::{
    CreateFormState, TextBuffer, is_interactive_terminal, parse_metadata, run_create_tui,
};
pub(crate) use file_viewer::run_file_viewer;
pub(crate) use input::{
    char_to_byte_index, create_action, create_field_label, crop_text, empty_to_none, line_len,
    on_off, placeholder_if_empty, requested_directories, summarize_multiline, summarize_text,
};
pub(crate) use loading::show_loading_message;
pub(crate) use menu::{
    menu_item_style, multi_select_menu, multi_select_menu_with_tick, render_menu_tabs, select_menu,
    select_menu_with_tick,
};

#[cfg(test)]
pub(crate) use file_viewer::{
    build_file_tree, compute_filtered_visible, compute_visible, file_viewer_move_selection_down,
    file_viewer_move_selection_up,
};
#[cfg(test)]
pub(crate) use menu::{
    adjust_focused_on_filter, build_visible_indices, filter_matches, filterable_count, menu_action,
    menu_item_lines, menu_status_glyph, visible_position,
};

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
    pub(crate) subtitle: Option<String>,
    pub(crate) preview_lines: Vec<String>,
    pub(crate) status: MenuItemStatus,
    pub(crate) selectable: bool,
    pub(crate) filter_text: Option<String>,
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
    Installable,
    Installed,
    Checking,
    UpToDate,
    Updatable,
    CheckFailed,
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
            Some(installed)
                if crate::core::repository_versions_match(installed, &self.available) =>
            {
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
