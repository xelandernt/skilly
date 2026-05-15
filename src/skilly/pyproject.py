import tomllib
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

from packaging.requirements import Requirement


@dataclass(frozen=True)
class PyProjectInfo:
    """Parsed dependency information from pyproject.toml."""

    dependencies: list[Requirement]

    @classmethod
    def _get_extra_requirements(
        cls,
        toml: Mapping[str, object],
        extras: Sequence[str],
    ) -> list[Requirement]:
        dependency_groups = _mapping_value(toml.get("dependency-groups"))
        extra_requirements = []
        for extra in extras:
            for dep in _string_list_value(dependency_groups.get(extra)):
                extra_requirements.append(Requirement(dep))

        return extra_requirements

    @classmethod
    def from_pyproject_toml(
        cls,
        toml: Mapping[str, object],
        include_dev: bool = False,
        include_extras: Sequence[str] = (),
    ) -> "PyProjectInfo":
        """Parse dependency information from pyproject content.

        Args:
            toml: Parsed pyproject.toml content.
            include_dev: Whether the `dev` dependency group should be included.
            include_extras: Additional dependency groups to include.

        Returns:
            Parsed project dependency information.
        """
        dependencies = []
        project = _mapping_value(toml.get("project"))
        for dep in _string_list_value(project.get("dependencies")):
            dependencies.append(Requirement(dep))

        extras = tuple(include_extras)
        if include_dev:
            extras = (*extras, "dev")

        dependencies.extend(cls._get_extra_requirements(toml, extras))
        return cls(dependencies=dependencies)


def parse_toml(
    path: Path,
) -> dict[str, object]:
    """Read and parse a TOML file.

    Args:
        path: Path to the TOML file.
    Returns:
        Parsed TOML content.
    """
    loaded: object = tomllib.loads(path.read_text(encoding="utf-8"))
    return dict(_mapping_value(loaded))


def _mapping_value(value: object | None) -> Mapping[str, object]:
    if value is None:
        return {}
    if not isinstance(value, Mapping):
        raise TypeError(f"Expected mapping, got {type(value)!r}")
    normalized: dict[str, object] = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise TypeError(f"Expected string key, got {type(key)!r}")
        normalized[key] = item
    return normalized


def _string_list_value(value: object | None) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise TypeError(f"Expected list, got {type(value)!r}")
    normalized: list[str] = []
    for item in value:
        if not isinstance(item, str):
            raise TypeError(f"Expected string, got {type(item)!r}")
        normalized.append(item)
    return normalized
