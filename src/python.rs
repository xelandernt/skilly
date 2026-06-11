use crate::client::{ClientConfig, SkillsMpClient, SkillsMpSearchQuery};
use crate::core::{
    FileSystem, ProjectEnvironment, ScanDependencySelection, SkillData, SkillDirectoryFlavor,
    SkillResourceData, SkillSourceMetadata,
    available_dependency_skill_in as rust_available_dependency_skill,
    available_dependency_skill_with_file_system as rust_available_dependency_skill_with_file_system,
    discover_github_skills as rust_discover_github_skills,
    discover_installed_skills as rust_discover_installed_skills,
    discover_installed_skills_in as rust_discover_installed_skills_in,
    discover_venv_skills as rust_discover_venv_skills,
    discover_venv_skills_in as rust_discover_venv_skills_in,
    github_versions_match as rust_github_versions_match,
    parse_github_skill_url as rust_parse_github_skill_url,
    project_requirements as rust_project_requirements,
    project_requirements_in as rust_project_requirements_in, remove_skill as rust_remove_skill,
    remove_skill_in as rust_remove_skill_in, scan_project_in as rust_scan_project,
    scan_project_with_file_system as rust_scan_project_with_file_system,
    skills_directory as rust_skills_directory,
};
use crate::{cli, core};
use pyo3::class::basic::CompareOp;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyType};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn py_err<E: ToString>(error: E) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn to_py_serialized<T: Serialize>(py: Python<'_>, value: &T) -> PyResult<Py<PyAny>> {
    let json = serde_json::to_value(value).map_err(py_err)?;
    json_value_to_py(py, &json)
}

fn json_value_to_py(py: Python<'_>, value: &JsonValue) -> PyResult<Py<PyAny>> {
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(v) => Ok((*v)
            .into_pyobject(py)
            .map_err(py_err)?
            .to_owned()
            .into_any()
            .unbind()),
        JsonValue::Number(v) => {
            if let Some(number) = v.as_i64() {
                return Ok(number
                    .into_pyobject(py)
                    .map_err(py_err)?
                    .into_any()
                    .unbind());
            }
            if let Some(number) = v.as_u64() {
                return Ok(number
                    .into_pyobject(py)
                    .map_err(py_err)?
                    .into_any()
                    .unbind());
            }
            if let Some(number) = v.as_f64() {
                return Ok(number
                    .into_pyobject(py)
                    .map_err(py_err)?
                    .into_any()
                    .unbind());
            }
            Err(py_err("Unsupported JSON number"))
        }
        JsonValue::String(v) => Ok(v
            .clone()
            .into_pyobject(py)
            .map_err(py_err)?
            .into_any()
            .unbind()),
        JsonValue::Array(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(json_value_to_py(py, value)?)?;
            }
            Ok(list.into_any().unbind())
        }
        JsonValue::Object(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, json_value_to_py(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn client_config(
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> ClientConfig {
    ClientConfig {
        base_url,
        api_key,
        github_token,
        proxy,
    }
}

fn skillsmp_client(
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> anyhow::Result<SkillsMpClient> {
    SkillsMpClient::new(client_config(base_url, api_key, github_token, proxy))
}

fn with_skillsmp_client<T>(
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
    action: impl FnOnce(&SkillsMpClient) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let client = skillsmp_client(base_url, api_key, github_token, proxy)?;
    action(&client)
}

fn with_github_location<T>(
    github_url: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
    action: impl FnOnce(&SkillsMpClient, &core::GitHubSkillLocationData) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    with_skillsmp_client(base_url, api_key, github_token, proxy, |client| {
        let location = rust_parse_github_skill_url(&github_url)?;
        action(client, &location)
    })
}

fn py_path(py: Python<'_>, value: &str) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("pathlib")?
        .getattr("Path")?
        .call1((value,))?
        .unbind())
}

fn py_pure_posix_path(py: Python<'_>, value: &str) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("pathlib")?
        .getattr("PurePosixPath")?
        .call1((value,))?
        .unbind())
}

fn py_pure_windows_path(py: Python<'_>, value: &str) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("pathlib")?
        .getattr("PureWindowsPath")?
        .call1((value,))?
        .unbind())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BridgePathFlavor {
    Host,
    Posix,
    Windows,
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic()
    )
}

fn bridge_path_flavor(value: &str) -> BridgePathFlavor {
    if value.starts_with('/') {
        return BridgePathFlavor::Posix;
    }
    if has_windows_drive_prefix(value) || value.starts_with('\\') {
        return BridgePathFlavor::Windows;
    }
    BridgePathFlavor::Host
}

fn normalize_bridge_path(value: &str) -> (String, BridgePathFlavor) {
    let flavor = bridge_path_flavor(value);
    let normalized = match flavor {
        BridgePathFlavor::Host => value.to_string(),
        BridgePathFlavor::Posix => value.replace('\\', "/"),
        BridgePathFlavor::Windows => value.replace('/', "\\"),
    };
    (normalized, flavor)
}

fn py_bridge_path(py: Python<'_>, value: &str) -> PyResult<Py<PyAny>> {
    let (normalized, flavor) = normalize_bridge_path(value);
    match flavor {
        BridgePathFlavor::Host => py_path(py, &normalized),
        BridgePathFlavor::Posix => {
            if cfg!(windows) {
                py_pure_posix_path(py, &normalized)
            } else {
                py_path(py, &normalized)
            }
        }
        BridgePathFlavor::Windows => {
            if cfg!(windows) {
                py_path(py, &normalized)
            } else {
                py_pure_windows_path(py, &normalized)
            }
        }
    }
}

fn bridge_join_path(base: &str, child: &str) -> String {
    let (normalized, flavor) = normalize_bridge_path(base);
    match flavor {
        BridgePathFlavor::Host => Path::new(&normalized).join(child).display().to_string(),
        BridgePathFlavor::Posix => {
            if normalized.ends_with('/') {
                format!("{normalized}{child}")
            } else {
                format!("{normalized}/{child}")
            }
        }
        BridgePathFlavor::Windows => {
            if normalized.ends_with('\\') {
                format!("{normalized}{child}")
            } else {
                format!("{normalized}\\{child}")
            }
        }
    }
}

fn windows_root_to_posix(value: &str) -> Option<String> {
    if !value.starts_with('\\') || value.starts_with("\\\\") || has_windows_drive_prefix(value) {
        return None;
    }
    Some(format!(
        "/{}",
        value.trim_start_matches(['\\', '/']).replace('\\', "/")
    ))
}

fn path_exists_in_custom_fs(py: Python<'_>, file_system: &Bound<'_, PyAny>, value: &str) -> bool {
    let Ok(path_arg) = py_bridge_path(py, value) else {
        return false;
    };
    match file_system.call_method1("exists", (path_arg,)) {
        Ok(result) if result.extract().unwrap_or(false) => true,
        Ok(_) => {
            let Ok(path_arg) = py_bridge_path(py, value) else {
                return false;
            };
            match file_system.call_method1("is_dir", (path_arg,)) {
                Ok(result) => result.extract().unwrap_or(false),
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn normalize_resolved_custom_path(
    py: Python<'_>,
    file_system: &Bound<'_, PyAny>,
    value: &str,
) -> String {
    let Some(posix_path) = windows_root_to_posix(value) else {
        return value.to_string();
    };
    if path_exists_in_custom_fs(py, file_system, value) {
        return value.to_string();
    }
    if path_exists_in_custom_fs(py, file_system, &posix_path) {
        return posix_path;
    }
    value.to_string()
}

fn py_fspath_string(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value
        .py()
        .import("os")?
        .getattr("fspath")?
        .call1((value,))?
        .extract()
}

fn optional_path_arg(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<String>> {
    value.map(py_fspath_string).transpose()
}

fn default_directory_arg(value: Option<&Bound<'_, PyAny>>, default: &str) -> PyResult<String> {
    Ok(optional_path_arg(value)?.unwrap_or_else(|| default.to_string()))
}

fn relative_path_arg(value: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(py_fspath_string(value)?.replace('\\', "/"))
}

fn optional_string_attr(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<String>> {
    let attr = value.getattr(name)?;
    if attr.is_none() {
        return Ok(None);
    }
    attr.extract().map(Some)
}

fn resource_from_py(value: &Bound<'_, PyAny>) -> PyResult<SkillResourceData> {
    Ok(SkillResourceData {
        relative_path: relative_path_arg(&value.getattr("relative_path")?)?,
        kind: value.getattr("kind")?.extract()?,
        content: value.getattr("content")?.extract()?,
    })
}

fn resources_from_py(value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<SkillResourceData>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value
        .downcast::<PyList>()
        .map_err(|_| PyTypeError::new_err("resources must be a list"))?;
    list.iter().map(|item| resource_from_py(&item)).collect()
}

fn py_resource(py: Python<'_>, value: &SkillResourceData) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        "relative_path",
        py_pure_posix_path(py, &value.relative_path)?,
    )?;
    kwargs.set_item("kind", value.kind.clone())?;
    kwargs.set_item("content", value.content.clone())?;
    Ok(py
        .import("skilly.skills")?
        .getattr("SkillResource")?
        .call((), Some(&kwargs))?
        .unbind())
}

fn py_resources(py: Python<'_>, values: &[SkillResourceData]) -> PyResult<Vec<Py<PyAny>>> {
    values.iter().map(|value| py_resource(py, value)).collect()
}

fn py_resources_by_kind(
    py: Python<'_>,
    values: &[SkillResourceData],
    kind: &str,
) -> PyResult<Vec<Py<PyAny>>> {
    values
        .iter()
        .filter(|value| value.kind == kind)
        .map(|value| py_resource(py, value))
        .collect()
}

fn skill_source_metadata(
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
) -> SkillSourceMetadata {
    SkillSourceMetadata {
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
    }
}

fn skillsmp_search_query(
    q: String,
    page: Option<u32>,
    limit: Option<u32>,
    sort_by: Option<String>,
    category: Option<String>,
    occupation: Option<String>,
) -> SkillsMpSearchQuery {
    SkillsMpSearchQuery {
        q,
        page,
        limit,
        sort_by,
        category,
        occupation,
    }
}

struct PythonFileSystem {
    inner: Py<PyAny>,
}

impl PythonFileSystem {
    fn new(file_system: &Bound<'_, PyAny>) -> Self {
        Self {
            inner: file_system.clone().unbind(),
        }
    }
}

impl FileSystem for PythonFileSystem {
    fn read_file(&self, path: &Path) -> anyhow::Result<String> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("read_file", (path_arg,))?
                .extract()
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn write_file(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("write_file", (path_arg, content))?;
            Ok(())
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn list_files(&self, path: &Path) -> anyhow::Result<Vec<String>> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("list_files", (path_arg,))?
                .extract()
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn exists(&self, path: &Path) -> anyhow::Result<bool> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("exists", (path_arg,))?
                .extract()
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn is_dir(&self, path: &Path) -> anyhow::Result<bool> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("is_dir", (path_arg,))?
                .extract()
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn make_dir(&self, path: &Path, parents: bool, exist_ok: bool) -> anyhow::Result<()> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("parents", parents)?;
            kwargs.set_item("exist_ok", exist_ok)?;
            self.inner
                .bind(py)
                .call_method("make_dir", (path_arg,), Some(&kwargs))?;
            Ok(())
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn remove_tree(&self, path: &Path) -> anyhow::Result<()> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("remove_tree", (path_arg,))?;
            Ok(())
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn replace_tree(&self, path: &Path, replacement: &Path) -> anyhow::Result<()> {
        Python::with_gil(|py| {
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            let replacement_arg = py_bridge_path(py, &replacement.to_string_lossy())?;
            self.inner
                .bind(py)
                .call_method1("replace_tree", (path_arg, replacement_arg))?;
            Ok(())
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }

    fn resolve(&self, path: &Path) -> anyhow::Result<PathBuf> {
        Python::with_gil(|py| {
            let file_system = self.inner.bind(py);
            let path_arg = py_bridge_path(py, &path.to_string_lossy())?;
            let resolved = file_system.call_method1("resolve", (path_arg,))?;
            let resolved = py_fspath_string(&resolved)?;
            Ok(PathBuf::from(normalize_resolved_custom_path(
                py,
                file_system,
                &resolved,
            )))
        })
        .map_err(|error: PyErr| anyhow::anyhow!(error.to_string()))
    }
}

fn skill_from_text_impl(
    text: &str,
    path: Option<&Bound<'_, PyAny>>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    let source_metadata = skill_source_metadata(
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
    );
    let path = optional_path_arg(path)?;
    let skill = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        SkillData::from_text_in(
            &file_system,
            text,
            path.as_deref().map(Path::new),
            &source_metadata,
        )
    } else {
        SkillData::from_text(text, path.as_deref().map(Path::new), &source_metadata)
    }
    .map_err(py_err)?;
    Ok(PySkill::from_data(skill))
}

fn skill_from_file_impl(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    let path = py_fspath_string(path)?;
    let source_metadata = skill_source_metadata(
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
    );
    let skill = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        SkillData::from_file_with_source_metadata_in(
            &file_system,
            Path::new(&path),
            &source_metadata,
        )
        .map_err(py_err)?
    } else {
        py.allow_threads(|| {
            SkillData::from_file_with_source_metadata(Path::new(&path), &source_metadata)
        })
        .map_err(py_err)?
    };
    Ok(PySkill::from_data(skill))
}

fn skill_from_dir_impl(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    let path = py_fspath_string(path)?;
    let source_metadata = skill_source_metadata(
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
    );
    let skill = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        SkillData::from_dir_with_source_metadata_in(
            &file_system,
            Path::new(&path),
            &source_metadata,
        )
        .map_err(py_err)?
    } else {
        py.allow_threads(|| {
            SkillData::from_dir_with_source_metadata(Path::new(&path), &source_metadata)
        })
        .map_err(py_err)?
    };
    Ok(PySkill::from_data(skill))
}

fn discover_github_skills_impl(
    py: Python<'_>,
    github_url: String,
    source: Option<String>,
    skillsmp_id: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Vec<PySkill>> {
    let skills = py
        .allow_threads(|| {
            let client = skillsmp_client(base_url, api_key, github_token, proxy)?;
            rust_discover_github_skills(
                &client,
                &github_url,
                source.as_deref().unwrap_or(core::SKILLY_SOURCE_GITHUB),
                skillsmp_id,
            )
        })
        .map_err(py_err)?;
    Ok(skills.into_iter().map(PySkill::from_data).collect())
}

fn client_config_from_fetcher(fetcher: &Bound<'_, PyAny>) -> PyResult<ClientConfig> {
    Ok(client_config(
        optional_string_attr(fetcher, "base_url")?,
        optional_string_attr(fetcher, "api_key")?,
        optional_string_attr(fetcher, "github_token")?,
        optional_string_attr(fetcher, "proxy")?,
    ))
}

fn skill_from_github_fetcher_impl(
    py: Python<'_>,
    fetcher: &Bound<'_, PyAny>,
    github_url: String,
    source: Option<String>,
    skillsmp_id: Option<String>,
) -> PyResult<PySkill> {
    let config = client_config_from_fetcher(fetcher)?;
    let skills = discover_github_skills_impl(
        py,
        github_url,
        Some(source.unwrap_or_else(|| core::SKILLY_SOURCE_GITHUB.to_string())),
        skillsmp_id,
        config.base_url,
        config.api_key,
        config.github_token,
        config.proxy,
    )?;
    match skills.len() {
        1 => Ok(skills.into_iter().next().expect("one skill exists")),
        count => Err(PyValueError::new_err(format!(
            "GitHub URL resolves to {count} skills; use a direct skill directory URL instead"
        ))),
    }
}

#[pyclass(name = "Skill", module = "skilly._core")]
#[derive(Clone)]
struct PySkill {
    inner: SkillData,
}

impl PySkill {
    fn from_data(inner: SkillData) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySkill {
    #[new]
    #[pyo3(signature = (
        name,
        description,
        path=None,
        content="",
        license=None,
        compatibility=None,
        metadata=None,
        allowed_tools=None,
        resources=None,
        resource_warnings=None,
        source=None,
        package_name=None,
        package_version=None,
        github_url=None,
        github_commit_sha=None,
        skillsmp_id=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        description: String,
        path: Option<&Bound<'_, PyAny>>,
        content: &str,
        license: Option<String>,
        compatibility: Option<String>,
        metadata: Option<BTreeMap<String, String>>,
        allowed_tools: Option<String>,
        resources: Option<&Bound<'_, PyAny>>,
        resource_warnings: Option<Vec<String>>,
        source: Option<String>,
        package_name: Option<String>,
        package_version: Option<String>,
        github_url: Option<String>,
        github_commit_sha: Option<String>,
        skillsmp_id: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: SkillData {
                name,
                description,
                path: optional_path_arg(path)?,
                content: content.to_string(),
                license,
                compatibility,
                metadata: metadata.unwrap_or_default(),
                allowed_tools,
                resources: resources_from_py(resources)?,
                resource_warnings: resource_warnings.unwrap_or_default(),
                source: source.unwrap_or_else(|| core::SKILLY_UNKNOWN_SOURCE.to_string()),
                package_name,
                package_version,
                github_url,
                github_commit_sha,
                skillsmp_id,
            },
        })
    }

    #[classmethod]
    #[pyo3(signature = (
        text,
        path=None,
        source=None,
        package_name=None,
        package_version=None,
        github_url=None,
        github_commit_sha=None,
        skillsmp_id=None,
        file_system=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn from_text(
        _cls: &Bound<'_, PyType>,
        text: String,
        path: Option<&Bound<'_, PyAny>>,
        source: Option<String>,
        package_name: Option<String>,
        package_version: Option<String>,
        github_url: Option<String>,
        github_commit_sha: Option<String>,
        skillsmp_id: Option<String>,
        file_system: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        skill_from_text_impl(
            &text,
            path,
            source,
            package_name,
            package_version,
            github_url,
            github_commit_sha,
            skillsmp_id,
            file_system,
        )
    }

    #[classmethod]
    #[pyo3(signature = (
        path,
        source=None,
        package_name=None,
        package_version=None,
        github_url=None,
        github_commit_sha=None,
        skillsmp_id=None,
        file_system=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn from_file(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        path: &Bound<'_, PyAny>,
        source: Option<String>,
        package_name: Option<String>,
        package_version: Option<String>,
        github_url: Option<String>,
        github_commit_sha: Option<String>,
        skillsmp_id: Option<String>,
        file_system: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        skill_from_file_impl(
            py,
            path,
            source,
            package_name,
            package_version,
            github_url,
            github_commit_sha,
            skillsmp_id,
            file_system,
        )
    }

    #[classmethod]
    #[pyo3(signature = (
        path,
        source=None,
        package_name=None,
        package_version=None,
        github_url=None,
        github_commit_sha=None,
        skillsmp_id=None,
        file_system=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn from_dir(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        path: &Bound<'_, PyAny>,
        source: Option<String>,
        package_name: Option<String>,
        package_version: Option<String>,
        github_url: Option<String>,
        github_commit_sha: Option<String>,
        skillsmp_id: Option<String>,
        file_system: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        skill_from_dir_impl(
            py,
            path,
            source,
            package_name,
            package_version,
            github_url,
            github_commit_sha,
            skillsmp_id,
            file_system,
        )
    }

    #[classmethod]
    #[pyo3(signature = (fetcher, github_url, source=None, skillsmp_id=None))]
    fn from_github(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        fetcher: &Bound<'_, PyAny>,
        github_url: String,
        source: Option<String>,
        skillsmp_id: Option<String>,
    ) -> PyResult<Self> {
        skill_from_github_fetcher_impl(py, fetcher, github_url, source, skillsmp_id)
    }

    #[classmethod]
    fn from_skillsmp(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        fetcher: &Bound<'_, PyAny>,
        installable_skill: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        skill_from_github_fetcher_impl(
            py,
            fetcher,
            installable_skill.getattr("github_url")?.extract()?,
            Some(core::SKILLY_SOURCE_SKILLSMP.to_string()),
            Some(installable_skill.getattr("id")?.extract()?),
        )
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn description(&self) -> String {
        self.inner.description.clone()
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .path
            .as_deref()
            .map(|value| py_bridge_path(py, value))
            .transpose()
    }

    #[getter]
    fn content(&self) -> String {
        self.inner.content.clone()
    }

    #[getter]
    fn license(&self) -> Option<String> {
        self.inner.license.clone()
    }

    #[getter]
    fn compatibility(&self) -> Option<String> {
        self.inner.compatibility.clone()
    }

    #[getter]
    fn metadata(&self) -> BTreeMap<String, String> {
        self.inner.metadata.clone()
    }

    #[getter]
    fn allowed_tools(&self) -> Option<String> {
        self.inner.allowed_tools.clone()
    }

    #[getter]
    fn resources(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        py_resources(py, &self.inner.resources)
    }

    #[getter]
    fn resource_warnings(&self) -> Vec<String> {
        self.inner.resource_warnings.clone()
    }

    #[getter]
    fn source(&self) -> String {
        self.inner.source.clone()
    }

    #[getter]
    fn package_name(&self) -> Option<String> {
        self.inner.package_name.clone()
    }

    #[getter]
    fn package_version(&self) -> Option<String> {
        self.inner.package_version.clone()
    }

    #[getter]
    fn github_url(&self) -> Option<String> {
        self.inner.github_url.clone()
    }

    #[getter]
    fn github_commit_sha(&self) -> Option<String> {
        self.inner.github_commit_sha.clone()
    }

    #[getter]
    fn skillsmp_id(&self) -> Option<String> {
        self.inner.skillsmp_id.clone()
    }

    #[getter]
    fn skill_markdown_path(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.inner
            .path
            .as_deref()
            .map(|value| py_bridge_path(py, &bridge_join_path(value, "SKILL.md")))
            .transpose()
    }

    #[getter]
    fn directory(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.path(py)
    }

    #[getter]
    fn directory_name(&self) -> String {
        self.inner.directory_name()
    }

    #[getter]
    fn scripts(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        py_resources_by_kind(py, &self.inner.resources, core::RESOURCE_KIND_SCRIPT)
    }

    #[getter]
    fn references(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        py_resources_by_kind(py, &self.inner.resources, core::RESOURCE_KIND_REFERENCE)
    }

    #[getter]
    fn assets(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        py_resources_by_kind(py, &self.inner.resources, core::RESOURCE_KIND_ASSET)
    }

    fn get_resource(
        &self,
        py: Python<'_>,
        relative_path: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Py<PyAny>>> {
        let wanted = relative_path_arg(relative_path)?;
        self.inner
            .resources
            .iter()
            .find(|value| value.relative_path == wanted)
            .map(|value| py_resource(py, value))
            .transpose()
    }

    fn is_installed(&self) -> bool {
        self.inner.is_installed()
    }

    fn is_dependency(&self) -> bool {
        self.inner.is_dependency()
    }

    fn is_skillsmp(&self) -> bool {
        self.inner.is_skillsmp()
    }

    fn is_github(&self) -> bool {
        self.inner.source == core::SKILLY_SOURCE_GITHUB
    }

    fn can_update(&self) -> bool {
        self.inner.can_update()
    }

    fn matches(&self, other: &PySkill) -> bool {
        self.inner.matches(&other.inner)
    }

    fn package_reference(&self) -> Option<String> {
        self.inner.package_reference()
    }

    fn managed_metadata(&self) -> BTreeMap<String, String> {
        self.inner.managed_metadata()
    }

    #[pyo3(signature = (metadata=None))]
    fn render(&self, metadata: Option<BTreeMap<String, String>>) -> String {
        self.inner.render(metadata.as_ref())
    }

    #[pyo3(signature = (directory=None, skill_name=None, overwrite=false, file_system=None))]
    fn install_to(
        &self,
        py: Python<'_>,
        directory: Option<&Bound<'_, PyAny>>,
        skill_name: Option<String>,
        overwrite: bool,
        file_system: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let directory = default_directory_arg(directory, core::DEFAULT_SKILLS_PATH)?;
        let installed = if let Some(file_system) = file_system {
            let file_system = PythonFileSystem::new(file_system);
            self.inner
                .install_to_in(
                    &file_system,
                    Path::new(&directory),
                    skill_name.as_deref(),
                    overwrite,
                )
                .map_err(py_err)?
        } else {
            py.allow_threads(|| {
                self.inner
                    .install_to(Path::new(&directory), skill_name.as_deref(), overwrite)
            })
            .map_err(py_err)?
        };
        Ok(Self::from_data(installed))
    }

    #[pyo3(signature = (directory=None, skill_name=None, file_system=None))]
    fn replace_to(
        &self,
        py: Python<'_>,
        directory: Option<&Bound<'_, PyAny>>,
        skill_name: Option<String>,
        file_system: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let directory = default_directory_arg(directory, core::DEFAULT_SKILLS_PATH)?;
        let installed = if let Some(file_system) = file_system {
            let file_system = PythonFileSystem::new(file_system);
            self.inner
                .replace_to_in(&file_system, Path::new(&directory), skill_name.as_deref())
                .map_err(py_err)?
        } else {
            py.allow_threads(|| {
                self.inner
                    .replace_to(Path::new(&directory), skill_name.as_deref())
            })
            .map_err(py_err)?
        };
        Ok(Self::from_data(installed))
    }

    fn __repr__(&self) -> String {
        format!(
            "Skill(name={:?}, path={:?})",
            self.inner.name,
            self.inner.path.as_deref()
        )
    }

    fn __richcmp__(
        &self,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let Ok(other) = other.extract::<PyRef<'_, PySkill>>() else {
            return Ok(py.NotImplemented());
        };
        let result = match op {
            CompareOp::Eq => self.inner == other.inner,
            CompareOp::Ne => self.inner != other.inner,
            _ => return Ok(py.NotImplemented()),
        };
        Ok(result
            .into_pyobject(py)
            .map_err(py_err)?
            .to_owned()
            .into_any()
            .unbind())
    }
}

#[pyfunction]
#[pyo3(name = "skill_from_text", signature = (
    text,
    path=None,
    source=None,
    package_name=None,
    package_version=None,
    github_url=None,
    github_commit_sha=None,
    skillsmp_id=None,
    file_system=None
))]
#[allow(clippy::too_many_arguments)]
fn skill_from_text_py(
    text: String,
    path: Option<&Bound<'_, PyAny>>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    skill_from_text_impl(
        &text,
        path,
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
        file_system,
    )
}

#[pyfunction]
#[pyo3(name = "skill_from_file", signature = (
    path,
    source=None,
    package_name=None,
    package_version=None,
    github_url=None,
    github_commit_sha=None,
    skillsmp_id=None,
    file_system=None
))]
#[allow(clippy::too_many_arguments)]
fn skill_from_file_py(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    skill_from_file_impl(
        py,
        path,
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
        file_system,
    )
}

#[pyfunction]
#[pyo3(name = "skill_from_dir", signature = (
    path,
    source=None,
    package_name=None,
    package_version=None,
    github_url=None,
    github_commit_sha=None,
    skillsmp_id=None,
    file_system=None
))]
#[allow(clippy::too_many_arguments)]
fn skill_from_dir_py(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    source: Option<String>,
    package_name: Option<String>,
    package_version: Option<String>,
    github_url: Option<String>,
    github_commit_sha: Option<String>,
    skillsmp_id: Option<String>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    skill_from_dir_impl(
        py,
        path,
        source,
        package_name,
        package_version,
        github_url,
        github_commit_sha,
        skillsmp_id,
        file_system,
    )
}

#[pyfunction]
#[pyo3(name = "skill_render", signature = (skill, metadata=None))]
fn skill_render_py(skill: &PySkill, metadata: Option<BTreeMap<String, String>>) -> String {
    skill.inner.render(metadata.as_ref())
}

#[pyfunction]
#[pyo3(name = "skill_install_to", signature = (skill, directory=None, skill_name=None, overwrite=false, file_system=None))]
fn skill_install_to_py(
    py: Python<'_>,
    skill: &PySkill,
    directory: Option<&Bound<'_, PyAny>>,
    skill_name: Option<String>,
    overwrite: bool,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    skill.install_to(py, directory, skill_name, overwrite, file_system)
}

#[pyfunction]
#[pyo3(name = "resolve_skills_directory", signature = (agent="agents", global_=false))]
fn resolve_skills_directory_py(py: Python<'_>, agent: &str, global_: bool) -> PyResult<Py<PyAny>> {
    let flavor = match agent {
        "agents" => SkillDirectoryFlavor::Agents,
        "claude" => SkillDirectoryFlavor::Claude,
        "codex" => SkillDirectoryFlavor::Codex,
        "copilot" => SkillDirectoryFlavor::Copilot,
        _ => {
            return Err(PyValueError::new_err(
                "agent must be one of: agents, claude, codex, copilot",
            ));
        }
    };
    let directory = rust_skills_directory(flavor, global_).map_err(py_err)?;
    py_path(py, &directory.to_string_lossy())
}

#[pyfunction]
#[pyo3(name = "discover_installed_skills", signature = (directory=None, file_system=None))]
fn discover_installed_skills_py(
    py: Python<'_>,
    directory: Option<&Bound<'_, PyAny>>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<PySkill>> {
    let directory = default_directory_arg(directory, core::DEFAULT_SKILLS_PATH)?;
    let skills = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_discover_installed_skills_in(&file_system, Path::new(&directory)).map_err(py_err)?
    } else {
        py.allow_threads(|| rust_discover_installed_skills(Path::new(&directory)))
            .map_err(py_err)?
    };
    Ok(skills.into_iter().map(PySkill::from_data).collect())
}

#[pyfunction]
#[pyo3(name = "discover_venv_skills", signature = (path=None, file_system=None))]
fn discover_venv_skills_py(
    py: Python<'_>,
    path: Option<&Bound<'_, PyAny>>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<PySkill>> {
    let path = default_directory_arg(path, ".venv")?;
    let skills = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_discover_venv_skills_in(&file_system, Path::new(&path)).map_err(py_err)?
    } else {
        py.allow_threads(|| rust_discover_venv_skills(Path::new(&path)))
            .map_err(py_err)?
    };
    Ok(skills.into_iter().map(PySkill::from_data).collect())
}

#[pyfunction]
#[pyo3(name = "scan_project", signature = (directory=None, pyproject_toml_path=None, venv_path=None, include_project_dependencies=true, include_dependency_groups=true, include_optional_dependencies=true, file_system=None))]
fn scan_project_py(
    py: Python<'_>,
    directory: Option<&Bound<'_, PyAny>>,
    pyproject_toml_path: Option<&Bound<'_, PyAny>>,
    venv_path: Option<&Bound<'_, PyAny>>,
    include_project_dependencies: bool,
    include_dependency_groups: bool,
    include_optional_dependencies: bool,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<(PySkill, Option<PySkill>)>> {
    let environment = ProjectEnvironment::with_paths(
        Path::new(&default_directory_arg(
            directory,
            core::DEFAULT_SKILLS_PATH,
        )?),
        Path::new(&default_directory_arg(
            pyproject_toml_path,
            "pyproject.toml",
        )?),
        Path::new(&default_directory_arg(venv_path, ".venv")?),
        ScanDependencySelection {
            include_project_dependencies,
            include_dependency_groups,
            include_optional_dependencies,
        },
    );
    let matches = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_scan_project_with_file_system(&file_system, &environment).map_err(py_err)?
    } else {
        py.allow_threads(|| rust_scan_project(&environment))
            .map_err(py_err)?
    };
    Ok(matches
        .into_iter()
        .map(|item| {
            (
                PySkill::from_data(item.available),
                item.installed.map(PySkill::from_data),
            )
        })
        .collect())
}

#[pyfunction]
#[pyo3(name = "available_dependency_skill", signature = (installed, directory=None, pyproject_toml_path=None, venv_path=None, include_project_dependencies=true, include_dependency_groups=true, include_optional_dependencies=true, file_system=None))]
fn available_dependency_skill_py(
    py: Python<'_>,
    installed: &PySkill,
    directory: Option<&Bound<'_, PyAny>>,
    pyproject_toml_path: Option<&Bound<'_, PyAny>>,
    venv_path: Option<&Bound<'_, PyAny>>,
    include_project_dependencies: bool,
    include_dependency_groups: bool,
    include_optional_dependencies: bool,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PySkill>> {
    let environment = ProjectEnvironment::with_paths(
        Path::new(&default_directory_arg(
            directory,
            core::DEFAULT_SKILLS_PATH,
        )?),
        Path::new(&default_directory_arg(
            pyproject_toml_path,
            "pyproject.toml",
        )?),
        Path::new(&default_directory_arg(venv_path, ".venv")?),
        ScanDependencySelection {
            include_project_dependencies,
            include_dependency_groups,
            include_optional_dependencies,
        },
    );
    let available = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_available_dependency_skill_with_file_system(
            &file_system,
            &installed.inner,
            &environment,
        )
        .map_err(py_err)?
    } else {
        py.allow_threads(|| rust_available_dependency_skill(&installed.inner, &environment))
            .map_err(py_err)?
    };
    Ok(available.map(PySkill::from_data))
}

#[pyfunction]
#[pyo3(name = "parse_github_skill_url")]
fn parse_github_skill_url_py(py: Python<'_>, github_url: String) -> PyResult<Py<PyAny>> {
    let location = rust_parse_github_skill_url(&github_url).map_err(py_err)?;
    to_py_serialized(py, &location)
}

#[pyfunction]
#[pyo3(name = "discover_github_skills", signature = (
    github_url,
    source=None,
    skillsmp_id=None,
    base_url=None,
    api_key=None,
    github_token=None,
    proxy=None
))]
#[allow(clippy::too_many_arguments)]
fn discover_github_skills_py(
    py: Python<'_>,
    github_url: String,
    source: Option<String>,
    skillsmp_id: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Vec<PySkill>> {
    discover_github_skills_impl(
        py,
        github_url,
        source,
        skillsmp_id,
        base_url,
        api_key,
        github_token,
        proxy,
    )
}

#[pyfunction]
#[pyo3(name = "github_versions_match")]
fn github_versions_match_py(installed: &PySkill, available: &PySkill) -> bool {
    rust_github_versions_match(&installed.inner, &available.inner)
}

#[pyfunction]
#[pyo3(name = "remove_skill", signature = (name, directory=None, file_system=None))]
fn remove_skill_py(
    py: Python<'_>,
    name: String,
    directory: Option<&Bound<'_, PyAny>>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<PySkill> {
    let directory = default_directory_arg(directory, core::DEFAULT_SKILLS_PATH)?;
    let removed = if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_remove_skill_in(&file_system, &name, Path::new(&directory)).map_err(py_err)?
    } else {
        py.allow_threads(|| rust_remove_skill(&name, Path::new(&directory)))
            .map_err(py_err)?
    };
    Ok(PySkill::from_data(removed))
}

#[pyfunction]
#[pyo3(name = "project_requirements", signature = (pyproject_toml_path=None, include_dev=false, include_extras=None, file_system=None))]
fn project_requirements_py(
    py: Python<'_>,
    pyproject_toml_path: Option<&Bound<'_, PyAny>>,
    include_dev: bool,
    include_extras: Option<Vec<String>>,
    file_system: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<String>> {
    let path = default_directory_arg(pyproject_toml_path, "pyproject.toml")?;
    let include_extras = include_extras.unwrap_or_default();
    if let Some(file_system) = file_system {
        let file_system = PythonFileSystem::new(file_system);
        rust_project_requirements_in(&file_system, Path::new(&path), include_dev, &include_extras)
            .map_err(py_err)
    } else {
        py.allow_threads(|| {
            rust_project_requirements(Path::new(&path), include_dev, &include_extras)
        })
        .map_err(py_err)
    }
}

#[pyfunction]
#[pyo3(name = "skillsmp_search", signature = (
    q,
    page=None,
    limit=None,
    sort_by=None,
    category=None,
    occupation=None,
    base_url=None,
    api_key=None,
    github_token=None,
    proxy=None
))]
#[allow(clippy::too_many_arguments)]
fn skillsmp_search_py(
    py: Python<'_>,
    q: String,
    page: Option<u32>,
    limit: Option<u32>,
    sort_by: Option<String>,
    category: Option<String>,
    occupation: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let response = py
        .allow_threads(|| {
            with_skillsmp_client(base_url, api_key, github_token, proxy, |client| {
                client.search(&skillsmp_search_query(
                    q, page, limit, sort_by, category, occupation,
                ))
            })
        })
        .map_err(py_err)?;
    to_py_serialized(py, &response)
}

#[pyfunction]
#[pyo3(name = "skillsmp_ai_search", signature = (q, base_url=None, api_key=None, github_token=None, proxy=None))]
fn skillsmp_ai_search_py(
    py: Python<'_>,
    q: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let response = py
        .allow_threads(|| {
            with_skillsmp_client(base_url, api_key, github_token, proxy, |client| {
                client.ai_search(&q)
            })
        })
        .map_err(py_err)?;
    to_py_serialized(py, &response)
}

#[pyfunction]
#[pyo3(name = "skillsmp_fetch_github_directory", signature = (github_url, current_path, base_url=None, api_key=None, github_token=None, proxy=None))]
fn skillsmp_fetch_github_directory_py(
    py: Python<'_>,
    github_url: String,
    current_path: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let entries = py
        .allow_threads(|| {
            with_github_location(
                github_url,
                base_url,
                api_key,
                github_token,
                proxy,
                |client, location| client.fetch_github_directory(location, &current_path),
            )
        })
        .map_err(py_err)?;
    to_py_serialized(py, &entries)
}

#[pyfunction]
#[pyo3(name = "skillsmp_fetch_github_file", signature = (github_url, path, base_url=None, api_key=None, github_token=None, proxy=None))]
fn skillsmp_fetch_github_file_py(
    py: Python<'_>,
    github_url: String,
    path: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let blob = py
        .allow_threads(|| {
            with_github_location(
                github_url,
                base_url,
                api_key,
                github_token,
                proxy,
                |client, location| client.fetch_github_file(location, &path),
            )
        })
        .map_err(py_err)?;
    to_py_serialized(py, &blob)
}

#[pyfunction]
#[pyo3(name = "skillsmp_fetch_github_snapshot", signature = (github_url, base_url=None, api_key=None, github_token=None, proxy=None))]
fn skillsmp_fetch_github_snapshot_py(
    py: Python<'_>,
    github_url: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let snapshot = py
        .allow_threads(|| {
            with_github_location(
                github_url,
                base_url,
                api_key,
                github_token,
                proxy,
                |client, location| client.fetch_github_snapshot(location),
            )
        })
        .map_err(py_err)?;
    to_py_serialized(py, &snapshot)
}

#[pyfunction]
#[pyo3(name = "skillsmp_resolve_github_ref_and_commit_sha", signature = (github_url, base_url=None, api_key=None, github_token=None, proxy=None))]
fn skillsmp_resolve_github_ref_and_commit_sha_py(
    py: Python<'_>,
    github_url: String,
    base_url: Option<String>,
    api_key: Option<String>,
    github_token: Option<String>,
    proxy: Option<String>,
) -> PyResult<Py<PyAny>> {
    let resolved = py
        .allow_threads(|| {
            with_github_location(
                github_url,
                base_url,
                api_key,
                github_token,
                proxy,
                |client, location| client.resolve_github_ref_and_commit_sha(location),
            )
        })
        .map_err(py_err)?;
    to_py_serialized(py, &resolved)
}

#[pyfunction]
fn run_cli(py: Python<'_>, args: Vec<String>) -> PyResult<i32> {
    Ok(py.allow_threads(|| cli::run(args)))
}

#[pymodule]
#[pyo3(name = "_core")]
fn python_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySkill>()?;
    m.add_function(wrap_pyfunction!(skill_from_text_py, m)?)?;
    m.add_function(wrap_pyfunction!(skill_from_file_py, m)?)?;
    m.add_function(wrap_pyfunction!(skill_from_dir_py, m)?)?;
    m.add_function(wrap_pyfunction!(skill_render_py, m)?)?;
    m.add_function(wrap_pyfunction!(skill_install_to_py, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_skills_directory_py, m)?)?;
    m.add_function(wrap_pyfunction!(discover_installed_skills_py, m)?)?;
    m.add_function(wrap_pyfunction!(discover_venv_skills_py, m)?)?;
    m.add_function(wrap_pyfunction!(scan_project_py, m)?)?;
    m.add_function(wrap_pyfunction!(available_dependency_skill_py, m)?)?;
    m.add_function(wrap_pyfunction!(parse_github_skill_url_py, m)?)?;
    m.add_function(wrap_pyfunction!(discover_github_skills_py, m)?)?;
    m.add_function(wrap_pyfunction!(github_versions_match_py, m)?)?;
    m.add_function(wrap_pyfunction!(remove_skill_py, m)?)?;
    m.add_function(wrap_pyfunction!(project_requirements_py, m)?)?;
    m.add_function(wrap_pyfunction!(skillsmp_search_py, m)?)?;
    m.add_function(wrap_pyfunction!(skillsmp_ai_search_py, m)?)?;
    m.add_function(wrap_pyfunction!(skillsmp_fetch_github_directory_py, m)?)?;
    m.add_function(wrap_pyfunction!(skillsmp_fetch_github_file_py, m)?)?;
    m.add_function(wrap_pyfunction!(skillsmp_fetch_github_snapshot_py, m)?)?;
    m.add_function(wrap_pyfunction!(
        skillsmp_resolve_github_ref_and_commit_sha_py,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(run_cli, m)?)?;
    Ok(())
}
