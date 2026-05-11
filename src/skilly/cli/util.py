from typing import Sequence
from pathlib import Path
from cyclopts import App

util_cli = App("util", help="Useful utilities")


@util_cli.command()
def dependencies(
    file: Path = Path("pyproject.toml"), dev: bool = False, extras: Sequence[str] = ()
) -> None:
    # Todo: Sequence[str] might be wrong here for now
    from skilly.pyproject import parse_toml, PyProjectInfo

    toml = parse_toml(file)
    info = PyProjectInfo.from_pyproject_toml(
        toml, include_dev=dev, include_extras=extras
    )
    for dep in info.dependencies:
        print(dep.name)


@util_cli.command()
def venv(path: Path = Path(".venv"), detailed: bool = False) -> None:
    from skilly.skills import discover_venv_skills

    skills = discover_venv_skills(path)

    print(f"Found {len(skills)} skills:")
    for skill in skills:
        package_reference = skill.package_reference() or "unknown"
        print(f"{skill.name}[{package_reference}]:\n{skill.description}")
        if detailed:
            print("\tResources:")
            for resource in skill.resources:
                content_length = len(resource.content.split("\n"))
                print(
                    f"\t\t{resource.relative_path} [{resource.kind}]: {content_length} lines."
                )
