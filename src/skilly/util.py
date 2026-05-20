from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from skilly.constants import DEFAULT_SKILLS_PATH
from skilly.repository import ProjectSettings, SkillRepository
from skilly.skills import Skill, discover_installed_skills, discover_venv_skills

if TYPE_CHECKING:
    from skilly.filesystem import FileSystem


def get_project_skills(
    project: ProjectSettings | None = None,
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    include_dev: bool = False,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    repository = SkillRepository(file_system=file_system)
    return list(
        repository.project_skills(
            project=project,
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            include_dev=include_dev,
        )
    )


def get_venv_skills(
    venv_path: Path = Path(".venv"),
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return discover_venv_skills(venv_path, file_system=file_system)


def get_skills_from_directory(
    directory: Path = DEFAULT_SKILLS_PATH,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return discover_installed_skills(directory, file_system=file_system)
