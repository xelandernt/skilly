from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path

from packaging.requirements import Requirement

from .constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from .filesystem import DEFAULT_FILE_SYSTEM, FileSystem
from .pyproject import PyProjectInfo, parse_toml
from .skills import Skill, discover_installed_skills, discover_venv_skills


@dataclass(frozen=True)
class SkillMatch:
    """A comparison between an available skill and an installed one."""

    available: Skill
    installed: Skill | None = None

    @property
    def status(self) -> SkillInstallStatus:
        """Return the install status for this available skill."""
        if self.installed is None:
            return SkillInstallStatus.INSTALLABLE
        if self.available.package_version == self.installed.package_version:
            return SkillInstallStatus.INSTALLED
        return SkillInstallStatus.UPDATABLE


@dataclass(frozen=True)
class SkillRepository:
    """High-level orchestration for installing, discovering, and matching skills."""

    file_system: FileSystem = DEFAULT_FILE_SYSTEM

    def list(self, directory: Path = DEFAULT_SKILLS_PATH) -> list[Skill]:
        """List installed skills from a target directory.

        Args:
            directory: Root directory containing installed skills.

        Returns:
            The installed skills in that directory.
        """
        return discover_installed_skills(directory, file_system=self.file_system)

    def find(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill | None:
        """Find an installed skill by directory name or unique skill name.

        Args:
            name: Directory name or unique skill name.
            directory: Root directory containing installed skills.

        Returns:
            The matching skill, or None when no skill matches.
        """
        try:
            return self.require(name, directory=directory)
        except FileNotFoundError:
            return None

    def require(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill:
        """Require an installed skill by directory name or unique skill name.

        Args:
            name: Directory name or unique skill name.
            directory: Root directory containing installed skills.

        Returns:
            The matching installed skill.

        Raises:
            FileNotFoundError: If no installed skill matches.
            ValueError: If multiple installed skills share the requested name.
        """
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
        """Install a skill into the managed skills directory.

        Args:
            skill: Skill to install.
            directory: Root directory where skills are installed.
            skill_name: Optional destination directory name override.
            overwrite: Whether existing files may be overwritten.
            replace: Whether to remove an existing installed directory first.

        Returns:
            The installed skill reloaded from disk.
        """
        destination = self.file_system.resolve(directory / (skill_name or skill.name))
        if replace and self.file_system.exists(destination):
            self.file_system.remove_tree(destination)
        return skill.install_to(
            directory,
            skill_name=skill_name,
            overwrite=overwrite or replace,
            file_system=self.file_system,
        )

    def remove(self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH) -> Skill:
        """Remove an installed skill.

        Args:
            name: Directory name or unique skill name.
            directory: Root directory containing installed skills.

        Returns:
            The removed skill.
        """
        skill = self.require(name, directory=directory)
        self.file_system.remove_tree(skill.directory)
        return skill

    def project_requirements(
        self,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        include_dev: bool = False,
    ) -> Sequence[Requirement]:
        """Read package requirements from pyproject.toml.

        Args:
            pyproject_toml_path: Path to the project manifest.
            include_dev: Whether dev dependencies should be included.

        Returns:
            Parsed dependency requirements.
        """
        pyproject = parse_toml(pyproject_toml_path, file_system=self.file_system)
        return PyProjectInfo.from_pyproject_toml(
            pyproject,
            include_dev=include_dev,
        ).dependencies

    def project_skills(
        self,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
    ) -> list[Skill]:
        """Return dependency skills declared by the current project.

        Args:
            pyproject_toml_path: Path to the project manifest.
            venv_path: Virtual environment to scan for package skills.
            include_dev: Whether dev dependencies should be included.

        Returns:
            Skills that are both present in the virtual environment and declared in
            the project manifest.
        """
        package_names = {
            requirement.name
            for requirement in self.project_requirements(
                pyproject_toml_path=pyproject_toml_path,
                include_dev=include_dev,
            )
        }
        return [
            skill
            for skill in discover_venv_skills(venv_path, file_system=self.file_system)
            if skill.package_name in package_names
        ]

    def scan_project(
        self,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
    ) -> list[SkillMatch]:
        """Scan project dependencies and classify available skills.

        Args:
            directory: Root directory containing installed skills.
            pyproject_toml_path: Path to the project manifest.
            venv_path: Virtual environment to scan.
            include_dev: Whether dev dependencies should be included.

        Returns:
            Matching results classified as installed, installable, or updatable.
        """
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
    ) -> list[SkillMatch]:
        """Return dependency skills with newer available versions."""
        return [
            match
            for match in self.scan_project(
                directory=directory,
                pyproject_toml_path=pyproject_toml_path,
                venv_path=venv_path,
                include_dev=include_dev,
            )
            if match.status is SkillInstallStatus.UPDATABLE
        ]

    def available_dependency_skill(
        self,
        installed_skill: Skill,
        *,
        pyproject_toml_path: Path = Path("pyproject.toml"),
        venv_path: Path = Path(".venv"),
        include_dev: bool = False,
    ) -> Skill | None:
        """Return the currently available dependency-backed version of a skill."""
        for skill in self.project_skills(
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
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
        """Match an available skill against installed skills."""
        for installed_skill in installed_skills:
            if candidate_filter is not None and not candidate_filter(installed_skill):
                continue
            if available_skill.matches(installed_skill):
                return installed_skill
        return None
