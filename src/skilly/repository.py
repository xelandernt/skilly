from __future__ import annotations

import shutil
from collections.abc import Callable, Sequence
from dataclasses import dataclass

from pathlib import Path

from packaging.requirements import Requirement

from . import _bridge as bridge
from .constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from .skills import Skill, discover_installed_skills, discover_venv_skills


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


class SkillRepository:
    def list(self, directory: Path = DEFAULT_SKILLS_PATH) -> list[Skill]:
        return discover_installed_skills(directory)

    def find(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill | None:
        try:
            return self.require(name, directory=directory)
        except FileNotFoundError:
            return None

    def require(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill:
        skills = self.list(directory)
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
    ) -> Skill:
        destination = directory / (skill_name or skill.name)
        if replace and destination.exists():
            shutil.rmtree(destination)
        return skill.install_to(
            directory,
            skill_name=skill_name,
            overwrite=overwrite or replace,
        )

    def remove(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill:
        return bridge.remove_skill(name, directory)

    def project_requirements(
        self,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> Sequence[Requirement]:
        requirements = bridge.project_requirements(
            str(pyproject_toml_path),
            include_dev=include_dev,
            include_extras=list(include_extras),
        )
        return [Requirement(spec) for spec in requirements]

    def project_skills(
        self,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> Sequence[Skill]:
        package_names = {
            requirement.name
            for requirement in self.project_requirements(
                pyproject_toml_path=pyproject_toml_path,
                include_dev=include_dev,
                include_extras=include_extras,
            )
        }
        return [
            skill
            for skill in discover_venv_skills(venv_path)
            if skill.package_name in package_names
        ]

    def scan_project(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> Sequence[SkillMatch]:
        installed = self.list(directory)
        matches = [
            SkillMatch(
                available=skill,
                installed=self.match_installed(installed, skill),
            )
            for skill in self.project_skills(
                pyproject_toml_path=pyproject_toml_path,
                venv_path=venv_path,
                include_dev=include_dev,
                include_extras=include_extras,
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
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> Sequence[SkillMatch]:
        return [
            item
            for item in self.scan_project(
                directory=directory,
                pyproject_toml_path=pyproject_toml_path,
                venv_path=venv_path,
                include_dev=include_dev,
                include_extras=include_extras,
            )
            if item.status is SkillInstallStatus.UPDATABLE
        ]

    def available_dependency_skill(
        self,
        installed_skill: Skill,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> Skill | None:
        for skill in self.project_skills(
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
            include_extras=include_extras,
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
