use super::*;

pub(super) fn remember_status(
    messages: &mut Vec<String>,
    status_message: &mut Option<String>,
    message: String,
) {
    *status_message = Some(message.clone());
    messages.push(message);
}

pub(super) fn install_available_skill(
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

pub(super) fn update_skill(
    directory: &Path,
    skill: &SkillData,
    client: &SkillsMpClient,
) -> Result<String> {
    if skill.is_dependency() {
        let environment = build_project_environment(directory, &ScanDependencySelection::default());
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

    if let (Some(repository_url), Some(provider)) =
        (skill.repository_url.as_deref(), skill.repository_provider)
    {
        let discovered = discover_repository_skills(client, repository_url, Some(provider))?;
        let Some(refreshed) = resolve_repository_update(skill, &discovered)? else {
            return Ok(format!(
                "{} is already up to date ({})",
                skill_directory_name(skill),
                skill.repository_commit_sha.as_deref().unwrap_or("unknown")
            ));
        };
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

pub(super) fn skill_update_available(
    directory: &Path,
    skill: &SkillData,
    client: &SkillsMpClient,
) -> Result<bool> {
    if skill.is_dependency() {
        let environment = build_project_environment(directory, &ScanDependencySelection::default());
        return Ok(available_dependency_skill_in(skill, &environment)?
            .is_some_and(|available| available.package_version != skill.package_version));
    }
    if let (Some(repository_url), Some(provider)) =
        (skill.repository_url.as_deref(), skill.repository_provider)
    {
        let discovered = discover_repository_skills(client, repository_url, Some(provider))?;
        return Ok(resolve_repository_update(skill, &discovered)?.is_some());
    }
    Ok(false)
}

pub(super) fn update_available_or_remember_error(
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
