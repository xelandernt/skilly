use super::*;

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
