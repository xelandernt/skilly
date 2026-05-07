[private]
default:
    @just --list

# update dependencies
update:
    uv lock --upgrade
    @just install

# upgrade dependencies
upgrade:
    @just update

# install dependencies
install:
    uv sync --all-extras --frozen
    @just hook

# lint project
lint:
    uv run prek run --all-files

# test project
test *args:
    uv run --no-sync pytest {{ args }}

# type check project
typecheck:
    uv run pyrefly check

# install pre-commit hooks
hook:
    uv run prek install --install-hooks --overwrite

# uninstall pre-commit hooks
unhook:
    uv run prek uninstall
