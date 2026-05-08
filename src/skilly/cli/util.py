from typing import Sequence
from pathlib import Path
from cyclopts import App

util_cli = App("util")


@util_cli.command()
def dependencies(
    file: Path = Path("pyproject.toml"), dev: bool = False, extras: Sequence[str] = ()
) -> None:
    # Todo: Sequence[str] might be wrong here for now
    from skilly.parsers import parse_toml, PyProjectInfo

    toml = parse_toml(file)
    info = PyProjectInfo.from_pyproject_toml(
        toml, include_dev=dev, include_extras=extras
    )
    for dep in info.dependencies:
        print(dep.name)


@util_cli.command()
def venv(path: Path = Path(".venv"), detailed: bool = False) -> None:

    from skilly.skills import VenvSkills

    skills = VenvSkills.from_dir(path)

    print(f"Found {len(skills.skills)} skills:")
    for skill in skills.skills:
        print(
            f"{skill.skill.name}[{skill.package_name}=={skill.package_version}]:\n{skill.skill.description}"
        )
        if detailed:
            print("\tResources:")
            for resource in skill.skill.resources:
                content_length = len(resource.content.split("\n"))
                print(
                    f"\t\t{resource.relative_path} [{resource.kind}]: {content_length} lines."
                )
