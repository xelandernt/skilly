from typing import Sequence
from pathlib import Path
from cyclopts import App

util_cli = App("util")


@util_cli.command()
def dependencies(
    file: Path = Path("pyproject.toml"), dev: bool = False, extras: Sequence[str] = ()
) -> None:
    # Todo: Sequence[str] might be wrong here for now
    from libi.parsers import parse_toml, PyProjectInfo

    toml = parse_toml(file)
    info = PyProjectInfo.from_pyproject_toml(
        toml, include_dev=dev, include_extras=extras
    )
    for dep in info.dependencies:
        print(dep.name)


@util_cli.command()
def venv(path: Path = Path(".venv")) -> None:

    from libi.skills import VenvSkills

    skills = VenvSkills.from_dir(path)
    for skill in skills.skills:
        print(
            f"{skill.skill.name}[{skill.package_name}=={skill.package_version}]: {skill.skill.description}"
        )
