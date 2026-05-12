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

GitHub installs now use repository tarballs instead of recursively walking the contents API, which keeps GitHub API request counts low.

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

You can also pass `--github-token` to GitHub-fetching commands such as `skilly skillsmp download`, `skilly skillsmp search`, `skilly skillsmp list`, and `skilly list`.

When downloading from a repository URL that contains multiple skills, use `--skill-name <name>` to select one skill or `--all` to install every skill in that repository.


### python

```python
from pathlib import Path

from skilly import Skill, SkillRepository

skill = Skill.from_dir(Path(".agents/skills/my-skill"))
github_skill = Skill.from_github(
    fetcher,
    "https://github.com/example/project/tree/main/.agents/skills/my-skill",
)

repository = SkillRepository()
for match in repository.scan_project():
    print(match.available.name, match.status)
```

## Development

To see a list of useful commands run:
```shell
just install-dev
```


## License

[MIT](LICENSE)
