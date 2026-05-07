from pathlib import Path
from typing import List

from skilly.filesystem import FileSystem, DEFAULT_FILE_SYSTEM
from skilly.parsers import parse_toml, PyProjectInfo
from skilly.skills import Skill, VenvSkills


def get_project_skills(
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> List[Skill]:
    toml = parse_toml(pyproject_toml_path, file_system=file_system)
    info = PyProjectInfo.from_pyproject_toml(toml)
    venv_skills = VenvSkills.from_dir(venv_path, file_system=file_system)
    return [s.skill for s in venv_skills.filter_skills(info.dependencies)]
