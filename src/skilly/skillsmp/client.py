from __future__ import annotations

import abc
import base64
import os
from pathlib import PurePosixPath
from typing import TypeVar

import niquests
from pydantic import BaseModel, ConfigDict, RootModel

from ..skills import GitHubContentItem, GitHubFileBlob, GitHubSkillLocation
from .response import AsyncResponse, Response

SKILLSMP_API_KEY_ENV_VAR = "SKILLSMP_API_KEY"
GITHUB_API_BASE_URL = "https://api.github.com"

ResponseModelT = TypeVar("ResponseModelT", bound=BaseModel)
HeadersMap = dict[str, str]
ProxyConfig = dict[str, str]
QueryParamValue = str | list[str] | None
QueryParams = dict[str, QueryParamValue]


class SkillsMpSkill(BaseModel):
    model_config = ConfigDict(extra="allow")

    id: str
    name: str
    author: str
    description: str
    githubUrl: str
    skillUrl: str
    stars: int | None = None
    updatedAt: str | int | None = None


class SkillsMpPagination(BaseModel):
    model_config = ConfigDict(extra="allow")

    page: int
    limit: int
    total: int
    totalPages: int
    hasNext: bool
    hasPrev: bool
    totalIsExact: bool | None = None


class SkillsMpFilters(BaseModel):
    model_config = ConfigDict(extra="allow")

    search: str | None = None
    sortBy: str | None = None
    category: str | None = None
    occupation: str | None = None


class SkillsMpSearchData(BaseModel):
    model_config = ConfigDict(extra="allow")

    skills: list[SkillsMpSkill]
    pagination: SkillsMpPagination
    filters: SkillsMpFilters


class SkillsMpAiSearchData(BaseModel):
    model_config = ConfigDict(extra="allow")

    skills: list[SkillsMpSkill] = []
    results: list[SkillsMpSkill] = []


class SkillsMpMeta(BaseModel):
    model_config = ConfigDict(extra="allow")

    requestId: str | None = None
    responseTimeMs: int | None = None


class SkillsMpError(BaseModel):
    model_config = ConfigDict(extra="allow")

    code: str
    message: str


class SkillsMpSearchApiResponse(BaseModel):
    model_config = ConfigDict(extra="allow")

    success: bool
    data: SkillsMpSearchData
    meta: SkillsMpMeta | None = None


class SkillsMpAiSearchApiResponse(BaseModel):
    model_config = ConfigDict(extra="allow")

    success: bool
    data: SkillsMpAiSearchData
    meta: SkillsMpMeta | None = None


class SkillsMpErrorApiResponse(BaseModel):
    model_config = ConfigDict(extra="allow")

    success: bool
    error: SkillsMpError
    meta: SkillsMpMeta | None = None


class GitHubContentEntry(BaseModel):
    model_config = ConfigDict(extra="allow")

    type: str
    name: str
    path: str
    url: str
    html_url: str | None = None
    size: int | None = None
    download_url: str | None = None


class GitHubContentEntries(RootModel[list[GitHubContentEntry]]):
    pass


class GitHubFileContent(BaseModel):
    model_config = ConfigDict(extra="allow")

    type: str
    name: str
    path: str
    html_url: str | None = None
    size: int | None = None
    encoding: str | None = None
    content: str | None = None


def _get_api_key_from_env() -> str | None:
    return os.getenv(SKILLSMP_API_KEY_ENV_VAR)


def _normalize_query_item(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (str, int, float)):
        return str(value)
    raise TypeError(f"Unsupported query list item type: {type(value)!r}")


def _normalize_query_value(value: object) -> QueryParamValue:
    if value is None:
        return None
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (str, int, float)):
        return str(value)
    if isinstance(value, list):
        return [_normalize_query_item(item) for item in value]
    raise TypeError(f"Unsupported query parameter type: {type(value)!r}")


def _decode_github_file_content(file_content: GitHubFileContent) -> str:
    if file_content.content is None:
        raise ValueError(
            f"GitHub file response for {file_content.path} is missing content"
        )
    if file_content.encoding not in (None, "base64"):
        raise ValueError(
            f"Unsupported GitHub file encoding for {file_content.path}: {file_content.encoding}"
        )
    normalized_content = file_content.content.replace("\n", "")
    return base64.b64decode(normalized_content).decode("utf-8")


def _extract_commit_sha_from_html_url(html_url: str | None) -> str | None:
    if html_url is None:
        return None
    parts = [part for part in html_url.split("/") if part]
    if len(parts) < 6 or parts[0] not in {"http:", "https:"}:
        return None
    if parts[1] != "github.com" or parts[4] not in {"blob", "tree"}:
        return None
    return parts[5]


class _SkillsMpBase(abc.ABC):
    def __init__(
        self,
        *,
        base_url: str | None = None,
        api_key: str | None = None,
        proxy: str | None = None,
    ) -> None:
        self.base_url = base_url or "https://skillsmp.com/api/v1"
        self._provided_api_key = api_key
        self._proxy = proxy

    def _build_url(self, path: str) -> str:
        return f"{self.base_url}{path}"

    def _get_api_key(self) -> str:
        api_key = self._provided_api_key or _get_api_key_from_env()
        if not api_key:
            raise ValueError(
                "API key is required. Set it via environment variable "
                f"{SKILLSMP_API_KEY_ENV_VAR} or pass it to the client."
            )
        return api_key

    def _build_headers(self, *, require_api_key: bool = False) -> HeadersMap:
        headers: HeadersMap = {
            "Accept": "application/json",
        }
        api_key = self._provided_api_key or _get_api_key_from_env()
        if api_key is not None:
            headers["Authorization"] = f"Bearer {api_key}"
        elif require_api_key:
            self._get_api_key()
        return headers

    def _build_request(
        self,
        path: str,
        query_params: dict[str, object],
        *,
        require_api_key: bool = False,
    ) -> tuple[str, HeadersMap, QueryParams]:
        headers = self._build_headers(require_api_key=require_api_key)
        params: QueryParams = {}
        for key, value in query_params.items():
            normalized_value = _normalize_query_value(value)
            if normalized_value is not None:
                params[key] = normalized_value
        return self._build_url(path), headers, params

    def _build_proxies(self) -> ProxyConfig | None:
        if self._proxy is None:
            return None
        return {
            "http": self._proxy,
            "https": self._proxy,
        }

    def _build_github_api_url(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> str:
        base_url = (
            f"{GITHUB_API_BASE_URL}/repos/{location.owner}/{location.repo}/contents"
        )
        if str(path) in {"", "."}:
            return base_url
        return f"{base_url}/{path.as_posix()}"


class AsyncSkillsMp(_SkillsMpBase):
    def __init__(
        self,
        client: niquests.AsyncSession | None = None,
        *,
        base_url: str | None = None,
        api_key: str | None = None,
        proxy: str | None = None,
    ) -> None:
        super().__init__(base_url=base_url, api_key=api_key, proxy=proxy)
        self._client = client if client is not None else niquests.AsyncSession()

    async def _request(
        self,
        path: str,
        query_params: dict[str, object],
        response_model: type[ResponseModelT],
        *,
        require_api_key: bool = False,
    ) -> AsyncResponse[ResponseModelT]:
        url, headers, params = self._build_request(
            path,
            query_params,
            require_api_key=require_api_key,
        )
        response = await self._client.get(
            url,
            headers=headers,
            params=params,
            proxies=self._build_proxies(),
            stream=False,
        )
        response.raise_for_status()
        return AsyncResponse(response, response_model)

    async def _github_request(
        self,
        url: str,
        response_model: type[ResponseModelT],
        *,
        params: dict[str, object] | None = None,
    ) -> AsyncResponse[ResponseModelT]:
        response = await self._client.get(
            url,
            headers={"Accept": "application/json"},
            params=params,
            proxies=self._build_proxies(),
            stream=False,
        )
        response.raise_for_status()
        return AsyncResponse(response, response_model)

    async def search(
        self,
        q: str,
        *,
        page: int | None = None,
        limit: int | None = None,
        sort_by: str | None = None,
        category: str | None = None,
        occupation: str | None = None,
    ) -> AsyncResponse[SkillsMpSearchApiResponse]:
        return await self._request(
            "/skills/search",
            {
                "q": q,
                "page": page,
                "limit": limit,
                "sortBy": sort_by,
                "category": category,
                "occupation": occupation,
            },
            SkillsMpSearchApiResponse,
        )

    async def ai_search(self, q: str) -> AsyncResponse[SkillsMpAiSearchApiResponse]:
        return await self._request(
            "/skills/ai-search",
            {"q": q},
            SkillsMpAiSearchApiResponse,
            require_api_key=True,
        )

    async def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        response = await self._github_request(
            self._build_github_api_url(location, current_path),
            GitHubContentEntries,
            params={"ref": location.ref} if location.ref is not None else {},
        )
        entries = (await response.parsed_data).root
        return [
            GitHubContentItem(
                type=entry.type,
                name=entry.name,
                path=PurePosixPath(entry.path),
                commit_sha=_extract_commit_sha_from_html_url(entry.html_url),
            )
            for entry in entries
        ]

    async def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        response = await self._github_request(
            self._build_github_api_url(location, path),
            GitHubFileContent,
            params={"ref": location.ref} if location.ref is not None else {},
        )
        file_content = await response.parsed_data
        return GitHubFileBlob(
            path=PurePosixPath(file_content.path),
            content=_decode_github_file_content(file_content),
            size=file_content.size or 0,
            commit_sha=_extract_commit_sha_from_html_url(file_content.html_url),
        )


class SkillsMp(_SkillsMpBase):
    def __init__(
        self,
        client: niquests.Session | None = None,
        *,
        base_url: str | None = None,
        api_key: str | None = None,
        proxy: str | None = None,
    ) -> None:
        super().__init__(base_url=base_url, api_key=api_key, proxy=proxy)
        self._client = client if client is not None else niquests.Session()

    def _request(
        self,
        path: str,
        query_params: dict[str, object],
        response_model: type[ResponseModelT],
        *,
        require_api_key: bool = False,
    ) -> Response[ResponseModelT]:
        url, headers, params = self._build_request(
            path,
            query_params,
            require_api_key=require_api_key,
        )
        response = self._client.get(
            url,
            headers=headers,
            params=params,
            proxies=self._build_proxies(),
            stream=False,
        )
        response.raise_for_status()
        return Response(response, response_model)

    def _github_request(
        self,
        url: str,
        response_model: type[ResponseModelT],
        *,
        params: dict[str, object] | None = None,
    ) -> Response[ResponseModelT]:
        response = self._client.get(
            url,
            headers={"Accept": "application/json"},
            params=params,
            proxies=self._build_proxies(),
            stream=False,
        )
        response.raise_for_status()
        return Response(response, response_model)

    def search(
        self,
        q: str,
        *,
        page: int | None = None,
        limit: int | None = None,
        sort_by: str | None = None,
        category: str | None = None,
        occupation: str | None = None,
    ) -> Response[SkillsMpSearchApiResponse]:
        return self._request(
            "/skills/search",
            {
                "q": q,
                "page": page,
                "limit": limit,
                "sortBy": sort_by,
                "category": category,
                "occupation": occupation,
            },
            SkillsMpSearchApiResponse,
        )

    def ai_search(self, q: str) -> Response[SkillsMpAiSearchApiResponse]:
        return self._request(
            "/skills/ai-search",
            {"q": q},
            SkillsMpAiSearchApiResponse,
            require_api_key=True,
        )

    def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        response = self._github_request(
            self._build_github_api_url(location, current_path),
            GitHubContentEntries,
            params={"ref": location.ref} if location.ref is not None else {},
        )
        return [
            GitHubContentItem(
                type=entry.type,
                name=entry.name,
                path=PurePosixPath(entry.path),
                commit_sha=_extract_commit_sha_from_html_url(entry.html_url),
            )
            for entry in response.parsed_data.root
        ]

    def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        response = self._github_request(
            self._build_github_api_url(location, path),
            GitHubFileContent,
            params={"ref": location.ref} if location.ref is not None else {},
        )
        file_content = response.parsed_data
        return GitHubFileBlob(
            path=PurePosixPath(file_content.path),
            content=_decode_github_file_content(file_content),
            size=file_content.size or 0,
            commit_sha=_extract_commit_sha_from_html_url(file_content.html_url),
        )
