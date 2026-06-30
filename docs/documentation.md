# Documentation conventions

## Sources of truth

-   **CLI** — `src/cli/args.rs`. Verify with `skilly <command> --help`.
-   **Python API** — `skilly.__all__` and `skilly.skillsmp.__all__`.
-   **Agent Skills format** — link to the [Agent Skills
    specification](https://agentskills.io/specification) rather than
    duplicating it.

## Navigation

The navigation tree is defined explicitly in `zensical.toml`. Every page must be
listed there. Avoid auto-generated navigation: keep URLs and page titles
explicit.

## Link stability

-   Internal links use relative paths (e.g. `../cli/index.md`).
-   Published URLs are considered stable after the initial release. Rename pages
    only with a migration plan or redirect.

## Markdown features

This site uses standard Markdown with Zensical's theme features:

-   Admonitions for notes, warnings, and tips.
-   Code blocks with language tags for copyable examples.
-   Data tables for parameter and option references.

## Build

-   `just docs-build` runs a strict build with validation enabled. Warnings for
    absolute links, unrecognized links, and broken anchors are promoted to
    errors in CI.
-   Generated output in `site/` is gitignored. Never commit it.

## Docstrings

Do not rely on auto-generated API reference from docstrings for the first
release. Most public objects do not yet have complete docstrings, and
importing the compiled Rust extension during a docs build would couple
deployment to the build toolchain. Use hand-authored reference pages instead.

## Python examples

Examples should use supported import paths (`from skilly import ...`) and
current CLI options. Avoid documenting `_core`, `_bridge`, or other internal
modules as public API.
