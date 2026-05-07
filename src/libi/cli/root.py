from pathlib import Path

from cyclopts import App
from libi.cli.util import util_cli


cli = App()

cli.command(util_cli)


@cli.command()
def list() -> None:
    from libi.parsers import parse_toml
    from libi.parsers import PyProjectInfo
    from libi.skills import VenvSkills

    toml = parse_toml(Path("pyproject.toml"))
    info = PyProjectInfo.from_pyproject_toml(toml)
    venv_skills = VenvSkills.from_dir(Path(".venv"))
    for skill in venv_skills.filter_skills(info.dependencies):
        print(f"{skill.skill.name}: {skill.skill.description}")
