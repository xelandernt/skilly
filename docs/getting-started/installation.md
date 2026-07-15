# Installation

skilly is distributed as a Python package (with a native Rust extension), a
standalone npm package, and a Homebrew formula. All three deliver the same CLI;
the Python distribution also provides the import surface.

## Python (`uvx` / `pip`)

```shell
uvx skilly --help
```

Or install into a project:

```shell
uv add skilly
# or
pip install skilly
```

**Ships:** Python package with full import surface + CLI. Pre-built wheels for
Linux x64, macOS arm64/x64, and Windows x64.

## Node (`npx`)

```shell
npx @xelandernt/skilly --help
```

**Ships:** Native Rust CLI. Pre-built binaries for macOS arm64/x64, Linux x64
glibc, and Windows x64.

Note: The npm distribution does not include the Python import surface.

## Homebrew

```shell
brew tap xelandernt/skilly https://github.com/xelandernt/skilly
brew install xelandernt/skilly/skilly
```

**Ships:** Native Rust CLI. Pre-built binaries for macOS arm64/x64 and Linux
x64.

## Capability comparison

| Method | CLI | Python API | Supported platforms |
|---|---|---|---|
| `uvx` / `pip` | Yes | Yes | Linux x64, macOS arm64/x64, Windows x64 |
| `npx` | Yes | No | macOS arm64/x64, Linux x64 glibc, Windows x64 |
| `brew` | Yes | No | macOS arm64/x64, Linux x64 |

For the Python API, use the `uv` or `pip` installation. For CLI-only usage, any
method works.

## Verifying the installation

```shell
skilly --version
skilly --help
```
