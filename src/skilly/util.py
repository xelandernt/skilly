from pathlib import Path

from skilly.filesystem import DEFAULT_FILE_SYSTEM, FileSystem
from skilly.repository import SkillRepository
from skilly.skills import Skill, discover_venv_skills


def get_project_skills(
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    include_dev: bool = False,
) -> list[Skill]:
    """Return dependency skills declared by the current project.

    Args:
        pyproject_toml_path: Path to the project manifest.
        venv_path: Virtual environment to scan for package skills.
        file_system: File system abstraction used for file access.
        include_dev: Whether dev dependencies should be included.

    Returns:
        Skills that exist in both the project manifest and the virtual environment.
    """
    repository = SkillRepository(file_system=file_system)
    return repository.project_skills(
        pyproject_toml_path=pyproject_toml_path,
        venv_path=venv_path,
        include_dev=include_dev,
    )


def get_venv_skills(
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> list[Skill]:
    """Return all skills discovered in a virtual environment.

    Args:
        venv_path: Virtual environment root directory.
        file_system: File system abstraction used for file access.

    Returns:
        Skills discovered under the virtual environment's site-packages directory.
    """
    return discover_venv_skills(venv_path, file_system=file_system)
