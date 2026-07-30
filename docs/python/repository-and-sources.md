# Repository and project sources

## SkillRepository

`SkillRepository` bundles a directory and project settings for stateful
workflows — listing, installing, scanning, and updating skills.

```python
from pathlib import Path
from skilly import ProjectSettings, PythonSource, Skill, SkillRepository

repository = SkillRepository(
    directory=Path(".agents/skills"),
    project=ProjectSettings(
        sources=(
            PythonSource(
                dependency_groups=("dev",),
                optional_dependencies=("docs",),
            ),
        ),
    ),
)
```

### Constructor

| Parameter | Type | Default | Description |
|---|---|---|---|
| `directory` | `Path` | `Path(".agents/skills")` | Skills installation directory. |
| `project` | `ProjectSettings \| None` | Defaults to all sources | Project dependency settings. |
| `file_system` | `FileSystem \| None` | `None` (native) | Custom filesystem backend. |

### Methods

```python
# List installed skills
repository.list()

# Find a skill by name (returns None if not found)
repository.find("code-review")

# Find a skill or raise
repository.require("code-review")

# Install a skill
repository.install(
    Skill(name="deploy-check", description="...", body="#..."),
    overwrite=False,
)

# Remove a skill
repository.remove("deploy-check")

# Scan project dependencies for available skills
for match in repository.scan_project():
    print(match.available.name, match.status)

# Check for dependency updates
for update in repository.dependency_updates():
    print(update.installed.name, "->", update.available.name)

# Check repository and SkillsMP updates
for update in repository.updates():
    print(update.installed.name, "is updatable")
```

## ProjectSettings

Settings that define which project dependencies to scan for skills.

```python
from skilly import ProjectSettings

# Scan no sources (disable scanning)
ProjectSettings(sources=())

# All default sources (Python, Node, Maven)
ProjectSettings.defaults()
```

| Parameter | Type | Description |
|---|---|---|
| `sources` | `tuple[PackageSource, ...]` | Source configurations to scan. |

## PythonSource

```python
from skilly import PythonSource

source = PythonSource(
    pyproject_toml_path=Path("pyproject.toml"),
    venv_path=Path(".venv"),
    include_project_dependencies=True,
    dependency_groups=("dev", "test"),
    exclude_dependency_groups=None,
    optional_dependencies=None,
    exclude_optional_dependencies=("docs",),
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `pyproject_toml_path` | `Path` | `Path("pyproject.toml")` | Path to `pyproject.toml`. |
| `venv_path` | `Path` | `Path(".venv")` | Path to virtual environment. |
| `include_project_dependencies` | `bool` | `True` | Scan `[project].dependencies`. |
| `dependency_groups` | `tuple[str, ...] \| None` | `None` | Include only named groups. |
| `exclude_dependency_groups` | `tuple[str, ...] \| None` | `None` | Exclude named groups. |
| `optional_dependencies` | `tuple[str, ...] \| None` | `None` | Include only named extras. |
| `exclude_optional_dependencies` | `tuple[str, ...] \| None` | `None` | Exclude named extras. |

Cannot set both `dependency_groups` and `exclude_dependency_groups`, or both
`optional_dependencies` and `exclude_optional_dependencies` (raises
`ValueError`).

## NodeSource

```python
from skilly import NodeSource

source = NodeSource(
    package_json_path=Path("package.json"),
    node_modules_path=Path("node_modules"),
    include_dependencies=True,
    include_dev_dependencies=False,
    include_optional_dependencies=True,
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `package_json_path` | `Path` | `Path("package.json")` | Path to `package.json`. |
| `node_modules_path` | `Path` | `Path("node_modules")` | Path to `node_modules/`. |
| `include_dependencies` | `bool` | `True` | Scan `dependencies`. |
| `include_dev_dependencies` | `bool` | `True` | Scan `devDependencies`. |
| `include_optional_dependencies` | `bool` | `True` | Scan `optionalDependencies`. |

## MavenSource

```python
from skilly import MavenSource

source = MavenSource(
    pom_xml_path=Path("pom.xml"),
    repository_path=Path("~/.m2/repository"),
    include_compile_scope=True,
    include_runtime_scope=True,
    include_provided_scope=False,
    include_test_scope=True,
    include_system_scope=False,
)
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `pom_xml_path` | `Path` | `Path("pom.xml")` | Path to `pom.xml`. |
| `repository_path` | `Path` | `Path("~/.m2/repository")` | Local Maven repository. |
| `include_compile_scope` | `bool` | `True` | Include compile-scope deps. |
| `include_runtime_scope` | `bool` | `True` | Include runtime-scope deps. |
| `include_provided_scope` | `bool` | `False` | Include provided-scope deps. |
| `include_test_scope` | `bool` | `True` | Include test-scope deps. |
| `include_system_scope` | `bool` | `False` | Include system-scope deps. |

## SkillMatch

```python
@dataclass(frozen=True)
class SkillMatch:
    available: Skill
    installed: Skill | None = None

    @property
    def status(self) -> SkillInstallStatus: ...
```

`status` returns one of `"installable"`, `"installed"`, or `"updatable"`.

## InstalledSkillUpdate

```python
@dataclass(frozen=True)
class InstalledSkillUpdate:
    installed: Skill
    available: Skill
```

## Discovery functions

```python
from skilly import (
    NodeSource,
    PythonSource,
    discover_installed_skills,
    discover_package_source_skills,
    parse_repository_location,
    resolve_skills_directory,
)

# List all installed skills in the default directory
installed = discover_installed_skills()

# Discover skills from a specific package source
python_skills = discover_package_source_skills(PythonSource())
node_skills = discover_package_source_skills(NodeSource())

# Parse a supported repository URL into a skill location
location = parse_repository_location(
    "https://github.com/example/project/tree/main/skills/code-review"
)

# Resolve a named agent's skills directory
path = resolve_skills_directory("claude", global_=False)
```
