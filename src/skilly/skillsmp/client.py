from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from .. import _bridge as bridge
from ..skills import (
    GitHubContentItem,
    GitHubFileBlob,
    GitHubRepositorySnapshot,
    GitHubSkillLocation,
)
from .response import AsyncResponse, Response

if TYPE_CHECKING:
    from .._core import (
        SkillsMpAiSearchApiResponseData,
        SkillsMpAiSearchData as SkillsMpAiSearchPayload,
        SkillsMpErrorApiResponseData,
        SkillsMpErrorData,
        SkillsMpFiltersData,
        SkillsMpMetaData,
        SkillsMpPaginationData,
        SkillsMpSearchApiResponseData,
        SkillsMpSearchData as SkillsMpSearchPayload,
        SkillsMpSkillData,
    )


@dataclass(frozen=True)
class SkillsMpSkill:
    id: str
    name: str
    author: str
    description: str
    githubUrl: str
    skillUrl: str
    stars: int | None = None
    updatedAt: str | int | None = None

    @classmethod
    def from_data(cls, data: SkillsMpSkillData) -> "SkillsMpSkill":
        updated_at = data.get("updatedAt")
        if updated_at is not None and not isinstance(updated_at, str | int):
            raise TypeError(f"Expected string or int, got {type(updated_at)!r}")
        stars = data.get("stars")
        if stars is not None and not isinstance(stars, int):
            raise TypeError(f"Expected int, got {type(stars)!r}")
        return cls(
            id=data["id"],
            name=data["name"],
            author=data["author"],
            description=data["description"],
            githubUrl=data["githubUrl"],
            skillUrl=data["skillUrl"],
            stars=stars,
            updatedAt=updated_at,
        )


@dataclass(frozen=True)
class SkillsMpPagination:
    page: int
    limit: int
    total: int
    totalPages: int
    hasNext: bool
    hasPrev: bool
    totalIsExact: bool | None = None

    @classmethod
    def from_data(cls, data: SkillsMpPaginationData) -> "SkillsMpPagination":
        total_is_exact = data.get("totalIsExact")
        if total_is_exact is not None and not isinstance(total_is_exact, bool):
            raise TypeError(f"Expected bool, got {type(total_is_exact)!r}")
        return cls(
            page=data["page"],
            limit=data["limit"],
            total=data["total"],
            totalPages=data["totalPages"],
            hasNext=data["hasNext"],
            hasPrev=data["hasPrev"],
            totalIsExact=total_is_exact,
        )


@dataclass(frozen=True)
class SkillsMpFilters:
    search: str | None = None
    sortBy: str | None = None
    category: str | None = None
    occupation: str | None = None

    @classmethod
    def from_data(cls, data: SkillsMpFiltersData) -> "SkillsMpFilters":
        return cls(
            search=data.get("search"),
            sortBy=data.get("sortBy"),
            category=data.get("category"),
            occupation=data.get("occupation"),
        )


@dataclass(frozen=True)
class SkillsMpSearchData:
    skills: list[SkillsMpSkill]
    pagination: SkillsMpPagination
    filters: SkillsMpFilters

    @classmethod
    def from_data(cls, data: SkillsMpSearchPayload) -> "SkillsMpSearchData":
        return cls(
            skills=[SkillsMpSkill.from_data(item) for item in data["skills"]],
            pagination=SkillsMpPagination.from_data(data["pagination"]),
            filters=SkillsMpFilters.from_data(data["filters"]),
        )


@dataclass(frozen=True)
class SkillsMpAiSearchData:
    skills: list[SkillsMpSkill] = field(default_factory=list)
    results: list[SkillsMpSkill] = field(default_factory=list)

    @classmethod
    def from_data(cls, data: SkillsMpAiSearchPayload) -> "SkillsMpAiSearchData":
        skills = [SkillsMpSkill.from_data(item) for item in data.get("skills", [])]
        results = [SkillsMpSkill.from_data(item) for item in data.get("results", [])]
        return cls(skills=skills, results=results)


@dataclass(frozen=True)
class SkillsMpMeta:
    requestId: str | None = None
    responseTimeMs: int | None = None

    @classmethod
    def from_data(cls, data: SkillsMpMetaData) -> "SkillsMpMeta":
        response_time = data.get("responseTimeMs")
        if response_time is not None and not isinstance(response_time, int):
            raise TypeError(f"Expected int, got {type(response_time)!r}")
        return cls(
            requestId=data.get("requestId"),
            responseTimeMs=response_time,
        )


@dataclass(frozen=True)
class SkillsMpError:
    code: str
    message: str

    @classmethod
    def from_data(cls, data: SkillsMpErrorData) -> "SkillsMpError":
        return cls(
            code=data["code"],
            message=data["message"],
        )


@dataclass(frozen=True)
class SkillsMpSearchApiResponse:
    success: bool
    data: SkillsMpSearchData
    meta: SkillsMpMeta | None = None

    @classmethod
    def from_data(
        cls, data: SkillsMpSearchApiResponseData
    ) -> "SkillsMpSearchApiResponse":
        meta = data.get("meta")
        return cls(
            success=data["success"],
            data=SkillsMpSearchData.from_data(data["data"]),
            meta=None if meta is None else SkillsMpMeta.from_data(meta),
        )


@dataclass(frozen=True)
class SkillsMpAiSearchApiResponse:
    success: bool
    data: SkillsMpAiSearchData
    meta: SkillsMpMeta | None = None

    @classmethod
    def from_data(
        cls, data: SkillsMpAiSearchApiResponseData
    ) -> "SkillsMpAiSearchApiResponse":
        meta = data.get("meta")
        return cls(
            success=data["success"],
            data=SkillsMpAiSearchData.from_data(data["data"]),
            meta=None if meta is None else SkillsMpMeta.from_data(meta),
        )


@dataclass(frozen=True)
class SkillsMpErrorApiResponse:
    success: bool
    error: SkillsMpError
    meta: SkillsMpMeta | None = None

    @classmethod
    def from_data(
        cls, data: SkillsMpErrorApiResponseData
    ) -> "SkillsMpErrorApiResponse":
        meta = data.get("meta")
        return cls(
            success=data["success"],
            error=SkillsMpError.from_data(data["error"]),
            meta=None if meta is None else SkillsMpMeta.from_data(meta),
        )


@dataclass(frozen=True)
class ClientSettings:
    base_url: str | None = None
    api_key: str | None = None
    github_token: str | None = None
    proxy: str | None = None

    @classmethod
    def from_source(
        cls, source: bridge.ClientConfigSource | None = None
    ) -> "ClientSettings":
        if source is None:
            return cls()
        return cls(
            base_url=source.base_url,
            api_key=source.api_key,
            github_token=source.github_token,
            proxy=source.proxy,
        )

    def as_bridge_kwargs(self) -> bridge.ClientConfigKwargs:
        return {
            "base_url": self.base_url,
            "api_key": self.api_key,
            "github_token": self.github_token,
            "proxy": self.proxy,
        }


@dataclass(frozen=True)
class SkillsMpSearchQuery:
    text: str
    page: int | None = None
    limit: int | None = None
    sort_by: str | None = None
    category: str | None = None
    occupation: str | None = None


class _SkillsMpBase:
    def __init__(
        self,
        *,
        settings: ClientSettings | None = None,
        base_url: str | None = None,
        api_key: str | None = None,
        github_token: str | None = None,
        proxy: str | None = None,
    ) -> None:
        self._settings = settings or ClientSettings(
            base_url=base_url,
            api_key=api_key,
            github_token=github_token,
            proxy=proxy,
        )

    def _client_kwargs(self) -> bridge.ClientConfigKwargs:
        return self._settings.as_bridge_kwargs()

    @property
    def base_url(self) -> str | None:
        return self._settings.base_url

    @property
    def api_key(self) -> str | None:
        return self._settings.api_key

    @property
    def github_token(self) -> str | None:
        return self._settings.github_token

    @property
    def proxy(self) -> str | None:
        return self._settings.proxy

    def _search_query(self, query: str | SkillsMpSearchQuery) -> SkillsMpSearchQuery:
        if isinstance(query, SkillsMpSearchQuery):
            return query
        return SkillsMpSearchQuery(text=query)


class SkillsMp(_SkillsMpBase):
    def search(
        self,
        query: str | SkillsMpSearchQuery,
    ) -> Response[SkillsMpSearchApiResponse]:
        search_query = self._search_query(query)
        payload = bridge.skillsmp_search(
            search_query.text,
            page=search_query.page,
            limit=search_query.limit,
            sort_by=search_query.sort_by,
            category=search_query.category,
            occupation=search_query.occupation,
            **self._client_kwargs(),
        )
        return Response(payload, SkillsMpSearchApiResponse.from_data(payload))

    def ai_search(self, q: str) -> Response[SkillsMpAiSearchApiResponse]:
        payload = bridge.skillsmp_ai_search(q, **self._client_kwargs())
        return Response(payload, SkillsMpAiSearchApiResponse.from_data(payload))

    def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        payload = bridge.skillsmp_fetch_github_directory(
            location.url,
            current_path.as_posix(),
            **self._client_kwargs(),
        )
        return [GitHubContentItem.from_data(item) for item in payload]

    def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        payload = bridge.skillsmp_fetch_github_file(
            location.url,
            path.as_posix(),
            **self._client_kwargs(),
        )
        return GitHubFileBlob.from_data(payload)

    def fetch_github_snapshot(
        self,
        location: GitHubSkillLocation,
    ) -> GitHubRepositorySnapshot:
        payload = bridge.skillsmp_fetch_github_snapshot(
            location.url,
            **self._client_kwargs(),
        )
        return GitHubRepositorySnapshot.from_data(payload)

    def resolve_github_ref_and_commit_sha(
        self,
        location: GitHubSkillLocation,
    ) -> tuple[str, str]:
        payload = bridge.skillsmp_resolve_github_ref_and_commit_sha(
            location.url,
            **self._client_kwargs(),
        )
        if len(payload) != 2:
            raise ValueError(f"Expected 2 values, got {len(payload)}")
        return payload[0], payload[1]


class AsyncSkillsMp(_SkillsMpBase):
    def _sync(self) -> SkillsMp:
        return SkillsMp(settings=self._settings)

    async def search(
        self,
        query: str | SkillsMpSearchQuery,
    ) -> AsyncResponse[SkillsMpSearchApiResponse]:
        response = self._sync().search(query)
        return AsyncResponse(response.raw_response, response.parsed_data)

    async def ai_search(self, q: str) -> AsyncResponse[SkillsMpAiSearchApiResponse]:
        response = self._sync().ai_search(q)
        return AsyncResponse(response.raw_response, response.parsed_data)

    async def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        return self._sync().fetch_github_directory(location, current_path)

    async def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        return self._sync().fetch_github_file(location, path)

    async def fetch_github_snapshot(
        self,
        location: GitHubSkillLocation,
    ) -> GitHubRepositorySnapshot:
        return self._sync().fetch_github_snapshot(location)

    async def resolve_github_ref_and_commit_sha(
        self,
        location: GitHubSkillLocation,
    ) -> tuple[str, str]:
        return self._sync().resolve_github_ref_and_commit_sha(location)
