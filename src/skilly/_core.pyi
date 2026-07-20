from os import PathLike
from pathlib import Path, PurePosixPath
from typing import Literal, Protocol, TypeAlias, TypedDict

from .filesystem import FileSystem

ResourceKind: TypeAlias = Literal["script", "reference", "asset", "other"]
RepositoryProvider: TypeAlias = Literal[
    "github", "bitbucket-cloud", "bitbucket-data-center"
]
StrPath: TypeAlias = str | PathLike[str]

class SkillResourceLike(Protocol):
    @property
    def relative_path(self) -> PurePosixPath: ...
    @property
    def kind(self) -> ResourceKind: ...
    @property
    def content(self) -> bytes: ...

class RepositoryLocationData(TypedDict):
    provider: RepositoryProvider
    base_url: str
    namespace: str
    repo: str
    ref: str | None
    path: str
    url: str

class Skill:
    def __init__(
        self,
        name: str,
        description: str,
        path: StrPath | None = ...,
        content: str = ...,
        license: str | None = ...,
        compatibility: str | None = ...,
        metadata: dict[str, str] | None = ...,
        allowed_tools: str | None = ...,
        resources: list[SkillResourceLike] | None = ...,
        resource_warnings: list[str] | None = ...,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        package_ecosystem: str | None = ...,
    ) -> None: ...
    @classmethod
    def from_text(
        cls,
        text: str,
        path: StrPath | None = ...,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        package_ecosystem: str | None = ...,
        file_system: FileSystem | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_file(
        cls,
        path: StrPath,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        package_ecosystem: str | None = ...,
        file_system: FileSystem | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_dir(
        cls,
        path: StrPath,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        package_ecosystem: str | None = ...,
        file_system: FileSystem | None = ...,
    ) -> Skill: ...
    @property
    def name(self) -> str: ...
    @property
    def description(self) -> str: ...
    @property
    def path(self) -> Path | None: ...
    @property
    def content(self) -> str: ...
    @property
    def license(self) -> str | None: ...
    @property
    def compatibility(self) -> str | None: ...
    @property
    def metadata(self) -> dict[str, str]: ...
    @property
    def allowed_tools(self) -> str | None: ...
    @property
    def resources(self) -> list[SkillResourceLike]: ...
    @property
    def resource_warnings(self) -> list[str]: ...
    @property
    def source(self) -> str: ...
    @property
    def package_name(self) -> str | None: ...
    @property
    def package_version(self) -> str | None: ...
    @property
    def repository_provider(self) -> RepositoryProvider | None: ...
    @property
    def repository_url(self) -> str | None: ...
    @property
    def repository_commit_sha(self) -> str | None: ...
    @property
    def package_ecosystem(self) -> str | None: ...
    @property
    def skill_markdown_path(self) -> Path | None: ...
    @property
    def directory(self) -> Path | None: ...
    @property
    def directory_name(self) -> str: ...
    @property
    def scripts(self) -> list[SkillResourceLike]: ...
    @property
    def references(self) -> list[SkillResourceLike]: ...
    @property
    def assets(self) -> list[SkillResourceLike]: ...
    def get_resource(self, relative_path: StrPath) -> SkillResourceLike | None: ...
    def is_installed(self) -> bool: ...
    def is_dependency(self) -> bool: ...
    def can_update(self) -> bool: ...
    def matches(self, other: Skill) -> bool: ...
    def package_reference(self) -> str | None: ...
    def managed_metadata(self) -> dict[str, str]: ...
    def render(self, metadata: dict[str, str] | None = ...) -> str: ...
    def install_to(
        self,
        directory: StrPath | None = ...,
        skill_name: str | None = ...,
        overwrite: bool = ...,
        file_system: FileSystem | None = ...,
    ) -> Skill: ...
    def replace_to(
        self,
        directory: StrPath | None = ...,
        skill_name: str | None = ...,
        file_system: FileSystem | None = ...,
    ) -> Skill: ...

class SkillsMpSkillData(TypedDict):
    id: str
    name: str
    author: str
    description: str
    githubUrl: str
    skillUrl: str
    stars: int | None
    updatedAt: str | int | None

class _SkillsMpPaginationOptional(TypedDict, total=False):
    totalIsExact: bool | None

class SkillsMpPaginationData(_SkillsMpPaginationOptional):
    page: int
    limit: int
    total: int
    totalPages: int
    hasNext: bool
    hasPrev: bool

class SkillsMpFiltersData(TypedDict, total=False):
    search: str | None
    sortBy: str | None
    category: str | None
    occupation: str | None

class SkillsMpSearchData(TypedDict):
    skills: list[SkillsMpSkillData]
    pagination: SkillsMpPaginationData
    filters: SkillsMpFiltersData

class SkillsMpAiSearchData(TypedDict):
    skills: list[SkillsMpSkillData]
    results: list[SkillsMpSkillData]

class SkillsMpMetaData(TypedDict, total=False):
    requestId: str | None
    responseTimeMs: int | None

class SkillsMpErrorData(TypedDict):
    code: str
    message: str

class _SkillsMpSearchApiResponseOptional(TypedDict, total=False):
    meta: SkillsMpMetaData | None

class SkillsMpSearchApiResponseData(_SkillsMpSearchApiResponseOptional):
    success: bool
    data: SkillsMpSearchData

class _SkillsMpAiSearchApiResponseOptional(TypedDict, total=False):
    meta: SkillsMpMetaData | None

class SkillsMpAiSearchApiResponseData(_SkillsMpAiSearchApiResponseOptional):
    success: bool
    data: SkillsMpAiSearchData

class _SkillsMpErrorApiResponseOptional(TypedDict, total=False):
    meta: SkillsMpMetaData | None

class SkillsMpErrorApiResponseData(_SkillsMpErrorApiResponseOptional):
    success: bool
    error: SkillsMpErrorData

def skill_from_text(
    text: str,
    path: StrPath | None = ...,
    source: str | None = ...,
    package_name: str | None = ...,
    package_version: str | None = ...,
    package_ecosystem: str | None = ...,
    file_system: FileSystem | None = ...,
) -> Skill: ...
def skill_from_file(
    path: StrPath,
    source: str | None = ...,
    package_name: str | None = ...,
    package_version: str | None = ...,
    package_ecosystem: str | None = ...,
    file_system: FileSystem | None = ...,
) -> Skill: ...
def skill_from_dir(
    path: StrPath,
    source: str | None = ...,
    package_name: str | None = ...,
    package_version: str | None = ...,
    package_ecosystem: str | None = ...,
    file_system: FileSystem | None = ...,
) -> Skill: ...
def skill_render(skill: Skill, metadata: dict[str, str] | None = ...) -> str: ...
def skill_install_to(
    skill: Skill,
    directory: StrPath | None = ...,
    skill_name: str | None = ...,
    overwrite: bool = ...,
    file_system: FileSystem | None = ...,
) -> Skill: ...
def resolve_skills_directory(
    agent: Literal["agents", "claude", "codex", "copilot"] = ...,
    global_: bool = ...,
) -> Path: ...
def discover_installed_skills(
    directory: StrPath | None = ..., file_system: FileSystem | None = ...
) -> list[Skill]: ...
def discover_package_source_skills(
    source: dict[str, object], file_system: FileSystem | None = ...
) -> list[Skill]: ...
def scan_project(
    directory: StrPath | None = ...,
    sources: list[dict[str, object]] | None = ...,
    file_system: FileSystem | None = ...,
) -> list[tuple[Skill, Skill | None]]: ...
def available_dependency_skill(
    installed: Skill,
    directory: StrPath | None = ...,
    sources: list[dict[str, object]] | None = ...,
    file_system: FileSystem | None = ...,
) -> Skill | None: ...
def parse_repository_location(
    repository_url: str,
    provider: RepositoryProvider | None = ...,
) -> RepositoryLocationData: ...
def discover_repository_skills(
    repository_url: str,
    provider: RepositoryProvider | None = ...,
    token: str | None = ...,
) -> list[Skill]: ...
def repository_versions_match(installed: Skill, available: Skill) -> bool: ...
def remove_skill(
    name: str,
    directory: StrPath | None = ...,
    file_system: FileSystem | None = ...,
) -> Skill: ...
def project_requirements(
    pyproject_toml_path: StrPath | None = ...,
    include_dev: bool = ...,
    include_extras: list[str] | None = ...,
    file_system: FileSystem | None = ...,
) -> list[str]: ...
def skillsmp_search(
    q: str,
    page: int | None = ...,
    limit: int | None = ...,
    sort_by: str | None = ...,
    category: str | None = ...,
    occupation: str | None = ...,
    base_url: str | None = ...,
    api_key: str | None = ...,
    proxy: str | None = ...,
) -> SkillsMpSearchApiResponseData: ...
def skillsmp_ai_search(
    q: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    proxy: str | None = ...,
) -> SkillsMpAiSearchApiResponseData: ...
def run_cli(args: list[str]) -> int: ...
