mod ts

[default]
[private]
_:
    @just --list --list-submodules

# install dependencies
install:
    uv lock --upgrade
    uv sync --all-extras --frozen
    uv run --no-sync maturin develop
    @just ts::install
    @just hook

# lint project
lint:
    uv run prek run --all-files

# test project
test *args:
    uv run --no-sync cargo test --profile release
    uv run --no-sync maturin develop
    uv run --no-sync pytest {{ args }}
    @just ts::test

# type check project
typecheck:
    uv run pyrefly check
    @just ts::typecheck

# install pre-commit hooks
hook:
    uv run prek install --install-hooks --overwrite

# uninstall pre-commit hooks
unhook:
    uv run prek uninstall

# bump version and install
bump component="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    component="{{ component }}"
    cargo_toml="Cargo.toml"
    package_json="ts/package.json"

    current="$(awk '/^version *= */ { gsub(/[" ]/, "", $3); print $3; exit }' "$cargo_toml")"
    IFS='.' read -r major minor patch <<< "$current"

    case "$component" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
        *) echo "Unknown component: $component (use major, minor, or patch)" >&2; exit 2 ;;
    esac

    new_version="${major}.${minor}.${patch}"
    echo "Bumping version: ${current} -> ${new_version} (${component})"

    if [[ "$(uname)" == "Darwin" ]]; then
        sed -i '' "s/^version = \".*\"/version = \"${new_version}\"/" "$cargo_toml"
        sed -i '' "s/\"version\": \".*\"/\"version\": \"${new_version}\"/" "$package_json"
    else
        sed -i "s/^version = \".*\"/version = \"${new_version}\"/" "$cargo_toml"
        sed -i "s/\"version\": \".*\"/\"version\": \"${new_version}\"/" "$package_json"
    fi

    just install

# preview documentation site
docs:
    @just docs-build
    uv run --group docs zensical serve

# build documentation site (strict, production)
docs-build:
    rm -rf site
    uv run --group docs zensical build --strict

# publish project on pypi
publish:
    rm -rf dist
    uv build
    uv publish --token $PYPI_TOKEN
