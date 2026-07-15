from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Any, Literal, Protocol, TypeAlias

from . import _bridge as bridge
from ._core import Skill
from .constants import DEFAULT_SKILLS_PATH

if TYPE_CHECKING:
    from ._core import (
        GitHubContentItemData,
        GitHubFileBlobData,
        GitHubRepositorySnapshotData,
        GitHubSkillLocationData,
        RepositoryLocationData,
    )
    from .filesystem import FileSystem
    from .repository import PackageSource


ResourceKind: TypeAlias = Literal["script", "reference", "asset", "other"]


class SkillsMpInstallableSkill(Protocol):
    @property
    def id(self) -> str: ...

    @property
    def github_url(self) -> str: ...


@dataclass(frozen=True)
class SkillOrigin:
    source: str | None = None
    package_name: str | None = None
    package_version: str | None = None
    package_ecosystem: str | None = None
    github_url: str | None = None
    github_commit_sha: str | None = None
    skillsmp_id: str | None = None


@dataclass(frozen=True)
class SkillResource:
    relative_path: PurePosixPath
    kind: ResourceKind
    content: bytes = b""


@dataclass(frozen=True)
class GitHubSkillLocation:
    owner: str
    repo: str
    ref: str | None
    path: PurePosixPath
    url: str

    @property
    def skill_name(self) -> str:
        return self.path.name if str(self.path) not in {"", "."} else self.repo

    @classmethod
    def from_data(cls, data: GitHubSkillLocationData) -> "GitHubSkillLocation":
        return cls(
            owner=data["owner"],
            repo=data["repo"],
            ref=None if data.get("ref") is None else data["ref"],
            path=PurePosixPath(data.get("path", ".")),
            url=data["url"],
        )


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


@dataclass(frozen=True)
class GitHubContentItem:
    type: str
    name: str
    path: PurePosixPath
    commit_sha: str | None = None

    @classmethod
    def from_data(cls, data: GitHubContentItemData) -> "GitHubContentItem":
        return cls(
            type=data["type"],
            name=data["name"],
            path=PurePosixPath(data["path"]),
            commit_sha=None if data.get("commit_sha") is None else data["commit_sha"],
        )


@dataclass(frozen=True)
class GitHubFileBlob:
    path: PurePosixPath
    content: str
    size: int
    commit_sha: str | None = None

    @classmethod
    def from_data(cls, data: GitHubFileBlobData) -> "GitHubFileBlob":
        return cls(
            path=PurePosixPath(data["path"]),
            content=data["content"],
            size=data["size"],
            commit_sha=None if data.get("commit_sha") is None else data["commit_sha"],
        )


@dataclass(frozen=True)
class GitHubRepositorySnapshot:
    ref: str
    commit_sha: str
    files: dict[PurePosixPath, GitHubFileBlob]

    @classmethod
    def from_data(
        cls, data: GitHubRepositorySnapshotData
    ) -> "GitHubRepositorySnapshot":
        files = {
            PurePosixPath(path): GitHubFileBlob.from_data(blob)
            for path, blob in data["files"].items()
        }
        return cls(
            ref=data["ref"],
            commit_sha=data["commit_sha"],
            files=files,
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


def discover_github_skills(
    fetcher: bridge.ClientConfigSource,
    github_url: str,
    *,
    origin: SkillOrigin | None = None,
) -> list[Skill]:
    return bridge.discover_github_skills(
        github_url,
        origin=origin,
        **bridge.client_config_kwargs(fetcher),
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


def parse_github_skill_url(github_url: str) -> GitHubSkillLocation:
    return GitHubSkillLocation.from_data(bridge.parse_github_skill_url(github_url))


def github_versions_match(installed: Skill, available: Skill) -> bool:
    return bool(bridge.github_versions_match(installed, available))


def repository_versions_match(installed: Skill, available: Skill) -> bool:
    return bool(bridge.repository_versions_match(installed, available))
