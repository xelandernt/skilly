# Custom filesystems

skilly supports custom filesystem backends through the `FileSystem` protocol.
This allows operations to work against non-native filesystems (e.g. in-memory
stores, remote storage, or test fixtures).

## FileSystem protocol

```python
from skilly import FileSystem

class FileSystem(Protocol):
    def read_bytes(self, path: StrPath, max_size: int) -> bytes: ...
    def write_bytes(self, path: StrPath, content: bytes) -> None: ...
    def list_files(self, path: StrPath) -> Sequence[str]: ...
    def exists(self, path: StrPath) -> bool: ...
    def is_dir(self, path: StrPath) -> bool: ...
    def make_dir(
        self, path: StrPath, *, parents: bool = False, exist_ok: bool = False
    ) -> None: ...
    def remove_tree(self, path: StrPath) -> None: ...
    def replace_tree(self, path: StrPath, replacement: StrPath) -> None: ...
    def resolve(self, path: StrPath) -> StrPath: ...
```

### Method semantics

| Method | Description |
|---|---|
| `read_bytes` | Read a file, bounded by `max_size` bytes. |
| `write_bytes` | Write raw bytes to a file. |
| `list_files` | List child entry names in a directory (not full paths). |
| `exists` | Return whether the path exists. |
| `is_dir` | Return whether the path is a directory. |
| `make_dir` | Create a directory with optional `parents` and `exist_ok`. |
| `remove_tree` | Recursively remove a directory tree. |
| `replace_tree` | Atomically replace a directory tree with a prepared replacement. |
| `resolve` | Return a normalized absolute path. |

## Using a custom filesystem

Pass `file_system=` to `SkillRepository` or any discovery function:

```python
from skilly import SkillRepository

class MyFileSystem:
    def read_bytes(self, path, max_size): ...
    def write_bytes(self, path, content): ...
    # ... implement all FileSystem methods

repository = SkillRepository(
    file_system=MyFileSystem(),
)
```

## When to use

-   **Testing** — use an in-memory filesystem to avoid disk I/O.
-   **Sandboxing** — restrict skill operations to a virtual directory.
-   **Remote storage** — back skill storage with a database or object store.
