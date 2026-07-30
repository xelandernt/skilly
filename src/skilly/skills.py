from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Any, Literal, TypeAlias

from . import _bridge as bridge
from ._core import Skill
from .constants import DEFAULT_SKILLS_PATH

if TYPE_CHECKING:
    from ._core import RepositoryLocationData
    from .filesystem import FileSystem
    from .repository import PackageSource


ResourceKind: TypeAlias = Literal["script", "reference", "asset", "other"]


class SkillBundleError(ValueError):
    """Raised when :meth:`Skill.from_bundle` receives an invalid bundle."""

    code: str
    path: str
    field: str | None


@dataclass(frozen=True)
class SkillResource:
    relative_path: PurePosixPath
    kind: ResourceKind
    raw: bytes = b""

    @property
    def text(self) -> str:
        return self.raw.decode("utf-8")

    def is_text(self) -> bool:
        try:
            self.raw.decode("utf-8")
        except UnicodeDecodeError:
            return False
        return True


@dataclass(frozen=True)
class RepositoryLocation:
    provider: Literal["github", "bitbucket-cloud", "bitbucket-data-center"]
    base_url: str
    namespace: str
    repo: str
    ref: str | None
    path: PurePosixPath
    url: str

    @classmethod
    def from_data(cls, data: RepositoryLocationData) -> "RepositoryLocation":
        return cls(
            provider=data["provider"],
            base_url=data["base_url"],
            namespace=data["namespace"],
            repo=data["repo"],
            ref=data.get("ref"),
            path=PurePosixPath(data["path"]),
            url=data["url"],
        )


def discover_installed_skills(
    directory: Path = DEFAULT_SKILLS_PATH,
    *,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return bridge.discover_installed_skills(directory, file_system=file_system)


def resolve_skills_directory(
    agent: Literal["agents", "claude", "codex", "copilot"] = "agents",
    *,
    global_: bool = False,
) -> Path:
    return Path(bridge.resolve_skills_directory(agent, global_=global_))


def _source_to_dict(source: PackageSource) -> dict[str, Any]:
    from .repository import MavenSource, NodeSource, PythonSource

    if isinstance(source, PythonSource):
        return {
            "kind": "python",
            "venv_path": str(source.venv_path),
        }
    if isinstance(source, NodeSource):
        return {
            "kind": "node",
            "node_modules_path": str(source.node_modules_path),
        }
    if isinstance(source, MavenSource):
        return {
            "kind": "maven",
            "pom_xml_path": str(source.pom_xml_path),
            "repository_path": str(source.repository_path),
        }
    raise TypeError(f"unsupported source type: {type(source).__name__}")


def discover_package_source_skills(
    source: PackageSource,
    *,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return bridge.discover_package_source_skills(
        _source_to_dict(source), file_system=file_system
    )


def discover_repository_skills(
    repository_url: str,
    *,
    provider: Literal["github", "bitbucket-cloud", "bitbucket-data-center"]
    | None = None,
    token: str | None = None,
) -> list[Skill]:
    return bridge.discover_repository_skills(
        repository_url,
        provider=provider,
        token=token,
    )


def parse_repository_location(
    repository_url: str,
    *,
    provider: Literal["github", "bitbucket-cloud", "bitbucket-data-center"]
    | None = None,
) -> RepositoryLocation:
    return RepositoryLocation.from_data(
        bridge.parse_repository_location(repository_url, provider=provider)
    )


def repository_versions_match(installed: Skill, available: Skill) -> bool:
    return bool(bridge.repository_versions_match(installed, available))
