use super::*;

#[derive(Debug, Clone)]
pub(super) struct PendingSkillUpdate {
    pub(super) installed: SkillData,
    pub(super) available: SkillData,
}

pub(super) fn run_update(directory: &Path, config: ClientConfig, yes: bool) -> Result<()> {
    let client = Arc::new(SkillsMpClient::new(config)?);
    let installed_skills = discover_installed_skills_report(directory)?.valid_skills;
    let destination = ResolvedDestination {
        label: "current".to_string(),
        path: directory.to_path_buf(),
        color: ratatui::style::Color::White,
    };
    let entries = installed_skills
        .iter()
        .cloned()
        .map(|skill| ListedSkillEntry::Valid(Box::new(skill)))
        .collect::<Vec<_>>();
    let requests = build_update_check_requests(
        std::slice::from_ref(&destination),
        std::slice::from_ref(&entries),
    );
    let checks = UpdateChecks::start(requests, Arc::clone(&client));
    while checks.progress().is_checking() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut updates = Vec::new();
    let mut errors = Vec::new();
    for installed in installed_skills {
        match checks.state(&UpdateCheckKey::new(directory, &installed)) {
            Some(UpdateCheckState::Updatable(available)) => updates.push(PendingSkillUpdate {
                installed,
                available: *available,
            }),
            Some(UpdateCheckState::Failed(error)) => errors.push(format!(
                "Could not check updates for {}: {error}",
                skill_directory_name(&installed)
            )),
            Some(UpdateCheckState::Checking | UpdateCheckState::Latest) | None => {}
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
        match install_available_skill(
            directory,
            &update.available,
            Some(&skill_directory_name(&update.installed)),
            true,
        ) {
            Ok(updated) => println!("{}", format_applied_update(&update.installed, &updated)),
            Err(error) => errors.push(format!(
                "Could not update {}: {error}",
                skill_directory_name(&update.installed)
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
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let normalized = answer.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
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
