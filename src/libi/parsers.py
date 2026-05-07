import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

from packaging.requirements import Requirement

from .filesystem import DEFAULT_FILE_SYSTEM, FileSystem


@dataclass(frozen=True)
class PyProjectInfo:
    dependencies: list[Requirement]

    @classmethod
    def _get_extra_requirements(
        cls,
        toml: dict[str, Any],
        extras: Sequence[str],
    ) -> list[Requirement]:
        dependency_groups = toml.get("dependency-groups", {})
        extra_requirements = []
        for extra in dependency_groups.keys():
            if extra in extras:
                for dep in dependency_groups[extra]:
                    extra_requirements.append(Requirement(dep))

        return extra_requirements

    @classmethod
    def from_pyproject_toml(
        cls,
        toml: dict[str, Any],
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> "PyProjectInfo":
        dependencies = []
        for dep in toml.get("project", {}).get("dependencies", []):
            dependencies.append(Requirement(dep))

        extras = set(include_extras)
        if include_dev:
            extras.add("dev")

        dependencies.extend(cls._get_extra_requirements(toml, extras))
        return cls(dependencies=dependencies)


def parse_toml(
    path: Path,
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> dict[str, Any]:
    data = file_system.read_file(path)
    return tomllib.loads(data)
