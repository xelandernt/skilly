from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Protocol

from . import _bridge as bridge
from ._core import Skill
from .constants import DEFAULT_SKILLS_PATH

if TYPE_CHECKING:
    from ._core import (
        GitHubContentItemData,
        GitHubFileBlobData,
        GitHubRepositorySnapshotData,
        GitHubSkillLocationData,
    )
    from .filesystem import FileSystem


class GitHubSkillFetcher(Protocol):
    @property
    def base_url(self) -> str | None: ...

    @property
    def api_key(self) -> str | None: ...

    @property
    def github_token(self) -> str | None: ...

    @property
    def proxy(self) -> str | None: ...


class SkillsMpInstallableSkill(Protocol):
    @property
    def id(self) -> str: ...

    @property
    def githubUrl(self) -> str: ...


@dataclass(frozen=True)
class SkillOrigin:
    source: str | None = None
    package_name: str | None = None
    package_version: str | None = None
    github_url: str | None = None
    github_commit_sha: str | None = None
    skillsmp_id: str | None = None


@dataclass(frozen=True)
class SkillResource:
    relative_path: PurePosixPath
    kind: str
    content: str = ""


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
            owner=str(data["owner"]),
            repo=str(data["repo"]),
            ref=None if data.get("ref") is None else str(data["ref"]),
            path=PurePosixPath(str(data.get("path", "."))),
            url=str(data["url"]),
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
            type=str(data["type"]),
            name=str(data["name"]),
            path=PurePosixPath(str(data["path"])),
            commit_sha=None
            if data.get("commit_sha") is None
            else str(data["commit_sha"]),
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
            path=PurePosixPath(str(data["path"])),
            content=str(data["content"]),
            size=int(str(data["size"])),
            commit_sha=None
            if data.get("commit_sha") is None
            else str(data["commit_sha"]),
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
            ref=str(data["ref"]),
            commit_sha=str(data["commit_sha"]),
            files=files,
        )


def discover_installed_skills(
    directory: Path = DEFAULT_SKILLS_PATH,
    *,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return bridge.discover_installed_skills(directory, file_system=file_system)


def discover_venv_skills(
    path: Path = Path(".venv"),
    *,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return bridge.discover_venv_skills(path, file_system=file_system)


def discover_github_skills(
    fetcher: GitHubSkillFetcher,
    github_url: str,
    *,
    origin: SkillOrigin | None = None,
) -> list[Skill]:
    return bridge.discover_github_skills(
        github_url,
        origin=origin,
        **bridge.client_config_kwargs(fetcher),
    )


def parse_github_skill_url(github_url: str) -> GitHubSkillLocation:
    return GitHubSkillLocation.from_data(bridge.parse_github_skill_url(github_url))


def github_versions_match(installed: Skill, available: Skill) -> bool:
    return bool(bridge.github_versions_match(installed, available))
