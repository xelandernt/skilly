from __future__ import annotations

from collections.abc import AsyncIterator, Awaitable, Callable, Iterator
from typing import overload
from typing import Generic, TypeVar

import niquests
from niquests.models import ITER_CHUNK_SIZE
from pydantic import BaseModel

DataType = TypeVar("DataType", bound=BaseModel)
LineParser = Callable[[str], DataType]


async def _ensure_async_bytes_iterator(
    value: AsyncIterator[bytes] | Awaitable[AsyncIterator[bytes]],
) -> AsyncIterator[bytes]:
    if isinstance(value, Awaitable):
        return await value
    return value


async def _ensure_async_text_iterator(
    value: AsyncIterator[str] | Awaitable[AsyncIterator[str]],
) -> AsyncIterator[str]:
    if isinstance(value, Awaitable):
        return await value
    return value


@overload
async def _resolve_async_value(value: Awaitable[DataType] | DataType) -> DataType: ...


@overload
async def _resolve_async_value(
    value: Awaitable[bytes] | bytes | None,
) -> bytes | None: ...


@overload
async def _resolve_async_value(value: Awaitable[str] | str | None) -> str | None: ...


async def _resolve_async_value(value: object) -> object:
    if isinstance(value, Awaitable):
        return await value
    return value


class Response(Generic[DataType]):
    __slots__ = ("_line_parser", "_model", "_parsed_data", "_response")

    def __init__(
        self,
        response: niquests.Response,
        model: type[DataType],
        *,
        line_parser: LineParser[DataType] | None = None,
    ) -> None:
        self._response = response
        self._model = model
        self._line_parser = line_parser
        self._parsed_data: DataType | None = None

    @property
    def request(self) -> niquests.PreparedRequest | None:
        return self._response.request

    @property
    def raw_response(self) -> niquests.Response:
        return self._response

    @property
    def status_code(self) -> int | None:
        return self._response.status_code

    @property
    def ok(self) -> bool:
        return self._response.ok

    @property
    def is_redirect(self) -> bool:
        return self._response.is_redirect

    @property
    def content(self) -> bytes | None:
        return self._response.content

    @property
    def text(self) -> str | None:
        return self._response.text

    def json(self, **kwargs: object) -> object:
        return self._response.json(**kwargs)

    @property
    def parsed_data(self) -> DataType:
        if self._parsed_data is None:
            self._parsed_data = self._model.model_validate(self.json())
        return self._parsed_data

    def raise_for_status(self) -> None:
        self._response.raise_for_status()

    def iter_raw(self, chunk_size: int = ITER_CHUNK_SIZE) -> Iterator[bytes]:
        yield from self._response.iter_raw(chunk_size)

    def iter_content(
        self, chunk_size: int = ITER_CHUNK_SIZE, decode_unicode: bool = False
    ) -> Iterator[bytes | str]:
        if decode_unicode:
            yield from self._response.iter_content(
                chunk_size,
                decode_unicode=True,
            )
            return

        yield from self._response.iter_content(
            chunk_size,
            decode_unicode=False,
        )

    def iter_lines(
        self,
        chunk_size: int = ITER_CHUNK_SIZE,
        decode_unicode: bool = False,
        delimiter: str | bytes | None = None,
    ) -> Iterator[bytes | str]:
        if decode_unicode:
            yield from self._response.iter_lines(
                chunk_size,
                decode_unicode=True,
                delimiter=delimiter,
            )
            return

        yield from self._response.iter_lines(
            chunk_size,
            decode_unicode=False,
            delimiter=delimiter,
        )

    def _parse_line(self, line: str) -> DataType:
        if self._line_parser is not None:
            return self._line_parser(line)
        return self._model.model_validate(line)

    def iter_lines_parsed(self) -> Iterator[DataType]:
        for line in self.iter_lines(decode_unicode=True):
            parsed_line = line if isinstance(line, str) else line.decode()
            yield self._parse_line(parsed_line)


class AsyncResponse(Generic[DataType]):
    __slots__ = ("_line_parser", "_model", "_parsed_data", "_response")

    def __init__(
        self,
        response: niquests.AsyncResponse,
        model: type[DataType],
        *,
        line_parser: LineParser[DataType] | None = None,
    ) -> None:
        self._response = response
        self._model = model
        self._line_parser = line_parser
        self._parsed_data: DataType | None = None

    @property
    def request(self) -> niquests.PreparedRequest | None:
        return self._response.request

    @property
    def raw_response(self) -> niquests.AsyncResponse:
        return self._response

    @property
    def status_code(self) -> int | None:
        return self._response.status_code

    @property
    def ok(self) -> bool:
        return self._response.ok

    @property
    def is_redirect(self) -> bool:
        return self._response.is_redirect

    @property
    async def content(self) -> bytes | None:
        resolved = await _resolve_async_value(self._response.content)
        return (
            resolved
            if isinstance(resolved, bytes) or resolved is None
            else bytes(resolved)
        )

    @property
    async def text(self) -> str | None:
        resolved = await _resolve_async_value(self._response.text)
        return (
            resolved if isinstance(resolved, str) or resolved is None else str(resolved)
        )

    async def json(self, **kwargs: object) -> object:
        return await _resolve_async_value(self._response.json(**kwargs))

    @property
    async def parsed_data(self) -> DataType:
        if self._parsed_data is None:
            self._parsed_data = self._model.model_validate(await self.json())
        return self._parsed_data

    def raise_for_status(self) -> None:
        self._response.raise_for_status()

    async def iter_raw(self, chunk_size: int = ITER_CHUNK_SIZE) -> AsyncIterator[bytes]:
        iterator = await _ensure_async_bytes_iterator(
            self._response.iter_raw(chunk_size)
        )
        async for value in iterator:
            yield value

    async def iter_content(
        self, chunk_size: int = ITER_CHUNK_SIZE, decode_unicode: bool = False
    ) -> AsyncIterator[bytes | str]:
        if decode_unicode:
            text_iterator = await _ensure_async_text_iterator(
                self._response.iter_content(
                    chunk_size,
                    decode_unicode=True,
                )
            )
            async for text_value in text_iterator:
                yield text_value
            return

        bytes_iterator = await _ensure_async_bytes_iterator(
            self._response.iter_content(
                chunk_size,
                decode_unicode=False,
            )
        )
        async for bytes_value in bytes_iterator:
            yield bytes_value

    async def iter_lines(
        self,
        chunk_size: int = ITER_CHUNK_SIZE,
        decode_unicode: bool = False,
        delimiter: str | bytes | None = None,
    ) -> AsyncIterator[bytes | str]:
        if decode_unicode:
            text_iterator = await _ensure_async_text_iterator(
                self._response.iter_lines(
                    chunk_size,
                    decode_unicode=True,
                    delimiter=delimiter,
                )
            )
            async for text_value in text_iterator:
                yield text_value
            return

        bytes_iterator = await _ensure_async_bytes_iterator(
            self._response.iter_lines(
                chunk_size,
                decode_unicode=False,
                delimiter=delimiter,
            )
        )
        async for bytes_value in bytes_iterator:
            yield bytes_value

    def _parse_line(self, line: str) -> DataType:
        if self._line_parser is not None:
            return self._line_parser(line)
        return self._model.model_validate(line)

    async def iter_lines_parsed(self) -> AsyncIterator[DataType]:
        async for line in self.iter_lines(decode_unicode=True):
            parsed_line = line if isinstance(line, str) else line.decode()
            yield self._parse_line(parsed_line)
