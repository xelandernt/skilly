from pathlib import Path

from skilly.constants import DEFAULT_SKILLS_PATH
from skilly.repository import ProjectSettings, SkillRepository
from skilly.skills import Skill, discover_venv_skills


def get_project_skills(
    project: ProjectSettings | None = None,
    pyproject_toml_path: Path = Path("pyproject.toml"),
    venv_path: Path = Path(".venv"),
    include_dev: bool = False,
) -> list[Skill]:
    repository = SkillRepository()
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
) -> list[Skill]:
    return discover_venv_skills(venv_path)


def get_skills_from_directory(
    directory: Path = DEFAULT_SKILLS_PATH,
) -> list[Skill]:
    repository = SkillRepository()
    return repository.list(directory)
