from __future__ import annotations

import json
from collections.abc import AsyncIterator, Iterator
from typing import Generic, TypeVar

DataType = TypeVar("DataType")


class Response(Generic[DataType]):
    __slots__ = ("_parsed_data", "_payload")

    def __init__(self, payload: object, parsed_data: DataType) -> None:
        self._payload = payload
        self._parsed_data = parsed_data

    @property
    def request(self) -> None:
        return None

    @property
    def raw_response(self) -> object:
        return self._payload

    @property
    def status_code(self) -> int | None:
        return 200

    @property
    def ok(self) -> bool:
        return True

    @property
    def is_redirect(self) -> bool:
        return False

    @property
    def content(self) -> bytes:
        return json.dumps(self._payload).encode()

    @property
    def text(self) -> str:
        return json.dumps(self._payload)

    def json(self, **kwargs: object) -> object:
        del kwargs
        return self._payload

    @property
    def parsed_data(self) -> DataType:
        return self._parsed_data

    def raise_for_status(self) -> None:
        return None

    def iter_raw(self, chunk_size: int = 0) -> Iterator[bytes]:
        del chunk_size
        yield self.content

    def iter_content(
        self, chunk_size: int = 0, decode_unicode: bool = False
    ) -> Iterator[bytes | str]:
        del chunk_size
        if decode_unicode:
            yield self.text
            return
        yield self.content

    def iter_lines(
        self,
        chunk_size: int = 0,
        decode_unicode: bool = False,
        delimiter: str | bytes | None = None,
    ) -> Iterator[bytes | str]:
        del chunk_size, delimiter
        if decode_unicode:
            yield self.text
            return
        yield self.content

    def iter_lines_parsed(self) -> Iterator[DataType]:
        yield self.parsed_data


class AsyncResponse(Generic[DataType]):
    __slots__ = ("_response",)

    def __init__(self, payload: object, parsed_data: DataType) -> None:
        self._response = Response(payload, parsed_data)

    @property
    def request(self) -> None:
        return None

    @property
    def raw_response(self) -> object:
        return self._response.raw_response

    @property
    def status_code(self) -> int | None:
        return 200

    @property
    def ok(self) -> bool:
        return True

    @property
    def is_redirect(self) -> bool:
        return False

    @property
    async def content(self) -> bytes:
        return self._response.content

    @property
    async def text(self) -> str:
        return self._response.text

    async def json(self, **kwargs: object) -> object:
        del kwargs
        return self._response.raw_response

    @property
    async def parsed_data(self) -> DataType:
        return self._response.parsed_data

    def raise_for_status(self) -> None:
        return None

    async def iter_raw(self, chunk_size: int = 0) -> AsyncIterator[bytes]:
        del chunk_size
        yield self._response.content

    async def iter_content(
        self, chunk_size: int = 0, decode_unicode: bool = False
    ) -> AsyncIterator[bytes | str]:
        del chunk_size
        if decode_unicode:
            yield self._response.text
            return
        yield self._response.content

    async def iter_lines(
        self,
        chunk_size: int = 0,
        decode_unicode: bool = False,
        delimiter: str | bytes | None = None,
    ) -> AsyncIterator[bytes | str]:
        del chunk_size, delimiter
        if decode_unicode:
            yield self._response.text
            return
        yield self._response.content

    async def iter_lines_parsed(self) -> AsyncIterator[DataType]:
        yield self._response.parsed_data
