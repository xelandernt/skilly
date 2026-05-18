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
            id=bridge.ensure_string(data["id"]),
            name=bridge.ensure_string(data["name"]),
            author=bridge.ensure_string(data["author"]),
            description=bridge.ensure_string(data["description"]),
            githubUrl=bridge.ensure_string(data["githubUrl"]),
            skillUrl=bridge.ensure_string(data["skillUrl"]),
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
            page=_expect_int(data["page"]),
            limit=_expect_int(data["limit"]),
            total=_expect_int(data["total"]),
            totalPages=_expect_int(data["totalPages"]),
            hasNext=_expect_bool(data["hasNext"]),
            hasPrev=_expect_bool(data["hasPrev"]),
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
            search=_optional_string(data.get("search")),
            sortBy=_optional_string(data.get("sortBy")),
            category=_optional_string(data.get("category")),
            occupation=_optional_string(data.get("occupation")),
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
            requestId=_optional_string(data.get("requestId")),
            responseTimeMs=response_time,
        )


@dataclass(frozen=True)
class SkillsMpError:
    code: str
    message: str

    @classmethod
    def from_data(cls, data: SkillsMpErrorData) -> "SkillsMpError":
        return cls(
            code=bridge.ensure_string(data["code"]),
            message=bridge.ensure_string(data["message"]),
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
            success=_expect_bool(data["success"]),
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
            success=_expect_bool(data["success"]),
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
            success=_expect_bool(data["success"]),
            error=SkillsMpError.from_data(data["error"]),
            meta=None if meta is None else SkillsMpMeta.from_data(meta),
        )


class _SkillsMpBase:
    def __init__(
        self,
        *,
        base_url: str | None = None,
        api_key: str | None = None,
        github_token: str | None = None,
        proxy: str | None = None,
    ) -> None:
        self.base_url = base_url
        self.api_key = api_key
        self.github_token = github_token
        self.proxy = proxy

    def _client_kwargs(self) -> bridge.ClientConfigKwargs:
        return {
            "base_url": self.base_url,
            "api_key": self.api_key,
            "github_token": self.github_token,
            "proxy": self.proxy,
        }


class SkillsMp(_SkillsMpBase):
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
        payload = bridge.skillsmp_search(
            q,
            page=page,
            limit=limit,
            sort_by=sort_by,
            category=category,
            occupation=occupation,
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
        return bridge.ensure_string(payload[0]), bridge.ensure_string(payload[1])


class AsyncSkillsMp(_SkillsMpBase):
    def _sync(self) -> SkillsMp:
        return SkillsMp(
            base_url=self.base_url,
            api_key=self.api_key,
            github_token=self.github_token,
            proxy=self.proxy,
        )

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
        response = self._sync().search(
            q,
            page=page,
            limit=limit,
            sort_by=sort_by,
            category=category,
            occupation=occupation,
        )
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


def _expect_bool(value: bridge.BridgeValue) -> bool:
    if not isinstance(value, bool):
        raise TypeError(f"Expected bool, got {type(value)!r}")
    return value


def _expect_int(value: bridge.BridgeValue) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise TypeError(f"Expected int, got {type(value)!r}")
    return value


def _optional_string(value: bridge.BridgeValue | None) -> str | None:
    if value is None:
        return None
    return bridge.ensure_string(value)
