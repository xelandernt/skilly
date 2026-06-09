from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from . import _bridge as bridge
from .constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from .skills import (
    Skill,
    SkillOrigin,
    discover_github_skills,
    discover_installed_skills,
    github_versions_match,
)

if TYPE_CHECKING:
    from .filesystem import FileSystem
    from .skills import GitHubSkillFetcher


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
class ProjectSettings:
    pyproject_toml_path: Path = Path("pyproject.toml")
    venv_path: Path = Path(".venv")
    include_project_dependencies: bool = True
    include_dependency_groups: bool = True
    include_optional_dependencies: bool = True


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
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
    ) -> ProjectSettings:
        if project is not None:
            return project
        return ProjectSettings(
            pyproject_toml_path=pyproject_toml_path or self.project.pyproject_toml_path,
            venv_path=venv_path or self.project.venv_path,
            include_project_dependencies=self.project.include_project_dependencies
            if include_project_dependencies is None
            else include_project_dependencies,
            include_dependency_groups=self.project.include_dependency_groups
            if include_dependency_groups is None
            else include_dependency_groups,
            include_optional_dependencies=self.project.include_optional_dependencies
            if include_optional_dependencies is None
            else include_optional_dependencies,
        )

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
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[Skill]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_project_dependencies=include_project_dependencies,
            include_dependency_groups=include_dependency_groups,
            include_optional_dependencies=include_optional_dependencies,
        )
        return [
            match.available
            for match in self.scan_project(project=settings, file_system=file_system)
        ]

    def scan_project(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[SkillMatch]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_project_dependencies=include_project_dependencies,
            include_dependency_groups=include_dependency_groups,
            include_optional_dependencies=include_optional_dependencies,
        )
        return [
            SkillMatch(available=available, installed=installed)
            for available, installed in bridge.scan_project(
                self._directory(directory),
                settings.pyproject_toml_path,
                settings.venv_path,
                include_project_dependencies=settings.include_project_dependencies,
                include_dependency_groups=settings.include_dependency_groups,
                include_optional_dependencies=settings.include_optional_dependencies,
                file_system=self._resolve_file_system(file_system),
            )
        ]

    def dependency_updates(
        self,
        *,
        directory: Path | None = None,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[SkillMatch]:
        return [
            item
            for item in self.scan_project(
                directory=directory,
                project=self._project_settings(
                    project=project,
                    pyproject_toml_path=pyproject_toml_path,
                    venv_path=venv_path,
                    include_project_dependencies=include_project_dependencies,
                    include_dependency_groups=include_dependency_groups,
                    include_optional_dependencies=include_optional_dependencies,
                ),
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
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        github_fetcher: GitHubSkillFetcher | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        if installed_skill.is_dependency():
            return self.available_dependency_skill(
                installed_skill,
                directory=directory,
                project=project,
                pyproject_toml_path=pyproject_toml_path,
                venv_path=venv_path,
                include_project_dependencies=include_project_dependencies,
                include_dependency_groups=include_dependency_groups,
                include_optional_dependencies=include_optional_dependencies,
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
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        github_fetcher: GitHubSkillFetcher | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[InstalledSkillUpdate]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_project_dependencies=include_project_dependencies,
            include_dependency_groups=include_dependency_groups,
            include_optional_dependencies=include_optional_dependencies,
        )
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
        pyproject_toml_path: Path | None = None,
        venv_path: Path | None = None,
        include_project_dependencies: bool | None = None,
        include_dependency_groups: bool | None = None,
        include_optional_dependencies: bool | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_project_dependencies=include_project_dependencies,
            include_dependency_groups=include_dependency_groups,
            include_optional_dependencies=include_optional_dependencies,
        )
        return bridge.available_dependency_skill(
            installed_skill,
            self._directory(directory),
            settings.pyproject_toml_path,
            settings.venv_path,
            include_project_dependencies=settings.include_project_dependencies,
            include_dependency_groups=settings.include_dependency_groups,
            include_optional_dependencies=settings.include_optional_dependencies,
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
