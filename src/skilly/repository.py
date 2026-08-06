from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, TypeAlias

from . import _bridge as bridge
from .constants import (
    DEFAULT_PYPROJECT_PATH,
    DEFAULT_SKILLS_PATH,
    DEFAULT_VENV_PATH,
    SkillInstallStatus,
)
from .skills import (
    RepositoryDiscoveryClient,
    Skill,
    available_repository_update,
    discover_installed_skills,
)

if TYPE_CHECKING:
    from .filesystem import FileSystem


@dataclass(frozen=True)
class SkillMatch:
    available: Skill
    installed: Skill | None = None

    @property
    def status(self) -> SkillInstallStatus:
        if self.installed is None:
            return SkillInstallStatus.INSTALLABLE
        if self.available.package_version == self.installed.package_version:
            return SkillInstallStatus.INSTALLED
        return SkillInstallStatus.UPDATABLE


@dataclass(frozen=True)
class InstalledSkillUpdate:
    installed: Skill
    available: Skill


@dataclass(frozen=True)
class PythonSource:
    pyproject_toml_path: Path = DEFAULT_PYPROJECT_PATH
    venv_path: Path = DEFAULT_VENV_PATH
    include_project_dependencies: bool = True
    dependency_groups: tuple[str, ...] | None = None
    exclude_dependency_groups: tuple[str, ...] | None = None
    optional_dependencies: tuple[str, ...] | None = None
    exclude_optional_dependencies: tuple[str, ...] | None = None

    def __post_init__(self) -> None:
        if (
            self.dependency_groups is not None
            and self.exclude_dependency_groups is not None
        ):
            raise ValueError(
                "dependency_groups and exclude_dependency_groups cannot both be set"
            )
        if (
            self.optional_dependencies is not None
            and self.exclude_optional_dependencies is not None
        ):
            raise ValueError(
                "optional_dependencies and exclude_optional_dependencies cannot both be set"
            )


@dataclass(frozen=True)
class NodeSource:
    package_json_path: Path = Path("package.json")
    node_modules_path: Path = Path("node_modules")
    include_dependencies: bool = True
    include_dev_dependencies: bool = True
    include_optional_dependencies: bool = True


@dataclass(frozen=True)
class MavenSource:
    pom_xml_path: Path = Path("pom.xml")
    repository_path: Path = Path("~/.m2/repository").expanduser()
    include_compile_scope: bool = True
    include_runtime_scope: bool = True
    include_provided_scope: bool = False
    include_test_scope: bool = True
    include_system_scope: bool = False


PackageSource: TypeAlias = PythonSource | NodeSource | MavenSource


@dataclass(frozen=True)
class ProjectSettings:
    sources: tuple[PackageSource, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.sources, tuple):
            object.__setattr__(self, "sources", tuple(self.sources))

    @staticmethod
    def defaults() -> "ProjectSettings":
        return ProjectSettings(sources=(PythonSource(), NodeSource(), MavenSource()))


class SkillRepository:
    def __init__(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
        discovery_client: RepositoryDiscoveryClient | None = None,
    ) -> None:
        self.directory = directory
        self.project = project if project is not None else ProjectSettings.defaults()
        self._file_system = file_system
        self._discovery_client = discovery_client

    def _directory(self, directory: Path | None) -> Path:
        return self.directory if directory is None else directory

    def _resolve_file_system(
        self, file_system: FileSystem | None = None
    ) -> FileSystem | None:
        return file_system if file_system is not None else self._file_system

    def _project_settings(
        self,
        *,
        project: ProjectSettings | None = None,
    ) -> ProjectSettings:
        if project is not None:
            return project
        return self.project

    def list(
        self,
        directory: Path | None = None,
        *,
        file_system: FileSystem | None = None,
    ) -> list[Skill]:
        return discover_installed_skills(
            self._directory(directory),
            file_system=self._resolve_file_system(file_system),
        )

    def find(
        self,
        name: str,
        *,
        directory: Path | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        try:
            return self.require(name, directory=directory, file_system=file_system)
        except FileNotFoundError:
            return None

    def require(
        self,
        name: str,
        *,
        directory: Path | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill:
        skills = self.list(directory, file_system=file_system)
        for skill in skills:
            if skill.directory_name == name:
                return skill

        matches = [skill for skill in skills if skill.name == name]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            raise ValueError(f"Multiple installed skills match name: {name}")
        raise FileNotFoundError(f"Installed skill not found: {name}")

    def install(
        self,
        skill: Skill,
        *,
        directory: Path | None = None,
        skill_name: str | None = None,
        overwrite: bool = False,
        replace: bool = False,
        file_system: FileSystem | None = None,
    ) -> Skill:
        resolved_file_system = self._resolve_file_system(file_system)
        if replace:
            return skill.replace_to(
                self._directory(directory),
                skill_name=skill_name,
                file_system=resolved_file_system,
            )
        return skill.install_to(
            self._directory(directory),
            skill_name=skill_name,
            overwrite=overwrite,
            file_system=resolved_file_system,
        )

    def remove(
        self,
        name: str,
        *,
        directory: Path | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill:
        return bridge.remove_skill(
            name,
            self._directory(directory),
            file_system=self._resolve_file_system(file_system),
        )

    def project_skills(
        self,
        *,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[Skill]:
        return [
            match.available
            for match in self.scan_project(project=project, file_system=file_system)
        ]

    def scan_project(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[SkillMatch]:
        settings = project or self.project
        serialized_sources = _serialize_sources(settings.sources)
        return [
            SkillMatch(available=available, installed=installed)
            for available, installed in bridge.scan_project(
                self._directory(directory),
                sources=serialized_sources,
                file_system=self._resolve_file_system(file_system),
            )
        ]

    def dependency_updates(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[SkillMatch]:
        return [
            item
            for item in self.scan_project(
                directory=directory,
                project=self._project_settings(project=project),
                file_system=file_system,
            )
            if item.status is SkillInstallStatus.UPDATABLE
        ]

    def available_update(
        self,
        installed_skill: Skill,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        if installed_skill.is_dependency():
            return self.available_dependency_skill(
                installed_skill,
                directory=directory,
                project=project,
                file_system=file_system,
            )

        if (
            installed_skill.repository_url is not None
            and installed_skill.repository_provider is not None
        ):
            if self._discovery_client is None:
                raise RuntimeError(
                    "repository updates require a RepositoryDiscoveryClient"
                )
            discovered = self._discovery_client.discover(
                installed_skill.repository_url,
                provider=installed_skill.repository_provider,
            )
            if not discovered:
                raise ValueError(
                    "Repository URL resolves to no skills: "
                    f"{installed_skill.repository_url}"
                )
            return available_repository_update(installed_skill, discovered)

        return None

    def updates(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[InstalledSkillUpdate]:
        settings = self._project_settings(project=project)
        updates: list[InstalledSkillUpdate] = []
        for installed in self.list(directory, file_system=file_system):
            if not installed.can_update():
                continue
            available = self.available_update(
                installed,
                directory=directory,
                project=settings,
                file_system=file_system,
            )
            if available is None:
                continue
            updates.append(
                InstalledSkillUpdate(installed=installed, available=available)
            )
        return sorted(updates, key=lambda item: item.installed.directory_name)

    def available_dependency_skill(
        self,
        installed_skill: Skill,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        settings = self._project_settings(project=project)
        serialized_sources = _serialize_sources(settings.sources)
        return bridge.available_dependency_skill(
            installed_skill,
            self._directory(directory),
            sources=serialized_sources,
            file_system=self._resolve_file_system(file_system),
        )

    def match_installed(
        self,
        installed_skills: Sequence[Skill],
        available_skill: Skill,
        *,
        candidate_filter: Callable[[Skill], bool] | None = None,
    ) -> Skill | None:
        for installed_skill in installed_skills:
            if candidate_filter is not None and not candidate_filter(installed_skill):
                continue
            if available_skill.matches(installed_skill):
                return installed_skill
        return None


def _serialize_sources(
    sources: tuple[PackageSource, ...],
) -> list[dict[str, object]]:
    result: list[dict[str, object]] = []
    for source in sources:
        if isinstance(source, PythonSource):
            result.append(
                {
                    "kind": "python",
                    "pyproject_toml_path": str(source.pyproject_toml_path),
                    "venv_path": str(source.venv_path),
                    "include_project_dependencies": source.include_project_dependencies,
                    "dependency_groups": (
                        source.dependency_groups
                        if source.dependency_groups is not None
                        and len(source.dependency_groups) > 0
                        else None
                    ),
                    "exclude_dependency_groups": (
                        source.exclude_dependency_groups
                        if source.exclude_dependency_groups is not None
                        and len(source.exclude_dependency_groups) > 0
                        else None
                    ),
                    "optional_dependencies": (
                        source.optional_dependencies
                        if source.optional_dependencies is not None
                        and len(source.optional_dependencies) > 0
                        else None
                    ),
                    "exclude_optional_dependencies": (
                        source.exclude_optional_dependencies
                        if source.exclude_optional_dependencies is not None
                        and len(source.exclude_optional_dependencies) > 0
                        else None
                    ),
                }
            )
        elif isinstance(source, NodeSource):
            result.append(
                {
                    "kind": "node",
                    "package_json_path": str(source.package_json_path),
                    "node_modules_path": str(source.node_modules_path),
                    "include_dependencies": source.include_dependencies,
                    "include_dev_dependencies": source.include_dev_dependencies,
                    "include_optional_dependencies": source.include_optional_dependencies,
                }
            )
        elif isinstance(source, MavenSource):
            result.append(
                {
                    "kind": "maven",
                    "pom_xml_path": str(source.pom_xml_path),
                    "repository_path": str(source.repository_path),
                    "include_compile_scope": source.include_compile_scope,
                    "include_runtime_scope": source.include_runtime_scope,
                    "include_provided_scope": source.include_provided_scope,
                    "include_test_scope": source.include_test_scope,
                    "include_system_scope": source.include_system_scope,
                }
            )
    return result
