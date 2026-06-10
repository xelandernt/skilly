# @xelandernt/skilly

Run the native `skilly` CLI through `npx` without a separate TypeScript or
JavaScript CLI implementation.

## Usage

```shell
npx @xelandernt/skilly --help
```

The package preserves the native CLI's stdio behavior so interactive TUI flows
and non-interactive automation keep using the same Rust implementation.

## Supported targets

- macOS arm64
- macOS x64
- Linux x64 (glibc)
- Windows x64
