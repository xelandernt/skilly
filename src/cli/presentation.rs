use super::*;

pub(super) fn skillsmp_search_status(installed: Option<&SkillData>) -> &'static str {
    if installed.is_some() {
        STATUS_INSTALLED
    } else {
        STATUS_INSTALLABLE
    }
}

pub(super) fn skill_directory_name(skill: &SkillData) -> String {
    skill.directory_name()
}

pub(super) fn skill_origin_label(skill: &SkillData) -> &str {
    if skill.source == "repository" {
        return skill
            .repository_provider
            .map_or("repository", |provider| provider.as_str());
    }
    &skill.source
}

pub(super) fn skill_source_label(skill: &SkillData) -> String {
    if let (Some(repository_url), Some(provider)) =
        (skill.repository_url.as_deref(), skill.repository_provider)
        && let Ok(location) = parse_repository_location(repository_url, Some(provider))
    {
        return repository_source_label(&location);
    }
    if let Some(package_reference) = skill.package_reference() {
        let ecosystem = skill
            .package_ecosystem
            .as_ref()
            .map_or("dependency", |ecosystem| ecosystem.as_str());
        return format!("{} dependency · {package_reference}", title_case(ecosystem));
    }
    if skill.source == SKILLY_UNKNOWN_SOURCE {
        "Local".to_string()
    } else {
        title_case(&skill.source)
    }
}

pub(super) fn skillsmp_search_source_label(skill: &SkillsMpSkill) -> String {
    parse_repository_location(&skill.repository_url, None)
        .map(|location| repository_source_label(&location))
        .unwrap_or_else(|_| format!("SkillsMP · {}", skill.author))
}

pub(super) fn repository_source_label(location: &RepositoryLocationData) -> String {
    let provider = match location.provider {
        RepositoryProvider::GitHub => "GitHub".to_string(),
        RepositoryProvider::BitbucketCloud => "Bitbucket".to_string(),
        RepositoryProvider::BitbucketDataCenter => location
            .base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string(),
    };
    format!("{provider} · {}/{}", location.namespace, location.repo)
}

pub(super) fn repository_preview_label(
    provider: RepositoryProvider,
    repository_url: &str,
) -> String {
    parse_repository_location(repository_url, Some(provider))
        .map(|location| repository_source_label(&location))
        .unwrap_or_else(|_| provider.as_str().to_string())
}

pub(super) fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}

pub(super) fn installed_skill_label(skill: &SkillData) -> String {
    let mut details = Vec::new();
    if let Some(package_reference) = skill.package_reference() {
        details.push(package_reference);
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
        skill_origin_label(skill),
        detail_suffix
    )
}

pub(super) fn invalid_installed_skill_label(skill: &InvalidInstalledSkill) -> String {
    format!("{}: invalid [invalid]", skill.directory_name)
}

pub(super) fn listed_skill_label(entry: &ListedSkillEntry) -> String {
    match entry {
        ListedSkillEntry::Valid(skill) => installed_skill_label(skill),
        ListedSkillEntry::Invalid(skill) => invalid_installed_skill_label(skill),
    }
}

pub(super) fn listed_skill_name(entry: &ListedSkillEntry) -> String {
    match entry {
        ListedSkillEntry::Valid(skill) => skill.name.clone(),
        ListedSkillEntry::Invalid(skill) => skill.directory_name.clone(),
    }
}

pub(super) fn listed_skill_source_label(entry: &ListedSkillEntry) -> Option<String> {
    match entry {
        ListedSkillEntry::Valid(skill) => Some(skill_source_label(skill)),
        ListedSkillEntry::Invalid(_) => Some("Invalid skill".to_string()),
    }
}

pub(super) fn listed_skill_menu_status(
    entry: &ListedSkillEntry,
    state: Option<&UpdateCheckState>,
) -> MenuItemStatus {
    match entry {
        ListedSkillEntry::Valid(_) => match state {
            Some(UpdateCheckState::Checking) => MenuItemStatus::Checking,
            Some(UpdateCheckState::Updatable(_)) => MenuItemStatus::Updatable,
            Some(UpdateCheckState::Failed(_)) => MenuItemStatus::CheckFailed,
            Some(UpdateCheckState::Latest) => MenuItemStatus::UpToDate,
            None => MenuItemStatus::Default,
        },
        ListedSkillEntry::Invalid(_) => MenuItemStatus::Disabled,
    }
}

pub(super) fn update_check_state_for(
    checks: &UpdateChecks,
    directory: &Path,
    entry: &ListedSkillEntry,
) -> Option<UpdateCheckState> {
    let ListedSkillEntry::Valid(skill) = entry else {
        return None;
    };
    checks.state(&UpdateCheckKey::new(directory, skill))
}

pub(super) fn update_check_status(
    progress: UpdateCheckProgress,
    frame_index: usize,
    fallback: Option<&str>,
) -> Option<String> {
    if progress.total == 0 {
        return fallback.map(str::to_string);
    }
    if progress.is_checking() {
        let spinner = LOADING_FRAMES[frame_index % LOADING_FRAMES.len()];
        return Some(format!(
            "{spinner} Checking for updates... {}/{}",
            progress.checked, progress.total
        ));
    }
    fallback.map(str::to_string).or_else(|| {
        let update_label = if progress.updates == 1 {
            "update"
        } else {
            "updates"
        };
        let check_label = if progress.failures == 1 {
            "check"
        } else {
            "checks"
        };
        Some(format!(
            "Update check complete: {} {update_label} available, {} {check_label} failed",
            progress.updates, progress.failures,
        ))
    })
}

pub(super) fn refresh_list_menu(
    menu: &mut MenuUi,
    entries: &[ListedSkillEntry],
    directory: &Path,
    checks: &UpdateChecks,
    frame_index: usize,
    fallback_status: Option<&str>,
) -> bool {
    for (item, entry) in menu.items.iter_mut().zip(entries) {
        let state = update_check_state_for(checks, directory, entry);
        item.label = listed_skill_name(entry);
        item.subtitle = listed_skill_source_label(entry);
        item.preview_lines = listed_skill_preview_lines_with_update_state(entry, state.as_ref());
        item.status = listed_skill_menu_status(entry, state.as_ref());
    }
    let progress = checks.progress();
    menu.status = update_check_status(progress, frame_index, fallback_status);
    progress.is_checking()
}

pub(super) fn scan_choice_label(item: &SkillMatchData) -> String {
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

pub(super) fn scan_dependency_label(origins: &[ProjectDependencyOrigin]) -> String {
    if origins.is_empty() {
        return "unknown".to_string();
    }
    origins
        .iter()
        .map(ProjectDependencyOrigin::scan_label)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn scan_skill_actions(item: &SkillMatchData) -> Vec<&'static str> {
    match scan_match_status(&item.available, item.installed.as_ref()) {
        STATUS_UPDATABLE => vec![UPDATE_CHOICE, VIEW_FILES_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        STATUS_INSTALLED => vec![VIEW_FILES_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        _ => vec![INSTALL_CHOICE, VIEW_FILES_CHOICE, BACK_CHOICE, EXIT_CHOICE],
    }
}

pub(super) fn scan_menu_status(item: &SkillMatchData) -> MenuItemStatus {
    match scan_match_status(&item.available, item.installed.as_ref()) {
        STATUS_UPDATABLE => MenuItemStatus::Updatable,
        STATUS_INSTALLED => MenuItemStatus::Installed,
        _ => MenuItemStatus::Installable,
    }
}

pub(super) fn installed_skill_actions(update_available: bool, remove_choice: &str) -> Vec<&str> {
    let mut actions = vec![VIEW_FILES_CHOICE, remove_choice, BACK_CHOICE, EXIT_CHOICE];
    if update_available {
        actions.insert(1, UPDATE_CHOICE);
    }
    actions
}

pub(super) fn invalid_installed_skill_actions(remove_choice: &str) -> Vec<&str> {
    vec![remove_choice, BACK_CHOICE, EXIT_CHOICE]
}

pub(super) fn action_menu_default(actions: &[&str]) -> usize {
    actions
        .iter()
        .position(|action| {
            matches!(
                *action,
                INSTALL_CHOICE
                    | UPDATE_CHOICE
                    | VIEW_FILES_CHOICE
                    | APPLY_ALL_CHOICE
                    | INSTALL_ALL_CHOICE
                    | UPDATE_ALL_CHOICE
            )
        })
        .or_else(|| actions.iter().position(|action| *action == BACK_CHOICE))
        .unwrap_or(0)
}

pub(super) fn retained_multi_select_indices(action: Option<&str>, indices: &[usize]) -> Vec<usize> {
    match action {
        None | Some(BACK_CHOICE) => indices.to_vec(),
        Some(_) => Vec::new(),
    }
}

pub(super) fn menu_title_with_directory(title: String, directory: &Path) -> String {
    format!("{title} | Directory: {}", directory.display())
}

pub(super) fn menu_title_for_destination(
    title: String,
    destinations: &[ResolvedDestination],
    active_tab: usize,
) -> String {
    if destinations.len() <= 1 {
        return menu_title_with_directory(title, &destinations[active_tab].path);
    }
    title
}

pub(super) fn absolute_skill_path(skill: &SkillData) -> Option<PathBuf> {
    skill.path.as_deref().map(PathBuf::from)
}

pub(super) fn target_skill_path(skill: &SkillData, directory: &Path) -> PathBuf {
    directory.join(skill_directory_name(skill))
}

pub(super) fn no_skills_found_message(directory: &Path) -> String {
    format!("No skills found in directory {}", directory.display())
}

pub(super) fn no_skills_found_message_anywhere() -> String {
    "No skills found in any managed directory".to_string()
}

pub(super) fn pick_best_list_tab(
    destinations: &[ResolvedDestination],
    empty_flags: &[bool],
) -> usize {
    for (i, dest) in destinations.iter().enumerate() {
        if !empty_flags[i] && dest.label.contains("local") {
            return i;
        }
    }
    first_non_empty_tab(empty_flags)
}

pub(super) fn discover_installed_skills_report(
    directory: &Path,
) -> Result<InstalledSkillDiscoveryReport> {
    if !directory.exists() {
        return Ok(InstalledSkillDiscoveryReport::default());
    }
    if !directory.is_dir() {
        bail!("{}", directory.display());
    }

    let mut children =
        fs::read_dir(directory)?.collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    children.sort_by_key(|entry| entry.file_name());

    let mut report = InstalledSkillDiscoveryReport::default();
    for child in children {
        let child_path = child.path();
        if !child.file_type()?.is_dir() {
            continue;
        }
        match SkillData::from_dir_with_source_metadata(&child_path, &SkillSourceMetadata::default())
        {
            Ok(skill) => report.valid_skills.push(skill),
            Err(error) => report.invalid_skills.push(InvalidInstalledSkill {
                directory_name: child.file_name().to_string_lossy().to_string(),
                path: child_path,
                error: error.to_string(),
            }),
        }
    }
    Ok(report)
}

pub(super) fn listed_skill_entries(report: InstalledSkillDiscoveryReport) -> Vec<ListedSkillEntry> {
    let mut entries = report
        .valid_skills
        .into_iter()
        .map(|skill| ListedSkillEntry::Valid(Box::new(skill)))
        .collect::<Vec<_>>();
    entries.extend(
        report
            .invalid_skills
            .into_iter()
            .map(ListedSkillEntry::Invalid),
    );
    entries
}

pub(super) fn downloadable_skill_actions(item: &DownloadableSkillMatch) -> Vec<&'static str> {
    match item.status() {
        STATUS_INSTALLABLE => vec![INSTALL_CHOICE, VIEW_FILES_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        STATUS_UPDATABLE => {
            vec![
                UPDATE_CHOICE,
                VIEW_FILES_CHOICE,
                REMOVE_CHOICE,
                BACK_CHOICE,
                EXIT_CHOICE,
            ]
        }
        _ => vec![VIEW_FILES_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
    }
}

pub(super) fn downloadable_skill_menu_status(item: &DownloadableSkillMatch) -> MenuItemStatus {
    match item.status() {
        STATUS_INSTALLED => MenuItemStatus::Installed,
        STATUS_UPDATABLE => MenuItemStatus::Updatable,
        _ => MenuItemStatus::Installable,
    }
}

pub(super) fn skillsmp_search_menu_status(installed: Option<&SkillData>) -> MenuItemStatus {
    if installed.is_some() {
        MenuItemStatus::Installed
    } else {
        MenuItemStatus::Installable
    }
}

pub(super) fn downloadable_skill_matches(
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
                    installed.repository_url.is_some() && available.matches(installed)
                })
                .cloned(),
        })
        .collect()
}

pub(super) fn select_download_skill(skills: &[SkillData], skill_name: &str) -> Result<SkillData> {
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
            "downloadable skill not found: {}. available: {}",
            skill_name,
            skills
                .iter()
                .map(skill_directory_name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => bail!("multiple downloadable skills match name: {skill_name}"),
    }
}

pub(super) fn exit_menu_item(label: &str) -> MenuItemUi {
    MenuItemUi {
        label: EXIT_CHOICE.to_string(),
        subtitle: None,
        preview_lines: vec![label.to_string()],
        status: MenuItemStatus::Default,
        selectable: true,
        filter_text: None,
    }
}

pub(super) fn bundled_file_lines(skill: &SkillData) -> Vec<String> {
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

pub(super) fn skill_preview_lines(skill: &SkillData, extra_lines: &[String]) -> Vec<String> {
    let mut lines = vec![
        format!("Name: {}", skill.name),
        format!("Description: {}", skill.description),
    ];
    if let (Some(provider), Some(repository_url)) =
        (skill.repository_provider, skill.repository_url.as_deref())
    {
        lines.push(format!(
            "Repository: {}",
            repository_preview_label(provider, repository_url)
        ));
    } else {
        lines.push(format!("Source: {}", skill_origin_label(skill)));
    }
    lines.push(format!("Installed: {}", skill.is_installed()));
    if let Some(skill_path) = absolute_skill_path(skill) {
        lines.push(format!("Skill Path: {}", skill_path.display()));
    }
    if let Some(package_reference) = skill.package_reference() {
        lines.push(format!("Package: {package_reference}"));
    }
    if let (Some(repository_url), Some(commit_sha)) = (
        skill.repository_url.as_ref(),
        skill.repository_commit_sha.as_ref(),
    ) {
        lines.push(format!("Repository Url: {repository_url}"));
        lines.push(format!("Repository Commit: {commit_sha}"));
    }
    if !extra_lines.is_empty() {
        lines.push(String::new());
        lines.extend(extra_lines.iter().cloned());
    }
    lines.push(String::new());
    lines.extend(bundled_file_lines(skill));
    lines
}

pub(super) fn scan_match_preview_lines(item: &SkillMatchData) -> Vec<String> {
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

pub(super) fn installed_skill_preview_lines(skill: &SkillData) -> Vec<String> {
    skill_preview_lines(skill, &[])
}

pub(super) fn invalid_installed_skill_preview_lines(skill: &InvalidInstalledSkill) -> Vec<String> {
    vec![
        format!("Directory: {}", skill.directory_name),
        format!("Status: invalid"),
        format!("Path: {}", skill.path.display()),
        String::new(),
        "Error:".to_string(),
        skill.error.clone(),
    ]
}

pub(super) fn listed_skill_preview_lines(entry: &ListedSkillEntry) -> Vec<String> {
    listed_skill_preview_lines_with_update_state(entry, None)
}

pub(super) fn listed_skill_preview_lines_with_update_state(
    entry: &ListedSkillEntry,
    state: Option<&UpdateCheckState>,
) -> Vec<String> {
    match entry {
        ListedSkillEntry::Valid(skill) => {
            let extra = match state {
                Some(UpdateCheckState::Checking) => vec!["Update Status: checking".to_string()],
                Some(UpdateCheckState::Latest) => vec!["Update Status: latest".to_string()],
                Some(UpdateCheckState::Updatable(available)) => vec![
                    "Update Status: updatable".to_string(),
                    format!(
                        "Available Update: {}",
                        format_update_transition(skill, available)
                    ),
                ],
                Some(UpdateCheckState::Failed(error)) => vec![
                    "Update Status: check failed".to_string(),
                    format!("Update Check Error: {}", sanitize_tui_line(error)),
                ],
                None => Vec::new(),
            };
            skill_preview_lines(skill, &extra)
        }
        ListedSkillEntry::Invalid(skill) => invalid_installed_skill_preview_lines(skill),
    }
}

pub(super) fn sanitize_tui_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

pub(super) fn skillsmp_search_preview_lines(
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
        format!("Repository Url: {}", skill.repository_url),
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

pub(super) fn skillsmp_installable_preview_lines(
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

pub(super) fn installed_skillsmp_match(
    skill: &SkillsMpSkill,
    installed_skills: &[SkillData],
) -> Option<SkillData> {
    installed_skills
        .iter()
        .find(|installed| {
            installed.repository_url.as_deref() == Some(skill.repository_url.as_str())
        })
        .cloned()
}

pub(super) fn downloadable_skill_preview_lines(
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
