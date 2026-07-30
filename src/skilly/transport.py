"""Typed network boundary for repository-backed skill discovery."""

from collections.abc import Mapping
from typing import Protocol


class RepositoryTransport(Protocol):
    """Caller-owned network boundary for repository discovery.

    Implementations must apply their own DNS, redirect, timeout, response-size,
    concurrency, authentication, and successful-status policies before returning
    the complete response body.
    """

    def get(
        self,
        url: str,
        *,
        headers: Mapping[str, str],
        params: Mapping[str, str],
    ) -> bytes: ...
