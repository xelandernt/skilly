from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import TYPE_CHECKING, Literal, Protocol, TypeAlias, TypedDict

from . import _core
from .filesystem import FileSystem, StrPath

if TYPE_CHECKING:
    from ._core import (
        RepositoryLocationData,
        Skill,
        SkillsMpAiSearchApiResponseData,
        SkillsMpSearchApiResponseData,
    )

BridgeScalar: TypeAlias = str | int | float | bool | None
BridgeObject: TypeAlias = Mapping[str, "BridgeValue"]
BridgeArray: TypeAlias = Sequence["BridgeValue"]
BridgeValue: TypeAlias = BridgeScalar | BridgeObject | BridgeArray
AgentName: TypeAlias = Literal["agents", "claude", "codex", "copilot"]
RepositoryProvider: TypeAlias = Literal[
    "github", "bitbucket-cloud", "bitbucket-data-center"
]


class ClientConfigSource(Protocol):
    @property
    def base_url(self) -> str | None: ...

    @property
    def api_key(self) -> str | None: ...

    @property
    def proxy(self) -> str | None: ...


class ClientConfigKwargs(TypedDict):
    base_url: str | None
    api_key: str | None
    proxy: str | None


class SkillMetadataKwargs(TypedDict):
    source: str | None
    package_name: str | None
    package_version: str | None
    package_ecosystem: str | None


def client_config_kwargs(fetcher: ClientConfigSource | None) -> ClientConfigKwargs:
    if fetcher is None:
        return {
            "base_url": None,
            "api_key": None,
            "proxy": None,
        }
    return {
        "base_url": fetcher.base_url,
        "api_key": fetcher.api_key,
        "proxy": fetcher.proxy,
    }


def skill_metadata_kwargs(
    *,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    package_ecosystem: str | None = None,
) -> SkillMetadataKwargs:
    return {
        "source": source,
        "package_name": package_name,
        "package_version": package_version,
        "package_ecosystem": package_ecosystem,
    }


def skill_from_text(
    text: str,
    path: StrPath | None = None,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    package_ecosystem: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    return _core.skill_from_text(
        text,
        path=path,
        **skill_metadata_kwargs(
            source=source,
            package_name=package_name,
            package_version=package_version,
            package_ecosystem=package_ecosystem,
        ),
        file_system=file_system,
    )


def skill_from_file(
    path: StrPath,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    package_ecosystem: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    return _core.skill_from_file(
        path,
        **skill_metadata_kwargs(
            source=source,
            package_name=package_name,
            package_version=package_version,
            package_ecosystem=package_ecosystem,
        ),
        file_system=file_system,
    )


def skill_from_dir(
    path: StrPath,
    source: str | None = None,
    package_name: str | None = None,
    package_version: str | None = None,
    package_ecosystem: str | None = None,
    file_system: FileSystem | None = None,
) -> Skill:
    return _core.skill_from_dir(
        path,
        **skill_metadata_kwargs(
            source=source,
            package_name=package_name,
            package_version=package_version,
            package_ecosystem=package_ecosystem,
        ),
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


def resolve_skills_directory(
    agent: AgentName = "agents",
    *,
    global_: bool = False,
) -> StrPath:
    return _core.resolve_skills_directory(agent, global_=global_)


def discover_installed_skills(
    directory: StrPath | None = None,
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return _core.discover_installed_skills(directory, file_system=file_system)


def discover_package_source_skills(
    source: dict[str, object],
    file_system: FileSystem | None = None,
) -> list[Skill]:
    return _core.discover_package_source_skills(source, file_system=file_system)


def scan_project(
    directory: StrPath | None = None,
    sources: list[dict[str, object]] | None = None,
    file_system: FileSystem | None = None,
) -> list[tuple[Skill, Skill | None]]:
    return _core.scan_project(
        directory=directory,
        sources=sources,
        file_system=file_system,
    )


def available_dependency_skill(
    installed: Skill,
    directory: StrPath | None = None,
    sources: list[dict[str, object]] | None = None,
    file_system: FileSystem | None = None,
) -> Skill | None:
    return _core.available_dependency_skill(
        installed,
        directory=directory,
        sources=sources,
        file_system=file_system,
    )


def parse_repository_location(
    repository_url: str,
    provider: RepositoryProvider | None = None,
) -> RepositoryLocationData:
    return _core.parse_repository_location(repository_url, provider=provider)


def discover_repository_skills(
    repository_url: str,
    provider: RepositoryProvider | None = None,
    token: str | None = None,
) -> list[Skill]:
    return _core.discover_repository_skills(
        repository_url,
        provider=provider,
        token=token,
    )


def repository_versions_match(installed: Skill, available: Skill) -> bool:
    return _core.repository_versions_match(installed, available)


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
        proxy=proxy,
    )


def skillsmp_ai_search(
    q: str,
    base_url: str | None = None,
    api_key: str | None = None,
    proxy: str | None = None,
) -> SkillsMpAiSearchApiResponseData:
    return _core.skillsmp_ai_search(
        q,
        base_url=base_url,
        api_key=api_key,
        proxy=proxy,
    )


def run_cli(args: list[str]) -> int:
    return _core.run_cli(args)
