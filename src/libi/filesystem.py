import os
from pathlib import Path
from typing import Protocol, Sequence, Final


class FileSystem(Protocol):
    def read_file(self, path: Path) -> str:
        """Read a file and return its contents."""

    def list_files(self, path: Path) -> Sequence[str]:
        """List child entry names in a directory."""

    def is_dir(self, path: Path) -> bool:
        """Return whether the path is a directory."""

    def resolve(self, path: Path) -> Path:
        """Return a normalized absolute path."""


class DefaultFileSystem(FileSystem):
    def read_file(self, path: Path) -> str:
        with open(path, encoding="utf-8") as file_handle:
            return file_handle.read()

    def list_files(self, path: Path) -> Sequence[str]:
        return os.listdir(path)

    def is_dir(self, path: Path) -> bool:
        return os.path.isdir(path)

    def resolve(self, path: Path) -> Path:
        return path.resolve()


DEFAULT_FILE_SYSTEM: Final[FileSystem] = DefaultFileSystem()
