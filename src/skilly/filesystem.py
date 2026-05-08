import os
import shutil
from pathlib import Path
from typing import Final, Protocol, Sequence


class FileSystem(Protocol):
    def read_file(self, path: Path) -> str:
        """Read a file and return its contents."""

    def read_bytes(self, path: Path) -> bytes:
        """Read a file and return its bytes."""

    def write_bytes(self, path: Path, content: bytes) -> None:
        """Write bytes to a file path."""

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


class DefaultFileSystem(FileSystem):
    def read_file(self, path: Path) -> str:
        with open(path, encoding="utf-8") as file_handle:
            return file_handle.read()

    def read_bytes(self, path: Path) -> bytes:
        with open(path, "rb") as file_handle:
            return file_handle.read()

    def write_bytes(self, path: Path, content: bytes) -> None:
        with open(path, "wb") as file_handle:
            file_handle.write(content)

    def list_files(self, path: Path) -> Sequence[str]:
        return os.listdir(path)

    def exists(self, path: Path) -> bool:
        return os.path.exists(path)

    def is_dir(self, path: Path) -> bool:
        return os.path.isdir(path)

    def make_dir(
        self, path: Path, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        path.mkdir(parents=parents, exist_ok=exist_ok)

    def remove_tree(self, path: Path) -> None:
        shutil.rmtree(path)

    def resolve(self, path: Path) -> Path:
        return path.resolve()


DEFAULT_FILE_SYSTEM: Final[FileSystem] = DefaultFileSystem()
