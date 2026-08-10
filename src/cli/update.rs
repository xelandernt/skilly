use super::*;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

#[derive(Debug, Clone)]
pub(super) struct PendingSkillUpdate {
    pub(super) directory: PathBuf,
    pub(super) installed: SkillData,
    pub(super) available: SkillData,
}

pub(super) fn run_update(
    destinations: &[ResolvedDestination],
    config: ClientConfig,
    yes: bool,
) -> Result<()> {
    if destinations.is_empty() {
        println!("{CONFIGURE_HINT}");
        return Ok(());
    }
    let client = Arc::new(SkillsMpClient::new(config)?);
    let installed_by_destination = destinations
        .iter()
        .map(|destination| {
            discover_installed_skills_report(&destination.path).map(|report| report.valid_skills)
        })
        .collect::<Result<Vec<_>>>()?;
    let entries_by_destination = installed_by_destination
        .iter()
        .map(|installed_skills| {
            installed_skills
                .iter()
                .cloned()
                .map(|skill| ListedSkillEntry::Valid(Box::new(skill)))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let requests = build_update_check_requests(destinations, &entries_by_destination);
    let checks = UpdateChecks::start(requests, Arc::clone(&client));
    while checks.progress().is_checking() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut updates = Vec::new();
    let mut errors = Vec::new();
    for (destination, installed_skills) in destinations.iter().zip(installed_by_destination) {
        for installed in installed_skills {
            match checks.state(&UpdateCheckKey::new(&destination.path, &installed)) {
                Some(UpdateCheckState::Updatable(available)) => updates.push(PendingSkillUpdate {
                    directory: destination.path.clone(),
                    installed,
                    available: *available,
                }),
                Some(UpdateCheckState::Failed(error)) => errors.push(format!(
                    "Could not check updates for {} in {}: {error}",
                    skill_directory_name(&installed),
                    destination.path.display(),
                )),
                Some(UpdateCheckState::Checking | UpdateCheckState::Latest) | None => {}
            }
        }
    }

    updates.sort_by(|left, right| {
        left.directory.cmp(&right.directory).then_with(|| {
            skill_directory_name(&left.installed).cmp(&skill_directory_name(&right.installed))
        })
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
        println!(
            "{} ({})",
            format_pending_update(update),
            update.directory.display()
        );
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
        match install_available_skill(
            &update.directory,
            &update.available,
            Some(&skill_directory_name(&update.installed)),
            true,
        ) {
            Ok(updated) => println!(
                "{} ({})",
                format_applied_update(&update.installed, &updated),
                update.directory.display()
            ),
            Err(error) => errors.push(format!(
                "Could not update {} in {}: {error}",
                skill_directory_name(&update.installed),
                update.directory.display(),
            )),
        }
    }
    for error in errors {
        println!("{error}");
    }
    Ok(())
}

pub(super) fn confirm_apply_updates() -> Result<bool> {
    print!("Apply these updates? [y/N] ");
    io::stdout().flush()?;
    let _raw_mode = RawModeGuard::enable()?;
    let mut answer = String::new();
    loop {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if let Some(confirmed) = confirmation_key_result(key, &answer) {
            print!("\r\n");
            io::stdout().flush()?;
            return Ok(confirmed);
        }
        match key.code {
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                answer.push(character);
                print!("{character}");
                io::stdout().flush()?;
            }
            KeyCode::Backspace if answer.pop().is_some() => {
                print!("\u{8} \u{8}");
                io::stdout().flush()?;
            }
            _ => {}
        }
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

pub(super) fn confirmation_key_result(key: KeyEvent, answer: &str) -> Option<bool> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(false),
        KeyCode::Esc => Some(false),
        KeyCode::Enter => {
            let normalized = answer.trim().to_ascii_lowercase();
            Some(normalized == "y" || normalized == "yes")
        }
        _ => None,
    }
}

pub(super) fn format_pending_update(update: &PendingSkillUpdate) -> String {
    format!(
        "{} [{}]: {}",
        skill_directory_name(&update.installed),
        if update.installed.is_dependency() {
            "dependency"
        } else if let Some(provider) = update.installed.repository_provider {
            provider.as_str()
        } else {
            "unknown"
        },
        format_update_transition(&update.installed, &update.available)
    )
}

pub(super) fn format_update_transition(installed: &SkillData, available: &SkillData) -> String {
    if installed.is_dependency() {
        return format!(
            "{} {} -> {}",
            available.package_name.as_deref().unwrap_or("unknown"),
            installed.package_version.as_deref().unwrap_or("unknown"),
            available.package_version.as_deref().unwrap_or("unknown")
        );
    }

    format!(
        "{} -> {}",
        short_revision(installed.repository_commit_sha.as_deref()),
        short_revision(available.repository_commit_sha.as_deref())
    )
}

pub(super) fn format_applied_update(previous: &SkillData, updated: &SkillData) -> String {
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
        short_revision(updated.repository_commit_sha.as_deref())
    )
}

pub(super) fn short_revision(revision: Option<&str>) -> String {
    let value = revision.unwrap_or("unknown");
    value.chars().take(7).collect()
}
