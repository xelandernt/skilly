# skilly

[![Supported versions](https://img.shields.io/pypi/pyversions/that-depends.svg)](https://pypi.python.org/pypi/brave-api-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![pyrefly](https://img.shields.io/endpoint?url=https://pyrefly.org/badge.json)](https://github.com/facebook/pyrefly)

Skill's management.


## Installation

```shell
pip install skilly
```

## Commands

### cli
```shell
uvx skilly --help
```

### GitHub-backed downloads

For higher GitHub rate limits, authenticate requests with either:

```shell
export SKILLY_GITHUB_TOKEN=ghp_your_token
```

or the standard GitHub environment variables:

```shell
export GITHUB_TOKEN=ghp_your_token
# or
export GH_TOKEN=ghp_your_token
```

You can also pass `--github-token` to GitHub-fetching commands such as `skilly download`, `skilly skillsmp search`, `skilly skillsmp list`, and `skilly list`.

When downloading from a repository URL that contains multiple skills, use `--skill-name <name>` to select one skill or `--all` to install every skill in that repository.

Commands that manage installed skills accept destination flags. `--local` uses a project directory and `--global` uses the corresponding directory under your home directory. Select an agent-specific directory with `--claude`, `--codex`, or `--copilot`; without one of these flags, `.agents/skills` is used. An explicit `--directory` takes precedence over all destination flags.

```shell
skilly download https://github.com/example/project --global --codex
skilly list --local --claude
```


### python

```python
from pathlib import Path

from skilly import (
    FileSystem,
    ProjectSettings,
    Skill,
    SkillRepository,
    discover_venv_skills,
    resolve_skills_directory,
)
from skilly.skillsmp import ClientSettings, SkillsMp, SkillsMpSearchQuery

generated = Skill(
    name="my-skill",
    description="Use when generating a skill in memory.",
    content="## Instructions\nDo the thing.\n",
)
saved = generated.install_to(Path("/tmp/skills"), skill_name="custom-folder-name")
codex_global = resolve_skills_directory("codex", global_=True)

installed = Skill.from_dir(Path(".agents/skills/my-skill"))
github_skill = Skill.from_github(
    fetcher,
    "https://github.com/example/project/tree/main/.agents/skills/my-skill",
)

repository = SkillRepository()
for match in repository.scan_project(project=ProjectSettings(include_dev=True)):
    print(match.available.name, match.status)

class CustomFileSystem(FileSystem):
    ...

skills = repository.list(file_system=CustomFileSystem())
dependency_skills = discover_venv_skills(file_system=CustomFileSystem())

client = SkillsMp(settings=ClientSettings(base_url="https://skillsmp.com/api/v1"))
response = client.search(SkillsMpSearchQuery(text="python", limit=5))
print(response.parsed_data.data.skills[0].name)
```

## Development

To see a list of useful commands run:
```shell
just
```

For TUI styling notes, see [docs/tui-styling.md](docs/tui-styling.md).


## License

[MIT](LICENSE)
