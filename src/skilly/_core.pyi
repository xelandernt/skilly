from os import PathLike
from pathlib import Path, PurePosixPath
from typing import Literal, Protocol, TypeAlias, TypedDict

ResourceKind: TypeAlias = Literal["script", "reference", "asset", "other"]
StrPath: TypeAlias = str | PathLike[str]

class SkillResourceLike(Protocol):
    @property
    def relative_path(self) -> PurePosixPath: ...
    @property
    def kind(self) -> ResourceKind: ...
    @property
    def content(self) -> str: ...

class GitHubSkillFetcherLike(Protocol):
    base_url: str | None
    api_key: str | None
    github_token: str | None
    proxy: str | None

class SkillsMpInstallableSkillLike(Protocol):
    id: str
    githubUrl: str

class GitHubSkillLocationData(TypedDict):
    owner: str
    repo: str
    ref: str | None
    path: str
    url: str

class GitHubContentItemData(TypedDict):
    type: str
    name: str
    path: str
    commit_sha: str | None

class GitHubFileBlobData(TypedDict):
    path: str
    content: str
    size: int
    commit_sha: str | None

class GitHubRepositorySnapshotData(TypedDict):
    ref: str
    commit_sha: str
    files: dict[str, GitHubFileBlobData]

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
        github_url: str | None = ...,
        github_commit_sha: str | None = ...,
        skillsmp_id: str | None = ...,
    ) -> None: ...
    @classmethod
    def from_text(
        cls,
        text: str,
        path: StrPath | None = ...,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        github_url: str | None = ...,
        github_commit_sha: str | None = ...,
        skillsmp_id: str | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_file(
        cls,
        path: StrPath,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        github_url: str | None = ...,
        github_commit_sha: str | None = ...,
        skillsmp_id: str | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_dir(
        cls,
        path: StrPath,
        source: str | None = ...,
        package_name: str | None = ...,
        package_version: str | None = ...,
        github_url: str | None = ...,
        github_commit_sha: str | None = ...,
        skillsmp_id: str | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_github(
        cls,
        fetcher: GitHubSkillFetcherLike,
        github_url: str,
        source: str | None = ...,
        skillsmp_id: str | None = ...,
    ) -> Skill: ...
    @classmethod
    def from_skillsmp(
        cls,
        fetcher: GitHubSkillFetcherLike,
        installable_skill: SkillsMpInstallableSkillLike,
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
    def github_url(self) -> str | None: ...
    @property
    def github_commit_sha(self) -> str | None: ...
    @property
    def skillsmp_id(self) -> str | None: ...
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
    def is_skillsmp(self) -> bool: ...
    def is_github(self) -> bool: ...
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
    github_url: str | None = ...,
    github_commit_sha: str | None = ...,
    skillsmp_id: str | None = ...,
) -> Skill: ...
def skill_from_file(
    path: StrPath,
    source: str | None = ...,
    package_name: str | None = ...,
    package_version: str | None = ...,
    github_url: str | None = ...,
    github_commit_sha: str | None = ...,
    skillsmp_id: str | None = ...,
) -> Skill: ...
def skill_from_dir(
    path: StrPath,
    source: str | None = ...,
    package_name: str | None = ...,
    package_version: str | None = ...,
    github_url: str | None = ...,
    github_commit_sha: str | None = ...,
    skillsmp_id: str | None = ...,
) -> Skill: ...
def skill_render(skill: Skill, metadata: dict[str, str] | None = ...) -> str: ...
def skill_install_to(
    skill: Skill,
    directory: StrPath | None = ...,
    skill_name: str | None = ...,
    overwrite: bool = ...,
) -> Skill: ...
def discover_installed_skills(directory: StrPath | None = ...) -> list[Skill]: ...
def discover_venv_skills(path: StrPath | None = ...) -> list[Skill]: ...
def parse_github_skill_url(github_url: str) -> GitHubSkillLocationData: ...
def discover_github_skills(
    github_url: str,
    source: str | None = ...,
    skillsmp_id: str | None = ...,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> list[Skill]: ...
def github_versions_match(installed: Skill, available: Skill) -> bool: ...
def remove_skill(name: str, directory: StrPath | None = ...) -> Skill: ...
def project_requirements(
    pyproject_toml_path: StrPath | None = ...,
    include_dev: bool = ...,
    include_extras: list[str] | None = ...,
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
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> SkillsMpSearchApiResponseData: ...
def skillsmp_ai_search(
    q: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> SkillsMpAiSearchApiResponseData: ...
def skillsmp_fetch_github_directory(
    github_url: str,
    current_path: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> list[GitHubContentItemData]: ...
def skillsmp_fetch_github_file(
    github_url: str,
    path: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> GitHubFileBlobData: ...
def skillsmp_fetch_github_snapshot(
    github_url: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> GitHubRepositorySnapshotData: ...
def skillsmp_resolve_github_ref_and_commit_sha(
    github_url: str,
    base_url: str | None = ...,
    api_key: str | None = ...,
    github_token: str | None = ...,
    proxy: str | None = ...,
) -> tuple[str, str]: ...
def run_cli(args: list[str]) -> int: ...
