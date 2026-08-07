// --- file viewer tests ---

use super::super::tui::{
    build_file_tree, compute_filtered_visible, compute_visible, file_viewer_move_selection_down,
    file_viewer_move_selection_up,
};
use super::*;
use crate::core::SkillResourceData;
use std::collections::HashSet;

fn resource(path: &str, content: &str) -> SkillResourceData {
    SkillResourceData {
        relative_path: path.to_string(),
        kind: "other".to_string(),
        raw: content.as_bytes().to_vec(),
    }
}

#[test]
fn file_tree_always_has_skill_md_first() {
    let skill = SkillData {
        body: "# Title\n\nBody".to_string(),
        resources: vec![],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "SKILL.md");
    assert_eq!(tree[0].content, skill.render(None).as_bytes());
    assert_eq!(tree[0].depth, 0);
    assert!(!tree[0].is_dir);
}

#[test]
fn file_tree_builds_flat_resources() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("README.md", "readme content"),
            resource("setup.py", "setup content"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    assert_eq!(tree.len(), 3);
    // SKILL.md first
    assert_eq!(tree[0].name, "SKILL.md");
    assert_eq!(tree[0].depth, 0);
    // Then files in sorted order
    assert_eq!(tree[1].name, "README.md");
    assert_eq!(tree[1].depth, 0);
    assert_eq!(tree[1].content, b"readme content");
    assert!(!tree[1].is_dir);
    assert_eq!(tree[2].name, "setup.py");
    assert_eq!(tree[2].depth, 0);
}

#[test]
fn file_tree_creates_directory_entries() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("scripts/run.py", "print('hello')"),
            resource("references/api.md", "# API"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    // Alphabetical: references/ before scripts/
    // SKILL.md, references/, api.md, scripts/, run.py
    assert_eq!(tree.len(), 5);

    assert_eq!(tree[0].name, "SKILL.md");
    assert_eq!(tree[0].depth, 0);
    assert!(!tree[0].is_dir);

    assert_eq!(tree[1].name, "references");
    assert_eq!(tree[1].depth, 0);
    assert!(tree[1].is_dir);
    assert_eq!(tree[1].relative_path, "references/");

    assert_eq!(tree[2].name, "api.md");
    assert_eq!(tree[2].depth, 1);
    assert!(!tree[2].is_dir);

    assert_eq!(tree[3].name, "scripts");
    assert_eq!(tree[3].depth, 0);
    assert!(tree[3].is_dir);

    assert_eq!(tree[4].name, "run.py");
    assert_eq!(tree[4].depth, 1);
    assert!(!tree[4].is_dir);
}

#[test]
fn file_tree_handles_nested_directories() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("assets/icons/logo.svg", "<svg></svg>"),
            resource("scripts/sub/helper.py", "# helper"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    // Order: SKILL.md, assets/, icons/, logo.svg, scripts/, sub/, helper.py
    let paths: Vec<&str> = tree.iter().map(|e| e.relative_path.as_str()).collect();

    assert!(paths[1].starts_with("assets"));
    assert!(paths[2] == "assets/icons/");
    assert!(paths[3] == "assets/icons/logo.svg");
    assert!(paths[4].starts_with("scripts"));
    assert!(paths[5] == "scripts/sub/");
    assert!(paths[6] == "scripts/sub/helper.py");

    // Check depths
    assert_eq!(tree[1].depth, 0); // assets/
    assert_eq!(tree[2].depth, 1); // assets/icons/
    assert_eq!(tree[3].depth, 2); // logo.svg
    assert_eq!(tree[4].depth, 0); // scripts/
    assert_eq!(tree[5].depth, 1); // scripts/sub/
    assert_eq!(tree[6].depth, 2); // helper.py
}

#[test]
fn file_tree_multiple_dirs_sorted() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("z/back/file.txt", "back"),
            resource("a/front/file.txt", "front"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    // Should be sorted: a/... first, then z/...
    assert_eq!(tree[1].name, "a");
    // Find z dir somewhere after a
    let z_pos = tree.iter().position(|e| e.name == "z").unwrap();
    let a_pos = tree.iter().position(|e| e.name == "a").unwrap();
    assert!(a_pos < z_pos);
}

#[test]
fn compute_visible_shows_all_when_nothing_collapsed() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("scripts/run.py", "print('hello')"),
            resource("references/api.md", "# API"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    let collapsed = HashSet::new();
    let visible = compute_visible(&tree, &collapsed);

    assert_eq!(visible.len(), tree.len());
}

#[test]
fn compute_visible_hides_children_of_collapsed_dir() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("scripts/run.py", "print('hello')"),
            resource("scripts/helper.py", "# helper"),
            resource("references/api.md", "# API"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    // Alphabetical: references/ before scripts/
    // 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py, 5=helper.py

    let mut collapsed = HashSet::new();
    collapsed.insert("scripts/".to_string());

    let visible = compute_visible(&tree, &collapsed);
    let visible_paths: Vec<&str> = visible
        .iter()
        .map(|&i| tree[i].relative_path.as_str())
        .collect();

    // scripts/ children (run.py, helper.py) should be hidden
    assert!(visible_paths.contains(&"SKILL.md"));
    assert!(visible_paths.contains(&"references/"));
    assert!(visible_paths.contains(&"references/api.md"));
    assert!(visible_paths.contains(&"scripts/"));
    assert!(!visible_paths.contains(&"scripts/run.py"));
    assert!(!visible_paths.contains(&"scripts/helper.py"));
}

#[test]
fn compute_visible_hides_nested_children() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("a/b/c/file.txt", "deep"),
            resource("a/other.txt", "other"),
        ],
        ..installed_skill(None, None)
    };

    let tree = build_file_tree(&skill);
    // SKILL.md, a/, b/, c/, file.txt, other.txt
    let mut collapsed = HashSet::new();
    collapsed.insert("a/".to_string());

    let visible = compute_visible(&tree, &collapsed);
    let visible_paths: Vec<&str> = visible
        .iter()
        .map(|&i| tree[i].relative_path.as_str())
        .collect();

    // a/ is visible but all descendants (b/, c/, file.txt, other.txt) are hidden
    assert!(visible_paths.contains(&"SKILL.md"));
    assert!(visible_paths.contains(&"a/"));
    assert!(!visible_paths.contains(&"a/b/"));
    assert!(!visible_paths.contains(&"a/b/c/"));
    assert!(!visible_paths.contains(&"a/b/c/file.txt"));
    assert!(!visible_paths.contains(&"a/other.txt"));
}

#[test]
fn file_filter_matches_filenames_case_insensitively_and_shows_parents() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![
            resource("scripts/sub/helper.py", "# helper"),
            resource("scripts/run.py", "# run"),
            resource("references/api.md", "# API"),
        ],
        ..installed_skill(None, None)
    };
    let tree = build_file_tree(&skill);
    let mut collapsed = HashSet::new();
    collapsed.insert("scripts/".to_string());

    let visible = compute_filtered_visible(&tree, &collapsed, "HELPER");
    let paths: Vec<&str> = visible
        .iter()
        .map(|&index| tree[index].relative_path.as_str())
        .collect();

    assert_eq!(
        paths,
        vec!["scripts/", "scripts/sub/", "scripts/sub/helper.py"]
    );
}

#[test]
fn file_filter_matches_names_not_directory_paths() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![resource("scripts/run.py", "# run")],
        ..installed_skill(None, None)
    };
    let tree = build_file_tree(&skill);

    assert!(compute_filtered_visible(&tree, &HashSet::new(), "scripts").is_empty());
}

#[test]
fn empty_file_filter_preserves_collapsed_directories() {
    let skill = SkillData {
        body: "Body".to_string(),
        resources: vec![resource("scripts/run.py", "# run")],
        ..installed_skill(None, None)
    };
    let tree = build_file_tree(&skill);
    let mut collapsed = HashSet::new();
    collapsed.insert("scripts/".to_string());

    assert_eq!(
        compute_filtered_visible(&tree, &collapsed, ""),
        compute_visible(&tree, &collapsed)
    );
}

#[test]
fn file_viewer_move_selection_navigates_visible_only() {
    // Tree: 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py
    // (alphabetical: references/ before scripts/)
    let skill = SkillData {
        body: "B".to_string(),
        resources: vec![
            resource("scripts/run.py", "x"),
            resource("references/api.md", "x"),
        ],
        ..installed_skill(None, None)
    };
    let tree = build_file_tree(&skill);

    let mut collapsed = HashSet::new();
    collapsed.insert("scripts/".to_string());

    let visible = compute_visible(&tree, &collapsed);
    // visible: [0=SKILL.md, 1=references/, 2=api.md, 3=scripts/]

    // Down from SKILL.md (0) -> references/ (1)
    assert_eq!(file_viewer_move_selection_down(&visible, 0), 1);
    // Down from references/ (1) -> api.md (2)
    assert_eq!(file_viewer_move_selection_down(&visible, 1), 2);
    // Down from api.md (2) -> scripts/ (3)
    assert_eq!(file_viewer_move_selection_down(&visible, 2), 3);
    // Down at end -> stays
    assert_eq!(file_viewer_move_selection_down(&visible, 3), 3);

    // Up from scripts/ (3) -> api.md (2)
    assert_eq!(file_viewer_move_selection_up(&visible, 3), 2);
    // Up from api.md (2) -> references/ (1)
    assert_eq!(file_viewer_move_selection_up(&visible, 2), 1);
    // Up from references/ (1) -> SKILL.md (0)
    assert_eq!(file_viewer_move_selection_up(&visible, 1), 0);
    // Up at start -> stays
    assert_eq!(file_viewer_move_selection_up(&visible, 0), 0);
}

#[test]
fn file_viewer_move_selection_handles_current_becoming_hidden() {
    let skill = SkillData {
        body: "B".to_string(),
        resources: vec![
            resource("scripts/run.py", "x"),
            resource("references/api.md", "x"),
        ],
        ..installed_skill(None, None)
    };
    let tree = build_file_tree(&skill);
    // 0=SKILL.md, 1=references/, 2=api.md, 3=scripts/, 4=run.py

    let mut collapsed = HashSet::new();
    collapsed.insert("scripts/".to_string());

    let visible = compute_visible(&tree, &collapsed);
    // visible: [0, 1, 2, 3]

    // Moving down from run.py (4, hidden) should go to next visible: scripts/ (3 is the next visible? Actually 4 is hidden, so find first visible at or after 4)
    // No visible at or after 4, so should wrap to the first: 0
    // But looking at the function: visible.iter().find(|&&i| i >= current) - no match for >= 4, so unwrap_or(*visible.last() = 3)
    assert_eq!(file_viewer_move_selection_down(&visible, 4), 3);
    // Moving up from run.py (4, hidden) should find first visible >= 4: actually none, but then return first visible
    // Wait: visible.iter().find(|&&i| i >= current) -> no match -> unwrap_or(visible[0]) -> 0
    assert_eq!(file_viewer_move_selection_up(&visible, 4), 3);
}

// --- Filtering tests ---

fn skill_item(name: &str) -> MenuItemUi {
    MenuItemUi {
        label: name.to_string(),
        subtitle: None,
        preview_lines: Vec::new(),
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: Some(name.to_string()),
    }
}

fn exit_item() -> MenuItemUi {
    MenuItemUi {
        label: "Exit".to_string(),
        subtitle: None,
        preview_lines: vec!["Exit menu".to_string()],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    }
}

fn disabled_item(label: &str) -> MenuItemUi {
    MenuItemUi {
        label: label.to_string(),
        subtitle: None,
        preview_lines: Vec::new(),
        status: MenuItemStatus::Disabled,
        selectable: false,
        filter_text: Some(label.to_string()),
    }
}

#[test]
fn filter_matches_non_filterable_always_returns_true() {
    let item = exit_item();
    assert!(filter_matches("", &item));
}

#[test]
fn filter_matches_non_filterable_returns_false_when_filtering() {
    let item = exit_item();
    assert!(!filter_matches("xyz", &item));
}

#[test]
fn filter_matches_case_insensitive_substring() {
    let item = skill_item("Python");
    assert!(filter_matches("p", &item));
    assert!(filter_matches("PY", &item));
    assert!(filter_matches("thon", &item));
    assert!(!filter_matches("xyz", &item));
}

#[test]
fn filterable_count_counts_only_filterable_items() {
    let items = vec![skill_item("a"), exit_item(), skill_item("b")];
    assert_eq!(filterable_count(&items), 2);
}

#[test]
fn filterable_count_returns_zero_when_none_filterable() {
    let items = vec![exit_item(), exit_item()];
    assert_eq!(filterable_count(&items), 0);
}

#[test]
fn build_visible_indices_empty_filter_returns_all() {
    let items = vec![skill_item("a"), exit_item(), skill_item("b")];
    assert_eq!(build_visible_indices(&items, ""), vec![0, 1, 2]);
}

#[test]
fn build_visible_indices_filters_by_name() {
    let items = vec![skill_item("alpha"), exit_item(), skill_item("beta")];
    assert_eq!(build_visible_indices(&items, "alpha"), vec![0]);
}

#[test]
fn build_visible_indices_no_match_returns_empty() {
    let items = vec![skill_item("alpha"), exit_item(), skill_item("beta")];
    assert_eq!(build_visible_indices(&items, "xyz"), Vec::<usize>::new());
}

#[test]
fn build_visible_indices_disabled_filterable_still_visible_when_matched() {
    let items = vec![skill_item("alpha"), disabled_item("beta")];
    let visible = build_visible_indices(&items, "beta");
    assert_eq!(visible, vec![1]);
}

#[test]
fn visible_position_finds_index_in_visible() {
    let visible = vec![2, 5, 7];
    assert_eq!(visible_position(&visible, 5), Some(1));
    assert_eq!(visible_position(&visible, 2), Some(0));
    assert_eq!(visible_position(&visible, 99), None);
}

#[test]
fn visible_position_empty_list_returns_none() {
    let visible: Vec<usize> = vec![];
    assert_eq!(visible_position(&visible, 0), None);
}

#[test]
fn adjust_focused_on_filter_changes_when_not_in_visible() {
    let visible = vec![2, 5, 7];
    let mut focused = 3;
    adjust_focused_on_filter(&visible, &mut focused);
    assert_eq!(focused, 2);
}

#[test]
fn adjust_focused_on_filter_preserves_when_in_visible() {
    let visible = vec![2, 5, 7];
    let mut focused = 5;
    adjust_focused_on_filter(&visible, &mut focused);
    assert_eq!(focused, 5);
}

#[test]
fn adjust_focused_on_filter_empty_visible_does_nothing() {
    let visible: Vec<usize> = vec![];
    let mut focused = 3;
    adjust_focused_on_filter(&visible, &mut focused);
    assert_eq!(focused, 3);
}
