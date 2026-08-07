mod file_viewer;

use super::args::{
    Cli, Commands, CreateAction, ResolvedDestination, ScanDependencyArgs, next_non_empty_tab_index,
    next_selectable_index, next_tab_index, previous_non_empty_tab_index, previous_selectable_index,
    previous_tab_index,
};
use super::tui::{
    DownloadableSkillMatch, MenuAction, MenuItemStatus, MenuItemUi, TextBuffer,
    adjust_focused_on_filter, build_visible_indices, create_action, filter_matches,
    filterable_count, menu_action, menu_item_lines, menu_status_glyph, visible_position,
};
use super::update_checks::{UpdateCheckProgress, UpdateCheckRequest, UpdateCheckState};
use super::{
    APPLY_ALL_CHOICE, BACK_CHOICE, EXIT_CHOICE, INSTALL_ALL_CHOICE, INSTALL_CHOICE,
    PendingSkillUpdate, REMOVE_CHOICE, UPDATE_ALL_CHOICE, UPDATE_CHOICE, VIEW_FILES_CHOICE,
    action_menu_default, build_update_check_requests, common_repository_path,
    downloadable_skill_actions, downloadable_skill_menu_status, downloadable_skill_preview_lines,
    exit_menu_item, format_pending_update, installed_skill_actions, installed_skill_label,
    installed_skill_preview_lines, installed_skillsmp_match, listed_skill_menu_status,
    listed_skill_name, listed_skill_preview_lines_with_update_state, listed_skill_source_label,
    retained_multi_select_indices, scan_choice_label, scan_match_preview_lines, skill_source_label,
    skillsmp_search_preview_lines, skillsmp_search_source_label, update_check_status,
};
use crate::client::SkillsMpSkill;
use crate::core::{
    NamedSelection, ProjectDependencyOrigin, RepositoryProvider, SKILLY_SOURCE_DEPENDENCY,
    SKILLY_SOURCE_REPOSITORY, SkillData, SkillMatchData,
};
use clap::Parser;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Style};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn installed_skill(_ignored: Option<&str>, repository_url: Option<&str>) -> SkillData {
    SkillData {
        name: "python".to_string(),
        description: "Installed skill".to_string(),
        path: Some("/tmp/python".to_string()),
        body: "Body".to_string(),
        raw: Vec::new(),
        license: None,
        compatibility: None,
        metadata: BTreeMap::new(),
        allowed_tools: None,
        resources: Vec::new(),
        resource_warnings: Vec::new(),
        source: SKILLY_SOURCE_REPOSITORY.to_string(),
        package_name: None,
        package_version: None,
        repository_provider: repository_url.map(|_| RepositoryProvider::GitHub),
        repository_url: repository_url.map(str::to_string),
        repository_commit_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        package_ecosystem: None,
    }
}

fn dependency_match(origins: Vec<ProjectDependencyOrigin>) -> SkillMatchData {
    SkillMatchData {
        available: SkillData {
            name: "python".to_string(),
            description: "Available skill".to_string(),
            path: None,
            body: "Body".to_string(),
            raw: Vec::new(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools: None,
            resources: Vec::new(),
            resource_warnings: Vec::new(),
            source: SKILLY_SOURCE_DEPENDENCY.to_string(),
            package_name: Some("ruff".to_string()),
            package_version: Some("0.12.0".to_string()),
            repository_provider: None,
            repository_url: None,
            repository_commit_sha: None,
            package_ecosystem: None,
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
        repository_url: "https://github.com/example/project/tree/main/skills/python".to_string(),
        skill_url: "https://skillsmp.com/skills/skill-1".to_string(),
        stars: Some(42),
        updated_at: Some(JsonValue::String("1778091502".to_string())),
    }
}

#[test]
fn skillsmp_search_detects_installed_skill_by_repository_url() {
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
fn skillsmp_search_source_and_preview_include_installed_status() {
    let matched = installed_skill(
        None,
        Some("https://github.com/example/project/tree/main/skills/python"),
    );

    let preview =
        skillsmp_search_preview_lines(&search_result(), Some(&matched), Path::new("/tmp/install"));

    assert_eq!(
        skillsmp_search_source_label(&search_result()),
        "GitHub · example/project"
    );
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
        crate::core::absolute_path(Path::new("custom")).expect("relative path should resolve"),
        std::env::current_dir()
            .expect("current directory should resolve")
            .join("custom")
    );
    assert_eq!(
        crate::core::absolute_path(Path::new("~/.copilot")).expect("home path should resolve"),
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
fn update_command_accepts_yes_and_repository_token() {
    let cli = Cli::try_parse_from(["skilly", "update", "--yes", "--token", "token"])
        .expect("update command should parse");
    let Commands::Update { yes, token, .. } = cli.command else {
        panic!("expected update command");
    };

    assert!(yes);
    assert_eq!(token.as_deref(), Some("token"));
}

#[test]
fn installed_skill_actions_only_include_update_when_available() {
    assert_eq!(
        installed_skill_actions(false, REMOVE_CHOICE),
        vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
    );
    assert_eq!(
        installed_skill_actions(true, REMOVE_CHOICE),
        vec![
            VIEW_FILES_CHOICE,
            UPDATE_CHOICE,
            REMOVE_CHOICE,
            BACK_CHOICE,
            EXIT_CHOICE
        ]
    );
}

#[test]
fn update_checks_share_one_request_for_skills_from_the_same_repository() {
    let destination = ResolvedDestination {
        label: "test".to_string(),
        path: PathBuf::from("/tmp/test-skills"),
        color: Color::Blue,
    };
    let first = installed_skill(
        None,
        Some("https://github.com/example/repo/tree/main/skills/first"),
    );
    let second = SkillData {
        name: "second".to_string(),
        repository_url: Some("https://github.com/example/repo/tree/main/skills/second".to_string()),
        ..first.clone()
    };

    let requests = build_update_check_requests(
        &[destination],
        &[vec![
            super::tui::ListedSkillEntry::Valid(Box::new(first)),
            super::tui::ListedSkillEntry::Valid(Box::new(second)),
        ]],
    );

    let [UpdateCheckRequest::Repository { location, skills }] = requests.as_slice() else {
        panic!("expected one grouped repository update check");
    };
    assert_eq!(location.path, "skills");
    assert_eq!(skills.len(), 2);
}

#[test]
fn shared_repository_path_is_the_deepest_common_ancestor() {
    assert_eq!(
        common_repository_path(["skills/python", "skills/rust"].into_iter()),
        "skills"
    );
    assert_eq!(
        common_repository_path(["skills/python", "examples/rust"].into_iter()),
        "."
    );
}

#[test]
fn installed_list_uses_provenance_and_status_glyphs() {
    let installed = installed_skill(None, Some("https://github.com/example/repo"));
    let entry = super::tui::ListedSkillEntry::Valid(Box::new(installed.clone()));
    assert_eq!(listed_skill_name(&entry), "python");
    assert_eq!(
        listed_skill_source_label(&entry).as_deref(),
        Some("GitHub · example/repo")
    );
    assert_eq!(skill_source_label(&installed), "GitHub · example/repo");
    assert_eq!(menu_status_glyph(MenuItemStatus::Installable, 0), Some("+"));
    assert_eq!(menu_status_glyph(MenuItemStatus::UpToDate, 0), Some("✓"));
    assert_eq!(menu_status_glyph(MenuItemStatus::Updatable, 0), Some("↑"));
    assert_eq!(menu_status_glyph(MenuItemStatus::CheckFailed, 0), Some("!"));
    assert_ne!(
        menu_status_glyph(MenuItemStatus::Checking, 0),
        menu_status_glyph(MenuItemStatus::Checking, 1)
    );
    assert_eq!(
        listed_skill_menu_status(&entry, Some(&UpdateCheckState::Checking)),
        MenuItemStatus::Checking
    );
    assert_eq!(
        listed_skill_menu_status(
            &entry,
            Some(&UpdateCheckState::Failed("offline".to_string()))
        ),
        MenuItemStatus::CheckFailed
    );
    assert_eq!(
        listed_skill_menu_status(&entry, Some(&UpdateCheckState::Latest)),
        MenuItemStatus::UpToDate
    );
}

#[test]
fn skill_rows_render_dimmed_provenance_with_an_inline_status_glyph() {
    let item = MenuItemUi {
        label: "review".to_string(),
        subtitle: Some("GitHub · xelandernt/skilly".to_string()),
        preview_lines: Vec::new(),
        status: MenuItemStatus::Updatable,
        selectable: true,
        filter_text: Some("review".to_string()),
    };

    let lines = menu_item_lines(&item, "", Style::default(), 36, 2, 0);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content, "review");
    assert_eq!(lines[1].spans[1].style, lines[1].spans[2].style);
    assert_eq!(
        lines[1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>(),
        "  GitHub · xelandernt/skilly · ↑"
    );
}

#[test]
fn action_rows_remain_single_line() {
    let item = MenuItemUi {
        label: "view files".to_string(),
        subtitle: None,
        preview_lines: Vec::new(),
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    };

    assert_eq!(
        menu_item_lines(&item, "", Style::default(), 36, 2, 0).len(),
        1
    );
}

#[test]
fn failed_update_check_preview_is_single_line_and_actionable() {
    let entry = super::tui::ListedSkillEntry::Valid(Box::new(installed_skill(
        None,
        Some("https://github.com/example/repo"),
    )));

    let preview = listed_skill_preview_lines_with_update_state(
        &entry,
        Some(&UpdateCheckState::Failed(
            "network\nerror\u{1b}[31m".to_string(),
        )),
    );

    assert!(
        preview
            .iter()
            .any(|line| line == "Update Status: check failed")
    );
    assert!(
        preview
            .iter()
            .any(|line| line == "Update Check Error: network error [31m")
    );
}

#[test]
fn latest_update_check_preview_uses_latest_label() {
    let entry = super::tui::ListedSkillEntry::Valid(Box::new(installed_skill(
        None,
        Some("https://github.com/example/repo"),
    )));

    let preview =
        listed_skill_preview_lines_with_update_state(&entry, Some(&UpdateCheckState::Latest));

    assert!(preview.iter().any(|line| line == "Update Status: latest"));
}

#[test]
fn update_check_status_animates_progress_then_reports_summary() {
    let checking = UpdateCheckProgress {
        checked: 2,
        total: 5,
        updates: 1,
        failures: 0,
    };
    let complete = UpdateCheckProgress {
        checked: 5,
        total: 5,
        updates: 2,
        failures: 1,
    };

    assert_eq!(
        update_check_status(checking, 0, None).as_deref(),
        Some("⠋ Checking for updates... 2/5")
    );
    assert_eq!(
        update_check_status(complete, 0, None).as_deref(),
        Some("Update check complete: 2 updates available, 1 check failed")
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
fn tab_navigation_skips_empty_tabs() {
    let empty_flags = [false, true, false, true];

    assert_eq!(next_non_empty_tab_index(0, &empty_flags), 2);
    assert_eq!(next_non_empty_tab_index(2, &empty_flags), 0);
    assert_eq!(previous_non_empty_tab_index(2, &empty_flags), 0);
    assert_eq!(previous_non_empty_tab_index(0, &empty_flags), 2);
}

#[test]
fn tab_navigation_wraps_when_empty_tabs_are_allowed() {
    assert_eq!(next_tab_index(0, 4), 1);
    assert_eq!(next_tab_index(3, 4), 0);
    assert_eq!(previous_tab_index(0, 4), 3);
    assert_eq!(previous_tab_index(2, 4), 1);
}

#[test]
fn list_navigation_skips_non_selectable_invalid_entries() {
    let items = vec![
        MenuItemUi {
            label: "python".to_string(),
            subtitle: None,
            preview_lines: Vec::new(),
            status: MenuItemStatus::Default,
            selectable: true,
            filter_text: None,
        },
        MenuItemUi {
            label: ".system".to_string(),
            subtitle: None,
            preview_lines: Vec::new(),
            status: MenuItemStatus::Disabled,
            selectable: false,
            filter_text: None,
        },
        exit_menu_item("Exit list"),
    ];

    assert_eq!(next_selectable_index(&items, 0), 2);
    assert_eq!(previous_selectable_index(&items, 2), 0);
}

#[test]
fn downloadable_skill_menu_status_marks_updatable_entries() {
    let installed = installed_skill(None, Some("https://github.com/example/repo"));
    let available = SkillData {
        repository_commit_sha: Some("fedcba98765432100123456789abcdef01234567".to_string()),
        ..installed.clone()
    };
    let matched = DownloadableSkillMatch {
        available,
        installed: Some(installed),
    };

    assert_eq!(
        downloadable_skill_menu_status(&matched),
        MenuItemStatus::Updatable
    );
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
        vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
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
            repository_commit_sha: Some("fedcba98765432100123456789abcdef01234567".to_string()),
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
fn repository_skill_labels_display_their_provider_as_origin() {
    for (provider, label) in [
        (RepositoryProvider::GitHub, "github"),
        (RepositoryProvider::BitbucketCloud, "bitbucket-cloud"),
        (
            RepositoryProvider::BitbucketDataCenter,
            "bitbucket-data-center",
        ),
    ] {
        let skill = SkillData {
            repository_provider: Some(provider),
            ..installed_skill(None, Some("https://example.test/repository"))
        };

        assert_eq!(
            installed_skill_label(&skill),
            format!("python: python [{label}]")
        );
    }
}

#[test]
fn repository_skill_preview_collapses_source_and_provider() {
    let skill = installed_skill(
        None,
        Some("https://github.com/example/repo/tree/main/skills/python"),
    );

    let preview = installed_skill_preview_lines(&skill);

    assert!(
        preview
            .iter()
            .any(|line| line == "Repository: GitHub · example/repo")
    );
    assert!(!preview.iter().any(|line| line.starts_with("Source:")));
    assert!(
        !preview
            .iter()
            .any(|line| line.starts_with("Repository Provider:"))
    );
}

#[test]
fn downloadable_skill_preview_includes_absolute_destination_and_target_paths() {
    let preview = downloadable_skill_preview_lines(
        &DownloadableSkillMatch {
            available: SkillData {
                name: "python".to_string(),
                description: "Downloadable skill".to_string(),
                path: None,
                body: "Body".to_string(),
                raw: Vec::new(),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools: None,
                resources: Vec::new(),
                resource_warnings: Vec::new(),
                source: SKILLY_SOURCE_REPOSITORY.to_string(),
                package_name: None,
                package_version: None,
                repository_provider: Some(RepositoryProvider::GitHub),
                repository_url: Some(
                    "https://github.com/example/project/tree/main/skills/python".to_string(),
                ),
                repository_commit_sha: None,
                package_ecosystem: None,
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
    let selection = ScanDependencyArgs::default()
        .selection()
        .expect("default selection should succeed");

    assert!(selection.include_project_dependencies);
    assert_eq!(selection.dependency_groups, NamedSelection::All);
    assert_eq!(selection.optional_dependencies, NamedSelection::All);
}

#[test]
fn scan_dependency_args_reject_conflicting_named_filters() {
    let error = ScanDependencyArgs {
        groups: vec!["dev".to_string()],
        exclude_groups: vec!["docs".to_string()],
        ..ScanDependencyArgs::default()
    }
    .selection()
    .expect_err("conflicting group filters should fail");

    assert!(
        error
            .to_string()
            .contains("Include and exclude filters cannot be combined")
    );
}

#[test]
fn scan_choice_label_and_preview_include_dependency_origins() {
    let item = dependency_match(vec![
        ProjectDependencyOrigin::python_project(),
        ProjectDependencyOrigin::python_dependency_group("dev"),
        ProjectDependencyOrigin::python_optional_dependency("docs"),
    ]);

    let label = scan_choice_label(&item);
    let preview = scan_match_preview_lines(&item);

    assert_eq!(
        label,
        "python [ruff==0.12.0] [python:project, python:group:dev, python:extra:docs] [installable]"
    );
    assert!(
        preview.iter().any(|line| line
            == "Dependency Sources: python:project, python:group:dev, python:extra:docs")
    );
    assert!(
        preview
            .iter()
            .any(|line| line == "  - python dependency group: dev")
    );
    assert!(
        preview
            .iter()
            .any(|line| line == "  - python optional dependency: docs")
    );
}
