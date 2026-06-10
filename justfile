mod ts

default:
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

# publish project on pypi
publish:
    rm -rf dist
    uv build
    uv publish --token $PYPI_TOKEN
