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


class GitHubSkillFetcher(Protocol):
    base_url: str | None
    api_key: str | None
    github_token: str | None
    proxy: str | None


class SkillsMpInstallableSkill(Protocol):
    id: str
    githubUrl: str


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


def discover_installed_skills(directory: Path = DEFAULT_SKILLS_PATH) -> list[Skill]:
    return bridge.discover_installed_skills(directory)


def discover_venv_skills(path: Path = Path(".venv")) -> list[Skill]:
    return bridge.discover_venv_skills(path)


def discover_github_skills(
    fetcher: GitHubSkillFetcher,
    github_url: str,
    *,
    source: str | None = None,
    skillsmp_id: str | None = None,
) -> list[Skill]:
    return bridge.discover_github_skills(
        github_url,
        source=source,
        skillsmp_id=skillsmp_id,
        **bridge.client_config_kwargs(fetcher),
    )


def parse_github_skill_url(github_url: str) -> GitHubSkillLocation:
    return GitHubSkillLocation.from_data(bridge.parse_github_skill_url(github_url))


def github_versions_match(installed: Skill, available: Skill) -> bool:
    return bool(bridge.github_versions_match(installed, available))
