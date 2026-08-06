use crate::client::SkillsMpClient;
use crate::core::{
    ProjectEnvironment, RepositoryLocationData, RepositorySnapshotFetcher, SkillData,
    available_repository_update, discover_repository_skills_from_snapshot, scan_project_in,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

const MAX_UPDATE_CHECK_WORKERS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct UpdateCheckKey {
    directory: PathBuf,
    skill_directory_name: String,
}

impl UpdateCheckKey {
    pub(super) fn new(directory: &Path, skill: &SkillData) -> Self {
        Self {
            directory: directory.to_path_buf(),
            skill_directory_name: skill.directory_name(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum UpdateCheckState {
    Checking,
    Latest,
    Updatable(Box<SkillData>),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UpdateCheckProgress {
    pub(super) checked: usize,
    pub(super) total: usize,
    pub(super) updates: usize,
    pub(super) failures: usize,
}

impl UpdateCheckProgress {
    pub(super) fn is_checking(self) -> bool {
        self.checked < self.total
    }
}

pub(super) enum UpdateCheckRequest {
    Dependencies {
        environment: ProjectEnvironment,
        skills: Vec<(UpdateCheckKey, SkillData)>,
    },
    Repository {
        location: RepositoryLocationData,
        skills: Vec<(UpdateCheckKey, Box<SkillData>)>,
    },
    Failed {
        key: UpdateCheckKey,
        error: String,
    },
}

impl UpdateCheckRequest {
    fn keys(&self) -> Vec<UpdateCheckKey> {
        match self {
            Self::Dependencies { skills, .. } => {
                skills.iter().map(|(key, _)| key.clone()).collect()
            }
            Self::Repository { skills, .. } => skills.iter().map(|(key, _)| key.clone()).collect(),
            Self::Failed { key, .. } => vec![key.clone()],
        }
    }
}

pub(super) struct UpdateChecks {
    states: Arc<Mutex<BTreeMap<UpdateCheckKey, UpdateCheckState>>>,
    cancelled: Arc<AtomicBool>,
}

impl UpdateChecks {
    pub(super) fn start(requests: Vec<UpdateCheckRequest>, client: Arc<SkillsMpClient>) -> Self {
        let states = Arc::new(Mutex::new(BTreeMap::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut pending = Vec::new();

        for request in requests {
            for key in request.keys() {
                lock(&states).insert(key, UpdateCheckState::Checking);
            }
            match request {
                UpdateCheckRequest::Failed { key, error } => {
                    lock(&states).insert(key, UpdateCheckState::Failed(error));
                }
                request => pending.push(request),
            }
        }

        if pending.is_empty() {
            return Self { states, cancelled };
        }

        let pending_keys = pending
            .iter()
            .flat_map(UpdateCheckRequest::keys)
            .collect::<Vec<_>>();
        let worker_states = Arc::clone(&states);
        let worker_cancelled = Arc::clone(&cancelled);
        if let Err(error) = thread::Builder::new()
            .name("skilly-update-check".to_string())
            .spawn(move || run_requests(pending, &worker_states, &worker_cancelled, &client))
        {
            let message = format!("could not start update checks: {error}");
            for key in pending_keys {
                lock(&states).insert(key, UpdateCheckState::Failed(message.clone()));
            }
        }

        Self { states, cancelled }
    }

    pub(super) fn state(&self, key: &UpdateCheckKey) -> Option<UpdateCheckState> {
        lock(&self.states).get(key).cloned()
    }

    pub(super) fn progress(&self) -> UpdateCheckProgress {
        let states = lock(&self.states);
        UpdateCheckProgress {
            checked: states
                .values()
                .filter(|state| !matches!(state, UpdateCheckState::Checking))
                .count(),
            total: states.len(),
            updates: states
                .values()
                .filter(|state| matches!(state, UpdateCheckState::Updatable(_)))
                .count(),
            failures: states
                .values()
                .filter(|state| matches!(state, UpdateCheckState::Failed(_)))
                .count(),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for UpdateChecks {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn run_requests(
    requests: Vec<UpdateCheckRequest>,
    states: &Mutex<BTreeMap<UpdateCheckKey, UpdateCheckState>>,
    cancelled: &AtomicBool,
    client: &SkillsMpClient,
) {
    let mut requests = requests.into_iter();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let batch = requests
            .by_ref()
            .take(MAX_UPDATE_CHECK_WORKERS)
            .collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }
        thread::scope(|scope| {
            for request in batch {
                scope.spawn(move || match request {
                    UpdateCheckRequest::Dependencies {
                        environment,
                        skills,
                    } => check_dependencies(states, cancelled, &environment, skills),
                    UpdateCheckRequest::Repository { location, skills } => {
                        check_repository(states, cancelled, client, &location, skills);
                    }
                    UpdateCheckRequest::Failed { key, error } => {
                        set_state(states, cancelled, key, UpdateCheckState::Failed(error));
                    }
                });
            }
        });
    }
}

fn check_dependencies(
    states: &Mutex<BTreeMap<UpdateCheckKey, UpdateCheckState>>,
    cancelled: &AtomicBool,
    environment: &ProjectEnvironment,
    skills: Vec<(UpdateCheckKey, SkillData)>,
) {
    let discovered = match scan_project_in(environment) {
        Ok(discovered) => discovered,
        Err(error) => {
            let message = error.to_string();
            for (key, _) in skills {
                set_state(
                    states,
                    cancelled,
                    key,
                    UpdateCheckState::Failed(message.clone()),
                );
            }
            return;
        }
    };

    for (key, installed) in skills {
        let state = discovered
            .iter()
            .find(|item| installed.matches(&item.available))
            .map_or_else(
                || UpdateCheckState::Failed("dependency source not found".to_string()),
                |item| {
                    if installed.package_version == item.available.package_version {
                        UpdateCheckState::Latest
                    } else {
                        UpdateCheckState::Updatable(Box::new(item.available.clone()))
                    }
                },
            );
        set_state(states, cancelled, key, state);
    }
}

fn check_repository(
    states: &Mutex<BTreeMap<UpdateCheckKey, UpdateCheckState>>,
    cancelled: &AtomicBool,
    client: &SkillsMpClient,
    location: &RepositoryLocationData,
    skills: Vec<(UpdateCheckKey, Box<SkillData>)>,
) {
    let available = client
        .fetch_repository_snapshot(location)
        .and_then(|snapshot| discover_repository_skills_from_snapshot(location, &snapshot));
    match available {
        Ok(available) => {
            for (key, installed) in skills {
                let state = match available_repository_update(&installed, &available) {
                    Ok(Some(candidate)) => UpdateCheckState::Updatable(Box::new(candidate)),
                    Ok(None) => UpdateCheckState::Latest,
                    Err(error) => UpdateCheckState::Failed(error.to_string()),
                };
                set_state(states, cancelled, key, state);
            }
        }
        Err(error) => {
            let message = error.to_string();
            for (key, _) in skills {
                set_state(
                    states,
                    cancelled,
                    key,
                    UpdateCheckState::Failed(message.clone()),
                );
            }
        }
    };
}

fn set_state(
    states: &Mutex<BTreeMap<UpdateCheckKey, UpdateCheckState>>,
    cancelled: &AtomicBool,
    key: UpdateCheckKey,
    state: UpdateCheckState,
) {
    if !cancelled.load(Ordering::Relaxed) {
        lock(states).insert(key, state);
    }
}

#[cfg(test)]
mod tests {
    use super::{UpdateCheckKey, UpdateCheckState, UpdateChecks};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    #[test]
    fn progress_counts_pending_updates_and_failures() {
        let states = BTreeMap::from([
            (
                UpdateCheckKey {
                    directory: PathBuf::from("one"),
                    skill_directory_name: "checking".to_string(),
                },
                UpdateCheckState::Checking,
            ),
            (
                UpdateCheckKey {
                    directory: PathBuf::from("one"),
                    skill_directory_name: "latest".to_string(),
                },
                UpdateCheckState::Latest,
            ),
            (
                UpdateCheckKey {
                    directory: PathBuf::from("one"),
                    skill_directory_name: "failed".to_string(),
                },
                UpdateCheckState::Failed("offline".to_string()),
            ),
        ]);
        let checks = UpdateChecks {
            states: Arc::new(Mutex::new(states)),
            cancelled: Arc::new(AtomicBool::new(false)),
        };

        let progress = checks.progress();

        assert_eq!(progress.checked, 2);
        assert_eq!(progress.total, 3);
        assert_eq!(progress.updates, 0);
        assert_eq!(progress.failures, 1);
        assert!(progress.is_checking());
    }
}
