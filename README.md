[![npm](https://img.shields.io/npm/v/%40xelandernt%2Fskilly
)](https://www.npmjs.com/package/@xelandernt/skilly)
[![pyrefly](https://img.shields.io/endpoint?url=https://pyrefly.org/badge.json)](https://github.com/facebook/pyrefly)
[![GitHub stars](https://img.shields.io/github/stars/xelandernt/skilly)](https://github.com/xelandernt/skilly/stargazers)
[![PyPI Downloads](https://static.pepy.tech/badge/skilly/month)](https://pepy.tech/projects/skilly)
[![Supported versions](https://img.shields.io/pypi/pyversions/skilly.svg)](https://pypi.python.org/pypi/skilly)
# skilly

Manage [Agent Skills](https://agentskills.io/specification) from the command line
or Python.

`skilly` creates specification-compliant skills, installs skills from GitHub or
Python dependencies, and keeps managed skills up to date.

## Installation

Run with `uvx`:
```shell
uvx skilly --help

```

Run with `npx`:
```shell
npx @xelandernt/skilly --help
```

Install the native CLI with Homebrew from this repository's tap:

```shell
brew tap xelandernt/skilly https://github.com/xelandernt/skilly
brew install xelandernt/skilly/skilly
```

Homebrew installs the CLI only. Install the Python package when you need the
import surface as well:

```shell
pip install skilly
```

The npm package ships the same native Rust CLI behind the executable name
`skilly`. The current npm support matrix is macOS arm64/x64, Linux x64
(glibc), and Windows x64.

## Quick Start

Scan the current Python project for skills shipped by its dependencies:

```shell
uvx skilly scan
```

Download a skill from GitHub:

```shell
uvx skilly download https://github.com/example/project/tree/main/skills/code-review
```

Inspect installed skills:

```shell
uvx skilly list
```

## CLI Commands

| Command | Purpose |
| --- | --- |
| `uvx skilly scan` | Find skills provided by the project's Python dependencies. |
| `uvx skilly download <github-url>` | Install one or more skills from GitHub. |
| `uvx skilly list` | Browse, update, or remove installed skills. |
| `uvx skilly update` | Preview available updates; add `--yes` to apply all updates. |
| `uvx skilly remove <name>` | Remove an installed skill by directory name. |
| `uvx skilly skillsmp search <query>` | Search SkillsMP and install a selected result. |
| `uvx skilly create` | Create a valid skill through a terminal wizard or explicit options. |

Use `uvx skilly <command> --help` for all options.

### Create Skills

Interactive terminals open a full-screen editor for the required and optional
Agent Skills fields. `Ctrl+S` creates the skill, `Ctrl+X` cancels, and `Enter`
in the multi-line editors inserts real line breaks.

For scripts and automation, provide the required name and description:

```shell
uvx skilly create deployment-checks \
  --description "Validate deployment readiness before a production release." \
  --instructions "# Instructions

Run the deployment checklist and report blockers." \
  --metadata owner=platform \
  --with-scripts \
  --yes
```

Existing skills are rejected by default. Use `--overwrite` to replace one
atomically. See `uvx skilly create --help` for the complete contract.

### Install Dependency Skills

`uvx skilly scan` reads `pyproject.toml` and the project's `.venv`, then offers
skills shipped by direct, development, and optional dependencies:

```shell
uvx skilly scan
```

Exclude dependency categories when needed:

```shell
uvx skilly scan --no-dependency-groups --no-optional-dependencies
```

### Install GitHub Skills

```shell
uvx skilly download https://github.com/example/project
```

When a repository contains multiple skills, select one or install all:

```shell
uvx skilly download https://github.com/example/project --skill-name code-review
uvx skilly download https://github.com/example/project --all
```

### Destinations

Management commands accept the same destination options:

| Option | Destination |
| --- | --- |
| `--local` | Project-local skills directory. |
| `--global` | User-global skills directory. |
| `--claude` | Claude skills directory. |
| `--codex` | Codex skills directory. |
| `--copilot` | GitHub Copilot skills directory. |
| `--directory <path>` | Explicit directory; overrides all other destination options. |

Without destination options, `skilly` uses `.agents/skills`.

Set `SKILLY_DIRECTORY` to change that default without passing `--directory`
every time:

```shell
export SKILLY_DIRECTORY="$HOME/.config/skilly/skills"
skilly list
```

`--directory`, `--local`, `--global`, `--claude`, `--codex`, and `--copilot`
override `SKILLY_DIRECTORY` when provided explicitly.

```shell
uvx skilly download https://github.com/example/project --global --codex
uvx skilly list --local --claude
```

#### Destination Mapping

`skilly` resolves destination paths before use. Relative paths become absolute
from the current working directory, and `--directory "~/.copilot"` expands `~`
to your home directory.

| Flags                | Resolved destination                                                      |
|----------------------|---------------------------------------------------------------------------|
| _none_               | `SKILLY_DIRECTORY` when set, otherwise `.agents/skills`                   |
| `--local`            | `.agents/skills`                                                          |
| `--global`           | `~/.agents/skills`                                                        |
| `--claude`           | `.claude/skills`                                                          |
| `--claude --global`  | `~/.claude/skills`                                                        |
| `--codex`            | `.codex/skills`                                                           |
| `--codex --global`   | `~/.codex/skills`                                                         |
| `--copilot`          | `.github/skills`                                                          |
| `--copilot --global` | `~/.copilot/skills`                                                       |
| `--directory <path>` | Exactly that directory, after `~` expansion and absolute-path resolution. |

`--directory` overrides `--local`, `--global`, `--claude`, `--codex`, and
`--copilot`.

### GitHub Authentication

Authenticated requests have higher GitHub API rate limits. Use the first
available token:

```shell
export SKILLY_GITHUB_TOKEN=ghp_your_token
# or GITHUB_TOKEN / GH_TOKEN
```

GitHub-fetching commands also accept `--github-token`.

## Python API

`SkillRepository` is the stateful management interface. Bind a destination,
project settings, or custom filesystem once and reuse it:

```python
from pathlib import Path

from skilly import ProjectSettings, Skill, SkillRepository

repository = SkillRepository(
    directory=Path(".agents/skills"),
    project=ProjectSettings(include_dependency_groups=True),
)

created = repository.install(
    Skill(
        name="code-review",
        description="Review code for correctness and maintainability.",
        content="# Instructions\n\nReview the proposed change.",
    )
)

for match in repository.scan_project():
    print(match.available.name, match.status)
```

Use focused discovery functions when no repository state is needed:

```python
from skilly import discover_installed_skills, discover_venv_skills

installed = discover_installed_skills()
dependency_skills = discover_venv_skills()
```

The SkillsMP client returns typed results directly. Async methods expose the
same result types without blocking the event loop:

```python
from skilly.skillsmp import ClientSettings, SkillsMp, SkillsMpSearchQuery

client = SkillsMp(settings=ClientSettings(base_url="https://skillsmp.com/api/v1"))
result = client.search(SkillsMpSearchQuery(text="python", limit=5))
print(result.data.skills[0].github_url)
```

## Development

Install development dependencies and the editable extension:

```shell
just install
```

Run the required quality gates:

```shell
just lint
just test
just typecheck
```

The TypeScript launcher package also has dedicated helpers:

```shell
just ts::build
just ts::test
just ts::typecheck
just ts::smoke
just ts::publish-dry-run
```

## License

[MIT](LICENSE)
