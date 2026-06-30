from __future__ import annotations

from collections.abc import Sequence
from os import PathLike
from typing import Protocol, TypeAlias

StrPath: TypeAlias = str | PathLike[str]


class FileSystem(Protocol):
    def read_bytes(self, path: StrPath, max_size: int) -> bytes:
        """Read a file and return its raw bytes, bounded by max_size."""

    def write_bytes(self, path: StrPath, content: bytes) -> None:
        """Write bytes to a file path."""

    def list_files(self, path: StrPath) -> Sequence[str]:
        """List child entry names in a directory."""

    def exists(self, path: StrPath) -> bool:
        """Return whether the path exists."""

    def is_dir(self, path: StrPath) -> bool:
        """Return whether the path is a directory."""

    def make_dir(
        self, path: StrPath, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        """Create a directory."""

    def remove_tree(self, path: StrPath) -> None:
        """Remove a directory tree."""

    def replace_tree(self, path: StrPath, replacement: StrPath) -> None:
        """Atomically replace a directory tree with a prepared replacement."""

    def resolve(self, path: StrPath) -> StrPath:
        """Return a normalized absolute path using the filesystem's native flavor."""
