from __future__ import annotations

from collections.abc import Mapping, Sequence
from os import PathLike
from typing import TYPE_CHECKING, Protocol, TypeAlias, TypedDict

from . import _core

if TYPE_CHECKING:
    from ._core import (
        GitHubContentItemData,
        GitHubFileBlobData,
        GitHubRepositorySnapshotData,
        GitHubSkillLocationData,
        Skill,
        SkillsMpAiSearchApiResponseData,
        SkillsMpSearchApiResponseData,
    )
    from .filesystem import FileSystem

BridgeScalar: TypeAlias = str | int | float | bool | None
BridgeObject: TypeAlias = Mapping[str, "BridgeValue"]
BridgeArray: TypeAlias = Sequence["BridgeValue"]
BridgeValue: TypeAlias = BridgeScalar | BridgeObject | BridgeArray
StrPath: TypeAlias = str | PathLike[str]


class ClientConfigSource(Protocol):
    @property
    def base_url(self) -> str | None: ...

    @property
    def api_key(self) -> str | None: ...

    @property
    def github_token(self) -> str | None: ...

    @property
    def proxy(self) -> str | None: ...


class SkillOriginSource(Protocol):
    @property
    def source(self) -> str | None: ...

    @property
    def package_name(self) -> str | None: ...

    @property
    def package_version(self) -> str | None: ...

    @property
    def github_url(self) -> str | None: ...

    @property
    def github_commit_sha(self) -> str | None: ...

    @property
    def skillsmp_id(self) -> str | None: ...


class ClientConfigKwargs(TypedDict):
    base_url: str | None
    api_key: str | None
    github_token: str | None
    proxy: str | None


class SkillOriginKwargs(TypedDict):
    source: str | None
    package_name: str | None
    package_version: str | None
    github_url: str | None
    github_commit_sha: str | None
    skillsmp_id: str | None


def client_config_kwargs(fetcher: ClientConfigSource | None) -> ClientConfigKwargs:
    if fetcher is None:
        return {
            "base_url": None,
            "api_key": None,
            "github_token": None,
            "proxy": None,
        }
    return {
        "base_url": fetcher.base_url,
        "api_key": fetcher.api_key,
        "github_token": fetcher.github_token,
        "proxy": fetcher.proxy,
    }


def skill_origin_kwargs(origin: SkillOriginSource | None) -> SkillOriginKwargs:
    if origin is None:
        return {
            "source": None,
            "package_name": None,
            "package_version": None,
            "github_url": None,
            "github_commit_sha": None,
            "skillsmp_id": None,
        }
    return {
        "source": origin.source,
        "package_name": origin.package_name,
        "package_version": origin.package_version,
        "github_url": origin.github_url,
        "github_commit_sha": origin.github_commit_sha,
        "skillsmp_id": origin.skillsmp_id,
    }


def skill_from_text(
    text: str,
    path: StrPath | None = None,
    origin: SkillOriginSource | None = None,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    github_url: str | None = None,
    github_commit_sha: str | None = None,
    skillsmp_id: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    origin_kwargs = skill_origin_kwargs(origin)
    return _core.skill_from_text(
        text,
        path=path,
        source=source if source is not None else origin_kwargs["source"],
        package_name=package_name
        if package_name is not None
        else origin_kwargs["package_name"],
        package_version=package_version
        if package_version is not None
        else origin_kwargs["package_version"],
        github_url=github_url
        if github_url is not None
        else origin_kwargs["github_url"],
        github_commit_sha=github_commit_sha
        if github_commit_sha is not None
        else origin_kwargs["github_commit_sha"],
        skillsmp_id=skillsmp_id
        if skillsmp_id is not None
        else origin_kwargs["skillsmp_id"],
        file_system=file_system,
    )


def skill_from_file(
    path: StrPath,
    origin: SkillOriginSource | None = None,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    github_url: str | None = None,
    github_commit_sha: str | None = None,
    skillsmp_id: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    origin_kwargs = skill_origin_kwargs(origin)
    return _core.skill_from_file(
        path,
        source=source if source is not None else origin_kwargs["source"],
        package_name=package_name
        if package_name is not None
        else origin_kwargs["package_name"],
        package_version=package_version
        if package_version is not None
        else origin_kwargs["package_version"],
        github_url=github_url
        if github_url is not None
        else origin_kwargs["github_url"],
        github_commit_sha=github_commit_sha
        if github_commit_sha is not None
        else origin_kwargs["github_commit_sha"],
        skillsmp_id=skillsmp_id
        if skillsmp_id is not None
        else origin_kwargs["skillsmp_id"],
        file_system=file_system,
    )


def skill_from_dir(
    path: StrPath,
    origin: SkillOriginSource | None = None,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    github_url: str | None = None,
    github_commit_sha: str | None = None,
    skillsmp_id: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    origin_kwargs = skill_origin_kwargs(origin)
    return _core.skill_from_dir(
        path,
        source=source if source is not None else origin_kwargs["source"],
        package_name=package_name
        if package_name is not None
        else origin_kwargs["package_name"],
        package_version=package_version
        if package_version is not None
        else origin_kwargs["package_version"],
        github_url=github_url
        if github_url is not None
        else origin_kwargs["github_url"],
        github_commit_sha=github_commit_sha
        if github_commit_sha is not None
        else origin_kwargs["github_commit_sha"],
        skillsmp_id=skillsmp_id
        if skillsmp_id is not None
        else origin_kwargs["skillsmp_id"],
        file_system=file_system,
    )


def skill_render(skill: Skill, metadata: dict[str, str] | None = None) -> str:
    return _core.skill_render(skill, metadata)


def skill_install_to(
    skill: Skill,
    directory: StrPath | None = None,
    skill_name: str | None = None,
    overwrite: bool = False,
    file_system: FileSystem | None = None,
) -> Skill:
    return _core.skill_install_to(
        skill,
        directory=directory,
        skill_name=skill_name,
        overwrite=overwrite,
        file_system=file_system,
    )


def discover_installed_skills(
    directory: StrPath | None = None,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return _core.discover_installed_skills(directory, file_system=file_system)


def discover_venv_skills(
    path: StrPath | None = None,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return _core.discover_venv_skills(path, file_system=file_system)


def parse_github_skill_url(github_url: str) -> GitHubSkillLocationData:
    return _core.parse_github_skill_url(github_url)


def discover_github_skills(
    github_url: str,
    origin: SkillOriginSource | None = None,
    source: str | None = None,
    skillsmp_id: str | None = None,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> list[Skill]:
    origin_kwargs = skill_origin_kwargs(origin)
    return _core.discover_github_skills(
        github_url,
        source=source if source is not None else origin_kwargs["source"],
        skillsmp_id=skillsmp_id
        if skillsmp_id is not None
        else origin_kwargs["skillsmp_id"],
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def github_versions_match(installed: Skill, available: Skill) -> bool:
    return _core.github_versions_match(installed, available)


def remove_skill(
    name: str,
    directory: StrPath | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    return _core.remove_skill(name, directory, file_system=file_system)


def project_requirements(
    pyproject_toml_path: StrPath | None = None,
    include_dev: bool = False,
    include_extras: list[str] | None = None,
    file_system: FileSystem | None = None,
) -> list[str]:
    return _core.project_requirements(
        pyproject_toml_path=pyproject_toml_path,
        include_dev=include_dev,
        include_extras=include_extras,
        file_system=file_system,
    )


def skillsmp_search(
    q: str,
    page: int | None = None,
    limit: int | None = None,
    sort_by: str | None = None,
    category: str | None = None,
    occupation: str | None = None,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> SkillsMpSearchApiResponseData:
    return _core.skillsmp_search(
        q,
        page=page,
        limit=limit,
        sort_by=sort_by,
        category=category,
        occupation=occupation,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def skillsmp_ai_search(
    q: str,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> SkillsMpAiSearchApiResponseData:
    return _core.skillsmp_ai_search(
        q,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def skillsmp_fetch_github_directory(
    github_url: str,
    current_path: str,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> list[GitHubContentItemData]:
    return _core.skillsmp_fetch_github_directory(
        github_url,
        current_path,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def skillsmp_fetch_github_file(
    github_url: str,
    path: str,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> GitHubFileBlobData:
    return _core.skillsmp_fetch_github_file(
        github_url,
        path,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def skillsmp_fetch_github_snapshot(
    github_url: str,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> GitHubRepositorySnapshotData:
    return _core.skillsmp_fetch_github_snapshot(
        github_url,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def skillsmp_resolve_github_ref_and_commit_sha(
    github_url: str,
    base_url: str | None = None,
    api_key: str | None = None,
    github_token: str | None = None,
    proxy: str | None = None,
) -> tuple[str, str]:
    return _core.skillsmp_resolve_github_ref_and_commit_sha(
        github_url,
        base_url=base_url,
        api_key=api_key,
        github_token=github_token,
        proxy=proxy,
    )


def run_cli(args: list[str]) -> int:
    return _core.run_cli(args)
