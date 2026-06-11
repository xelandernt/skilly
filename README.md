[![npm](https://img.shields.io/npm/v/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![pypi](https://img.shields.io/pypi/v/skilly)](https://pypi.org/project/skilly/)
[![pyrefly](https://img.shields.io/endpoint?url=https://pyrefly.org/badge.json)](https://github.com/facebook/pyrefly)
[![GitHub stars](https://img.shields.io/github/stars/xelandernt/skilly)](https://github.com/xelandernt/skilly/stargazers)
[![npm Downloads](https://img.shields.io/npm/dm/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![PyPI Downloads](https://static.pepy.tech/badge/skilly/month)](https://pepy.tech/projects/skilly)
[![License](https://img.shields.io/github/license/xelandernt/skilly)](https://github.com/xelandernt/skilly/blob/main/LICENSE)

# skilly

Manage [Agent Skills](https://agentskills.io/specification) from the command line or Python.

`skilly` creates specification-compliant skills, installs skills from GitHub or Python dependencies, and keeps managed skills up to date.

## Table of Contents

- [Installation](#installation)
  - [Python](#python)
  - [Node](#node)
  - [Homebrew](#homebrew)
  - [Info](#info)
- [Quick Start](#quick-start)
- [CLI Commands](#cli-commands)
  - [Create Skills](#create-skills)
  - [Install Dependency Skills](#install-dependency-skills)
  - [Install GitHub Skills](#install-github-skills)
  - [Destinations](#destinations)
  - [Configure Destinations](#configure-destinations)
  - [GitHub Authentication](#github-authentication)
- [Python API](#python-api)
- [Development](#development)
- [License](#license)

## Installation


### Python
```shell
uvx skilly --help
```

### Node
```shell
npx @xelandernt/skilly --help
```


### Homebrew
```shell
brew tap xelandernt/skilly https://github.com/xelandernt/skilly
brew install xelandernt/skilly/skilly
```

### Info

| Method        | Ships                                                                                                |
|---------------|------------------------------------------------------------------------------------------------------|
| `uvx` / `pip` | Python package with import surface + CLI (pre-built wheels: Linux x64, macOS arm64/x64, Windows x64) |
| `npx`         | Native Rust CLI (macOS arm64/x64, Linux x64 glibc, Windows x64)                                      |
| `brew`        | Native Rust CLI (macOS arm64/x64, Linux x64)                                                         |

## Quick Start

```shell
uvx skilly scan                       # Find skills from Python dependencies
uvx skilly download <github-url>      # Install skills from GitHub
uvx skilly list                       # Browse installed skills
uvx skilly update                     # Preview available updates
```

## CLI Commands

| Command | Purpose |
|---------|---------|
| `scan` | Find skills provided by the project's Python dependencies |
| `download <github-url>` | Install one or more skills from GitHub |
| `list` | Browse, update, or remove installed skills |
| `update` | Preview available updates; `--yes` applies all |
| `remove <name>` | Remove an installed skill by directory name |
| `skillsmp search <query>` | Search SkillsMP and install a selected result |
| `create` | Create a valid skill through a terminal wizard or explicit options |
| `configure` | Set which directories skilly manages via TUI or CLI flags |

Run `skilly <command> --help` for all options.

### Create Skills

Interactive terminals open a full-screen editor. `Ctrl+S` creates the skill, `Ctrl+X` cancels.

```shell
uvx skilly create deployment-checks \
  --description "Validate deployment readiness before a production release." \
  --instructions "# Instructions

Run the deployment checklist and report blockers." \
  --metadata owner=platform \
  --with-scripts \
  --yes
```

Existing skills are rejected unless `--overwrite` is passed.

### Install Dependency Skills

`skilly scan` reads `pyproject.toml` and the project's `.venv`, then surfaces skills shipped by dependencies:

```shell
uvx skilly scan
```

Include or exclude specific extras and dependency groups (flags are repeatable):

```shell
uvx skilly scan --group dev --group test --exclude-extra docs
```

### Install GitHub Skills

```shell
uvx skilly download https://github.com/example/project
```

When a repository contains multiple skills:

```shell
uvx skilly download https://github.com/example/project --all
uvx skilly download https://github.com/example/project --skill-name code-review
```

### Destinations

All management commands accept the same destination options:

```shell
uvx skilly list --local        # .agents/skills
uvx skilly list --global       # ~/.agents/skills
uvx skilly list --claude       # .claude/skills
uvx skilly list --codex        # .codex/skills
uvx skilly list --copilot      # .github/skills (local), ~/.copilot/skills (global)
uvx skilly list --directory ~/custom     # Explicit directory
```

| Flags                | Resolved destination                                  |
|----------------------|-------------------------------------------------------|
| _none_               | `SKILLY_DIRECTORY` if set, otherwise `.agents/skills` |
| `--local`            | `.agents/skills`                                      |
| `--global`           | `~/.agents/skills`                                    |
| `--claude`           | `.claude/skills`                                      |
| `--claude --global`  | `~/.claude/skills`                                    |
| `--codex`            | `.codex/skills`                                       |
| `--codex --global`   | `~/.codex/skills`                                     |
| `--copilot`          | `.github/skills`                                      |
| `--copilot --global` | `~/.copilot/skills`                                   |
| `--directory <path>` | That directory (after `~` expansion)                  |

Set a default destination:

```shell
export SKILLY_DIRECTORY="$HOME/.config/skilly/skills"
```

`--directory` overrides all other destination options and `SKILLY_DIRECTORY`.

### Configure Destinations

`skilly configure` lets you choose which directories skilly should manage. Interactive terminals open a two-tab TUI (Global / Local). Non-interactive runs accept flags.

```shell
uvx skilly configure                 # Open the TUI
uvx skilly configure --show          # Print current config as TOML
uvx skilly configure --reset         # Restore defaults
```

Add or remove custom directories:

```shell
uvx skilly configure --add-global /opt/skills
uvx skilly configure --add-local .project/skills
uvx skilly configure --remove-global /opt/skills
uvx skilly configure --remove-local .project/skills
```

Enable or disable built-in destinations (valid keys: `agents_global`, `agents_local`, `claude_global`, `claude_local`, `codex_global`, `codex_local`, `copilot_global`, `copilot_local`):

```shell
uvx skilly configure --enable agents_global --disable copilot_global
```

Configuration is stored in `~/.skilly.toml`:

```toml
enabled_builtin = ["agents_global", "agents_local", ...]
custom_global_dirs = []
custom_local_dirs = []
```

### GitHub Authentication

Set a token for higher API rate limits (first available wins: `SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`):

```shell
export SKILLY_GITHUB_TOKEN=ghp_your_token
```

All GitHub-fetching commands also accept `--github-token`.

## Python API

`SkillRepository` bundles a directory and project settings for stateful workflows:

```python
from pathlib import Path
from skilly import ProjectSettings, Skill, SkillRepository

repository = SkillRepository(
    directory=Path(".agents/skills"),
    project=ProjectSettings(
        dependency_groups=("dev",),
        optional_dependencies=("docs",),
    ),
)

repository.install(
    Skill(
        name="code-review",
        description="Review code for correctness and maintainability.",
        content="# Instructions\n\nReview the proposed change.",
    )
)

for match in repository.scan_project():
    print(match.available.name, match.status)
```

Stateless discovery functions for one-shot reads:

```python
from skilly import discover_installed_skills, discover_venv_skills

installed = discover_installed_skills()
dependency_skills = discover_venv_skills()
```

SkillsMP client with typed results:

```python
from skilly.skillsmp import ClientSettings, SkillsMp, SkillsMpSearchQuery

client = SkillsMp(settings=ClientSettings(base_url="https://skillsmp.com/api/v1"))
result = client.search(SkillsMpSearchQuery(text="python", limit=5))
print(result.data.skills[0].github_url)
```

## Development

```shell
just install      # Install dev dependencies + editable extension
just lint         # Run linters
just test         # Run tests
just typecheck    # Run type checkers
```

## License

[MIT](LICENSE)
