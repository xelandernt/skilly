# Python API

The `skilly` Python package provides stateful workflows through
`SkillRepository`, local discovery functions, and caller-controlled repository
discovery.

## Compatibility

Requires Python 3.10 or later. The package includes a compiled Rust extension
via PyO3 (pre-built wheels for Linux x64, macOS arm64/x64, Windows x64).

## Import surfaces

```python
from skilly import (
    ResourceKind,
    Skill,
    RepositoryLocation,
    RepositoryDiscoveryClient,
    RepositoryTransport,
    SkillResource,
    discover_installed_skills,
    discover_package_source_skills,
    FileSystem,
    InstalledSkillUpdate,
    MavenSource,
    NodeSource,
    PackageSource,
    parse_repository_location,
    ProjectSettings,
    PythonSource,
    resolve_skills_directory,
    SkillMatch,
    SkillRepository,
)
```

For the SkillsMP client:

```python
from skilly.skillsmp import (
    AsyncSkillsMp,
    ClientSettings,
    SkillsMp,
    SkillsMpAiSearchResult,
    SkillsMpSearchQuery,
    SkillsMpSearchResult,
    SkillsMpSkill,
)
```

## Architecture

-   **`Skill`** — a Rust-backed domain object representing a skill with its
    metadata and file contents.
-   **`SkillRepository`** — stateful orchestrator wrapping a directory and
    project settings.
-   **Discovery functions** — lightweight stateless readers for one-shot use.
-   **Source types** — `PythonSource`, `NodeSource`, `MavenSource` — control
    how project dependencies are scanned.
-   **`FileSystem`** — a protocol for custom filesystem backends.
-   **`SkillsMp`** — synchronous and async clients for the SkillsMP registry.
