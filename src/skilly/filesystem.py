from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path
from typing import Protocol


class FileSystem(Protocol):
    def read_file(self, path: Path) -> str:
        """Read a file and return its contents."""

    def write_file(self, path: Path, content: str) -> None:
        """Write text to a file path."""

    def list_files(self, path: Path) -> Sequence[str]:
        """List child entry names in a directory."""

    def exists(self, path: Path) -> bool:
        """Return whether the path exists."""

    def is_dir(self, path: Path) -> bool:
        """Return whether the path is a directory."""

    def make_dir(
        self, path: Path, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        """Create a directory."""

    def remove_tree(self, path: Path) -> None:
        """Remove a directory tree."""

    def resolve(self, path: Path) -> Path:
        """Return a normalized absolute path."""


__all__ = ["FileSystem"]
