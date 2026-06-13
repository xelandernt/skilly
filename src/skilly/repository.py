from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from . import _bridge as bridge
from .constants import (
    DEFAULT_PYPROJECT_PATH,
    DEFAULT_SKILLS_PATH,
    DEFAULT_VENV_PATH,
    SkillInstallStatus,
)
from .skills import (
    Skill,
    SkillOrigin,
    discover_github_skills,
    discover_installed_skills,
    github_versions_match,
)

if TYPE_CHECKING:
    from ._bridge import ClientConfigSource
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
class PythonProjectSettings:
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
class NodeProjectSettings:
    package_json_path: Path = Path("package.json")
    node_modules_path: Path = Path("node_modules")
    include_dependencies: bool = True
    include_dev_dependencies: bool = True
    include_optional_dependencies: bool = True


@dataclass(frozen=True)
class ProjectSettings:
    python: PythonProjectSettings | None = None
    node: NodeProjectSettings | None = None

    def __post_init__(self) -> None:
        if self.python is None and self.node is None:
            object.__setattr__(self, "python", PythonProjectSettings())
            object.__setattr__(self, "node", NodeProjectSettings())


class SkillRepository:
    def __init__(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        project: ProjectSettings = ProjectSettings(),
        file_system: FileSystem | None = None,
    ) -> None:
        self.directory = directory
        self.project = project
        self._file_system = file_system

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
        return [
            SkillMatch(available=available, installed=installed)
            for available, installed in bridge.scan_project(
                self._directory(directory),
                pyproject_toml_path=settings.python.pyproject_toml_path
                if settings.python
                else Path("pyproject.toml"),
                venv_path=settings.python.venv_path
                if settings.python
                else Path(".venv"),
                include_project_dependencies=settings.python.include_project_dependencies
                if settings.python
                else False,
                dependency_groups=(
                    None
                    if settings.python is None
                    or settings.python.dependency_groups is None
                    else list(settings.python.dependency_groups)
                ),
                exclude_dependency_groups=(
                    None
                    if settings.python is None
                    or settings.python.exclude_dependency_groups is None
                    else list(settings.python.exclude_dependency_groups)
                ),
                optional_dependencies=(
                    None
                    if settings.python is None
                    or settings.python.optional_dependencies is None
                    else list(settings.python.optional_dependencies)
                ),
                exclude_optional_dependencies=(
                    None
                    if settings.python is None
                    or settings.python.exclude_optional_dependencies is None
                    else list(settings.python.exclude_optional_dependencies)
                ),
                package_json_path=settings.node.package_json_path
                if settings.node
                else Path("package.json"),
                node_modules_path=settings.node.node_modules_path
                if settings.node
                else Path("node_modules"),
                include_node_dependencies=settings.node.include_dependencies
                if settings.node
                else False,
                include_node_dev_dependencies=settings.node.include_dev_dependencies
                if settings.node
                else False,
                include_node_optional_dependencies=settings.node.include_optional_dependencies
                if settings.node
                else False,
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
        github_fetcher: ClientConfigSource | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        if installed_skill.is_dependency():
            return self.available_dependency_skill(
                installed_skill,
                directory=directory,
                project=project,
                file_system=file_system,
            )

        if installed_skill.github_url is None or github_fetcher is None:
            return None

        discovered = discover_github_skills(
            github_fetcher,
            installed_skill.github_url,
            origin=SkillOrigin(
                source="skillsmp"
                if installed_skill.is_skillsmp()
                else installed_skill.source,
                skillsmp_id=installed_skill.skillsmp_id,
            ),
        )
        if not discovered:
            raise ValueError(
                f"GitHub URL resolves to no skills: {installed_skill.github_url}"
            )

        refreshed = discovered[0]
        if github_versions_match(installed_skill, refreshed):
            return None
        return refreshed

    def updates(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        github_fetcher: ClientConfigSource | None = None,
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
                github_fetcher=github_fetcher,
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
        return bridge.available_dependency_skill(
            installed_skill,
            self._directory(directory),
            pyproject_toml_path=settings.python.pyproject_toml_path
            if settings.python
            else Path("pyproject.toml"),
            venv_path=settings.python.venv_path if settings.python else Path(".venv"),
            include_project_dependencies=settings.python.include_project_dependencies
            if settings.python
            else False,
            dependency_groups=(
                None
                if settings.python is None or settings.python.dependency_groups is None
                else list(settings.python.dependency_groups)
            ),
            exclude_dependency_groups=(
                None
                if settings.python is None
                or settings.python.exclude_dependency_groups is None
                else list(settings.python.exclude_dependency_groups)
            ),
            optional_dependencies=(
                None
                if settings.python is None
                or settings.python.optional_dependencies is None
                else list(settings.python.optional_dependencies)
            ),
            exclude_optional_dependencies=(
                None
                if settings.python is None
                or settings.python.exclude_optional_dependencies is None
                else list(settings.python.exclude_optional_dependencies)
            ),
            package_json_path=settings.node.package_json_path
            if settings.node
            else Path("package.json"),
            node_modules_path=settings.node.node_modules_path
            if settings.node
            else Path("node_modules"),
            include_node_dependencies=settings.node.include_dependencies
            if settings.node
            else False,
            include_node_dev_dependencies=settings.node.include_dev_dependencies
            if settings.node
            else False,
            include_node_optional_dependencies=settings.node.include_optional_dependencies
            if settings.node
            else False,
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
