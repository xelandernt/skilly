[![npm](https://img.shields.io/npm/v/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![pypi](https://img.shields.io/pypi/v/skilly)](https://pypi.org/project/skilly/)
[![pyrefly](https://img.shields.io/endpoint?url=https://pyrefly.org/badge.json)](https://github.com/facebook/pyrefly)
[![GitHub stars](https://img.shields.io/github/stars/xelandernt/skilly)](https://github.com/xelandernt/skilly/stargazers)
[![npm Downloads](https://img.shields.io/npm/dm/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![PyPI Downloads](https://static.pepy.tech/badge/skilly/month)](https://pepy.tech/projects/skilly)
[![License](https://img.shields.io/github/license/xelandernt/skilly)](https://github.com/xelandernt/skilly/blob/main/LICENSE)

# skilly

Manage [Agent Skills](https://agentskills.io/specification) from the command
line or Python. Creates specification-compliant skills, installs from GitHub,
Bitbucket Cloud, Bitbucket Data Center, or dependencies, and keeps them up to
date.

## Installation

```shell
uvx skilly --help               # Python (uvx/pip)
npx @xelandernt/skilly --help   # Node (npx)
brew install xelandernt/skilly/skilly  # Homebrew
```

### Python

```shell
uvx skilly --help
```

Ships CLI + Python import surface. Pre-built wheels for Linux x64, macOS
arm64/x64, Windows x64.

### Node

```shell
npx @xelandernt/skilly --help
```

Ships native Rust CLI (macOS arm64/x64, Linux x64 glibc, Windows x64). No
Python import surface.

### Homebrew

```shell
brew tap xelandernt/skilly https://github.com/xelandernt/skilly
brew install xelandernt/skilly/skilly
```

Ships native Rust CLI (macOS arm64/x64, Linux x64).

### Info

See the [installation guide](https://xelandernt.github.io/skilly/getting-started/installation/)
for a full capability comparison.

## Quick Start

```shell
skilly create deployment-checks \
  --description "Validate deployment readiness." \
  --instructions "# Instructions\n\nRun the deployment checklist." \
  --yes
skilly list
```

## CLI Commands

| Command                   | Purpose                                                            |
|---------------------------|--------------------------------------------------------------------|
| `scan`                    | Find skills provided by Python and Node project dependencies       |
| `download <repository-url>` | Install one or more skills from a supported repository           |
| `list`                    | Browse, update, or remove installed skills                         |
| `update`                  | Preview available updates; `--yes` applies all                     |
| `remove <name>`           | Remove an installed skill by directory name                        |
| `skillsmp search <query>` | Search SkillsMP and install a selected result                      |
| `create`                  | Create a valid skill directory                                     |
| `configure`               | Manage directories and repository credentials                      |

Run `skilly <command> --help` for all options.
Run `skilly --version` to print the installed package version.

### Create Skills

See [creating skills](https://xelandernt.github.io/skilly/cli/creating-skills/).

### Install Dependency Skills

See [dependency scanning](https://xelandernt.github.io/skilly/cli/dependency-scanning/).

### Install Repository Skills

See [installing and managing](https://xelandernt.github.io/skilly/cli/installing-and-managing/).

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

| Flags                | Resolved destination                                          |
|----------------------|---------------------------------------------------------------|
| _none_               | `SKILLY_DEFAULT_DIRECTORY` if set, otherwise `.agents/skills` |
| `--local`            | `.agents/skills`                                              |
| `--global`           | `~/.agents/skills`                                            |
| `--claude`           | `.claude/skills`                                              |
| `--claude --global`  | `~/.claude/skills`                                            |
| `--codex`            | `.codex/skills`                                               |
| `--codex --global`   | `~/.codex/skills`                                             |
| `--copilot`          | `.github/skills`                                              |
| `--copilot --global` | `~/.copilot/skills`                                           |
| `--directory <path>` | That directory (after `~` expansion)                          |

Set a default destination:

```shell
export SKILLY_DEFAULT_DIRECTORY="$HOME/.config/skilly/skills"
```

`--directory` overrides all other destination options and `SKILLY_DEFAULT_DIRECTORY`.

### Configuration

`skilly configure` manages directories and saved repository credentials.

```shell
uvx skilly configure
uvx skilly configure --show          # Print current config as TOML
uvx skilly configure --reset         # Restore defaults
```

Add or remove custom directories via CLI:

```shell
uvx skilly configure --add-global /opt/skills
uvx skilly configure --add-local .project/skills
uvx skilly configure --remove-global /opt/skills
uvx skilly configure --remove-local .project/skills
```

Configuration is stored in `~/.skilly.toml`:

```toml
default_directory = ".agents/skills"

[global]
directories = ["~/.agents/skills", "/opt/skills"]

[local]
directories = [".agents/skills", ".project/skills"]
```

The default directory opens first in interactive menus (`list`, `scan`, etc.).

### Repository Authentication

For `download`, `list`, and `update`, Skilly resolves credentials in this order:

1. `--token <TOKEN>` for a one-off command.
2. A saved provider credential with the exact provider and base URL.
3. A provider environment variable.

| Provider | Environment fallback | Saved base URL example |
|---|---|---|
| GitHub | `SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN` | `https://github.com` |
| Bitbucket Cloud | `SKILLY_BITBUCKET_CLOUD_TOKEN` | `https://bitbucket.org` |
| Bitbucket Data Center | `SKILLY_BITBUCKET_DATA_CENTER_TOKEN` | `https://git.example.com/bitbucket` |

Save a reusable credential with `skilly configure`, or use the
non-interactive command:

```shell
skilly configure --add-provider bitbucket-data-center \
  --provider-url https://git.example.com/bitbucket \
  --provider-token "$BITBUCKET_TOKEN"
```

Saved tokens are redacted from `skilly configure --show`, never written to
installed skill metadata, and stored in `~/.skilly.toml`; on Unix that file is
owner-only. Remove a saved credential with:

```shell
skilly configure --remove-provider bitbucket-data-center \
  --provider-url https://git.example.com/bitbucket
```

Use `--provider bitbucket-data-center` for Bitbucket Data Center repositories:

```shell
uvx skilly download https://git.example.com/bitbucket/projects/ENG/repos/skills \
  --provider bitbucket-data-center --token "$SKILLY_BITBUCKET_DATA_CENTER_TOKEN"
```

## Python API

Full [Python API reference](https://xelandernt.github.io/skilly/python/) with
`SkillRepository`, discovery functions, source types, SkillsMP client, and
custom filesystem protocol.

```python
from pathlib import Path
from skilly import (
    ProjectSettings,
    PythonSource,
    Skill,
    SkillRepository,
)

repository = SkillRepository(
    directory=Path(".agents/skills"),
    project=ProjectSettings(
        sources=(
            PythonSource(
                dependency_groups=("dev",),
                optional_dependencies=("docs",),
            ),
        ),
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
from skilly import (
    NodeSource,
    PythonSource,
    discover_installed_skills,
    discover_package_source_skills,
)

installed = discover_installed_skills()
python_skills = discover_package_source_skills(PythonSource())
node_skills = discover_package_source_skills(NodeSource())
```

`ProjectSettings` accepts `PythonSource`, `NodeSource`, and `MavenSource`:

```python
from pathlib import Path

from skilly import (
    MavenSource,
    NodeSource,
    ProjectSettings,
    PythonSource,
    SkillRepository,
)

repository = SkillRepository(
    directory=Path(".agents/skills"),
    project=ProjectSettings(
        sources=(
            NodeSource(
                include_dependencies=True,
                include_dev_dependencies=False,
            ),
        ),
    ),
)
```

Use `ProjectSettings(sources=())` to disable scanning, or pass
individual `PackageSource` entries to scan only specific ecosystems.
`SkillRepository()` defaults to Python, Node, and Maven sources.

### Maven support

Maven skills are discovered from JAR artifacts in the local Maven
repository (`~/.m2/repository` by default). The scanner:

- Reads only direct `<dependencies>` from `pom.xml` — profiles,
  plugins, and `<dependencyManagement>` are ignored.
- Resolves `${property}` references defined in the same file's
  `<properties>` block.
- Loads skills from recognized archive layouts:
  `.agents/skills/<name>/SKILL.md` and `skills/<name>/SKILL.md`.
- Preserves binary resources inside JARs.
- Rejects coordinates with path traversal components.

**Known limitations:**
- Only the local repository is used; no remote artifact resolution.
- No POM inheritance or effective-model merging.
- No Gradle build file support.
- Build execution and dependency graph traversal are not performed.
- MavenSource includes compile, runtime, and test scopes by default; provided
  and system scopes are excluded.

SkillsMP client with typed results:

```python
from skilly.skillsmp import ClientSettings, SkillsMp, SkillsMpSearchQuery

client = SkillsMp(settings=ClientSettings(base_url="https://skillsmp.com/api/v1"))
result = client.search(SkillsMpSearchQuery(text="python", limit=5))
print(result.data.skills[0].github_url)
```

## Development

```shell
just install
just lint
just test
just typecheck
```

## License

[MIT](LICENSE)
