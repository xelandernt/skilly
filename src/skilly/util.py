from pathlib import Path
from typing import List

from skilly.filesystem import FileSystem, DEFAULT_FILE_SYSTEM
from skilly.parsers import parse_toml, PyProjectInfo
from skilly.skills import DiscoveredSkill, Skill, VenvSkills


def get_project_discovered_skills(
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    include_dev: bool = False,
) -> List[DiscoveredSkill]:
    """
    Get discovered skills matching project dependencies.
    Args:
        include_dev: whether to include dev dependencies
        pyproject_toml_path: path to `pyproject.toml`
        venv_path: path to virtual environment
        file_system: file system to use.

    Returns:
        list of discovered skills in the virtual environment matching `pyproject.toml`.
    """
    toml = parse_toml(pyproject_toml_path, file_system=file_system)
    info = PyProjectInfo.from_pyproject_toml(toml, include_dev=include_dev)
    venv_skills = VenvSkills.from_dir(venv_path, file_system=file_system)
    return list(venv_skills.filter_skills(info.dependencies))


def get_project_skills(
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> List[Skill]:
    """
    Get skills matching project dependencies.
    Args:
        pyproject_toml_path: path to `pyproject.toml`
        venv_path: path to virtual environment
        file_system: file system to use.

    Returns:
        list of skills in the virtual environment matching `pyproject.toml`.
    """
    return [
        discovered_skill.skill
        for discovered_skill in get_project_discovered_skills(
            pyproject_toml_path=pyproject_toml_path,
            venv_path=venv_path,
            file_system=file_system,
        )
    ]


def get_venv_skills(
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> List[Skill]:
    """
    Get skills in the virtual environment.
    Args:
        venv_path: path to virtual environment
        file_system: file system to use.

    Returns:
        list of skills in the virtual environment.

    """
    venv_skills = VenvSkills.from_dir(venv_path, file_system=file_system)
    return [s.skill for s in venv_skills.skills]
