use super::*;

pub(super) fn build_update_check_requests(
    destinations: &[ResolvedDestination],
    entries_by_tab: &[Vec<ListedSkillEntry>],
) -> Vec<UpdateCheckRequest> {
    let mut requests = Vec::new();
    let mut repository_checks =
        std::collections::BTreeMap::<RepositoryUpdateCheckGroup, Vec<RepositoryUpdateCheck>>::new();
    for (destination, entries) in destinations.iter().zip(entries_by_tab) {
        let mut dependencies = Vec::new();
        for entry in entries {
            let ListedSkillEntry::Valid(skill) = entry else {
                continue;
            };
            let key = UpdateCheckKey::new(&destination.path, skill);
            if skill.is_dependency() {
                dependencies.push((key, skill.as_ref().clone()));
                continue;
            }
            match (skill.repository_url.as_ref(), skill.repository_provider) {
                (Some(repository_url), Some(provider)) => {
                    match parse_repository_location(repository_url, Some(provider)) {
                        Ok(location) => {
                            repository_checks
                                .entry(RepositoryUpdateCheckGroup::from(&location))
                                .or_default()
                                .push(RepositoryUpdateCheck {
                                    key,
                                    skill: skill.clone(),
                                    location,
                                });
                        }
                        Err(error) => requests.push(UpdateCheckRequest::Failed {
                            key,
                            error: error.to_string(),
                        }),
                    }
                }
                (Some(_), None) => requests.push(UpdateCheckRequest::Failed {
                    key,
                    error: "repository provider is missing".to_string(),
                }),
                (None, Some(_)) => requests.push(UpdateCheckRequest::Failed {
                    key,
                    error: "repository URL is missing".to_string(),
                }),
                (None, None) if skill.source == SKILLY_SOURCE_REPOSITORY => {
                    requests.push(UpdateCheckRequest::Failed {
                        key,
                        error: "repository update metadata is missing".to_string(),
                    });
                }
                (None, None) => {}
            }
        }
        if !dependencies.is_empty() {
            requests.push(UpdateCheckRequest::Dependencies {
                environment: build_project_environment(
                    &destination.path,
                    &ScanDependencySelection::default(),
                ),
                skills: dependencies,
            });
        }
    }
    for checks in repository_checks.into_values() {
        let location = shared_repository_location(&checks);
        let skills = checks
            .into_iter()
            .map(|check| (check.key, check.skill))
            .collect();
        requests.push(UpdateCheckRequest::Repository { location, skills });
    }
    requests
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RepositoryUpdateCheckGroup {
    provider: RepositoryProvider,
    base_url: String,
    namespace: String,
    repository: String,
    reference: Option<String>,
}

impl From<&RepositoryLocationData> for RepositoryUpdateCheckGroup {
    fn from(location: &RepositoryLocationData) -> Self {
        Self {
            provider: location.provider,
            base_url: location.base_url.clone(),
            namespace: location.namespace.clone(),
            repository: location.repo.clone(),
            reference: location.r#ref.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct RepositoryUpdateCheck {
    key: UpdateCheckKey,
    skill: Box<SkillData>,
    location: RepositoryLocationData,
}

pub(super) fn shared_repository_location(
    checks: &[RepositoryUpdateCheck],
) -> RepositoryLocationData {
    let mut location = checks[0].location.clone();
    location.path = common_repository_path(checks.iter().map(|check| check.location.path.as_str()));
    location
}

pub(super) fn common_repository_path<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut paths = paths;
    let Some(first) = paths.next() else {
        return ".".to_string();
    };
    let mut common = first
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    for path in paths {
        let parts = path
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>();
        let shared_length = common
            .iter()
            .zip(parts)
            .take_while(|(left, right)| *left == right)
            .count();
        common.truncate(shared_length);
    }
    if common.is_empty() {
        ".".to_string()
    } else {
        common.join("/")
    }
}
