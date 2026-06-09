from __future__ import annotations

import shutil
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from packaging.requirements import Requirement

from . import _bridge as bridge
from .constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from .skills import (
    Skill,
    SkillOrigin,
    discover_github_skills,
    discover_installed_skills,
    discover_venv_skills,
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
    include_dev: bool = False
    include_extras: Sequence[str] = ()


class SkillRepository:
    def __init__(self, *, file_system: FileSystem | None = None) -> None:
        self._file_system = file_system

    def _resolve_file_system(
        self, file_system: FileSystem | None = None
    ) -> FileSystem | None:
        return file_system if file_system is not None else self._file_system

    def _project_settings(
        self,
        *,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> ProjectSettings:
        if project is not None:
            return project
        return ProjectSettings(
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=tuple(include_extras),
        )

    def list(
        self,
        directory: Path = DEFAULT_SKILLS_PATH,
        *,
        file_system: FileSystem | None = None,
    ) -> list[Skill]:
        return discover_installed_skills(
            directory, file_system=self._resolve_file_system(file_system)
        )

    def find(
        self,
        name: str,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
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
        directory: Path = DEFAULT_SKILLS_PATH,
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
        directory: Path = DEFAULT_SKILLS_PATH,
        skill_name: str | None = None,
        overwrite: bool = False,
        replace: bool = False,
        file_system: FileSystem | None = None,
    ) -> Skill:
        resolved_file_system = self._resolve_file_system(file_system)
        destination = directory / (skill_name or skill.name)
        if replace:
            if resolved_file_system is not None:
                if resolved_file_system.exists(destination):
                    resolved_file_system.remove_tree(destination)
            elif destination.exists():
                shutil.rmtree(destination)
        return skill.install_to(
            directory,
            skill_name=skill_name,
            overwrite=overwrite or replace,
            file_system=resolved_file_system,
        )

    def remove(
        self,
        name: str,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        file_system: FileSystem | None = None,
    ) -> Skill:
        return bridge.remove_skill(
            name,
            directory,
            file_system=self._resolve_file_system(file_system),
        )

    def project_requirements(
        self,
        *,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        file_system: FileSystem | None = None,
    ) -> Sequence[Requirement]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            include_dev=include_dev,
            include_extras=include_extras,
        )
        requirements = bridge.project_requirements(
            str(settings.pyproject_toml_path),
            include_dev=settings.include_dev,
            include_extras=list(settings.include_extras),
            file_system=self._resolve_file_system(file_system),
        )
        return [Requirement(spec) for spec in requirements]

    def project_skills(
        self,
        *,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        file_system: FileSystem | None = None,
    ) -> Sequence[Skill]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=include_extras,
        )
        package_names = {
            requirement.name
            for requirement in self.project_requirements(
                project=settings,
                file_system=file_system,
            )
        }
        return [
            skill
            for skill in discover_venv_skills(
                settings.venv_path, file_system=self._resolve_file_system(file_system)
            )
            if skill.package_name in package_names
        ]

    def scan_project(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        file_system: FileSystem | None = None,
    ) -> Sequence[SkillMatch]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=include_extras,
        )
        installed = self.list(directory, file_system=file_system)
        matches = [
            SkillMatch(
                available=skill,
                installed=self.match_installed(installed, skill),
            )
            for skill in self.project_skills(
                project=settings,
                file_system=file_system,
            )
        ]
        return sorted(
            matches,
            key=lambda item: (
                item.available.package_name or "",
                item.available.name,
                item.available.package_version or "",
            ),
        )

    def dependency_updates(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
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
                    include_dev=include_dev,
                    include_extras=include_extras,
                ),
                file_system=file_system,
            )
            if item.status is SkillInstallStatus.UPDATABLE
        ]

    def available_update(
        self,
        installed_skill: Skill,
        *,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        github_fetcher: GitHubSkillFetcher | None = None,
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        if installed_skill.is_dependency():
            return self.available_dependency_skill(
                installed_skill,
                project=project,
                pyproject_toml_path=pyproject_toml_path,
                venv_path=venv_path,
                include_dev=include_dev,
                include_extras=include_extras,
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
        directory: Path = DEFAULT_SKILLS_PATH,
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        github_fetcher: GitHubSkillFetcher | None = None,
        file_system: FileSystem | None = None,
    ) -> Sequence[InstalledSkillUpdate]:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=include_extras,
        )
        updates: list[InstalledSkillUpdate] = []
        for installed in self.list(directory, file_system=file_system):
            if not installed.can_update():
                continue
            available = self.available_update(
                installed,
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
        project: ProjectSettings | None = None,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
        file_system: FileSystem | None = None,
    ) -> Skill | None:
        settings = self._project_settings(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=include_extras,
        )
        for skill in self.project_skills(
            project=settings,
            file_system=file_system,
        ):
            if skill.matches(installed_skill):
                return skill
        return None

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
