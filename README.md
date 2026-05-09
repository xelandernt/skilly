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
