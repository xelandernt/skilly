[![npm](https://img.shields.io/npm/v/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![pypi](https://img.shields.io/pypi/v/skilly)](https://pypi.org/project/skilly/)
[![pyrefly](https://img.shields.io/endpoint?url=https://pyrefly.org/badge.json)](https://github.com/facebook/pyrefly)
[![GitHub stars](https://img.shields.io/github/stars/xelandernt/skilly)](https://github.com/xelandernt/skilly/stargazers)
[![npm Downloads](https://img.shields.io/npm/dm/%40xelandernt%2Fskilly)](https://www.npmjs.com/package/@xelandernt/skilly)
[![PyPI Downloads](https://static.pepy.tech/badge/skilly/month)](https://pepy.tech/projects/skilly)
[![License](https://img.shields.io/github/license/xelandernt/skilly)](https://github.com/xelandernt/skilly/blob/main/LICENSE)

# skilly

Manage [Agent Skills](https://agentskills.io/specification) from the command
line or Python. Creates specification-compliant skills, installs from GitHub or
dependencies, and keeps them up to date.

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

Full [CLI reference](https://xelandernt.github.io/skilly/cli/) with command
descriptions, options, and workflow guides.

### Create Skills

See [creating skills](https://xelandernt.github.io/skilly/cli/creating-skills/).

### Install Dependency Skills

See [dependency scanning](https://xelandernt.github.io/skilly/cli/dependency-scanning/).

### Install GitHub Skills

See [installing and managing](https://xelandernt.github.io/skilly/cli/installing-and-managing/).

### Destinations

See [destinations reference](https://xelandernt.github.io/skilly/cli/destinations-and-configuration/).

### Configure Destinations

See [configuration docs](https://xelandernt.github.io/skilly/cli/destinations-and-configuration/).

### GitHub Authentication

Set a token for higher API rate limits (first available wins:
`SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`):

```shell
export SKILLY_GITHUB_TOKEN=ghp_your_token
```

All GitHub-fetching commands also accept `--github-token`.

## Python API

Full [Python API reference](https://xelandernt.github.io/skilly/python/) with
`SkillRepository`, discovery functions, source types, SkillsMP client, and
custom filesystem protocol.

### Maven support

See [dependency scanning](https://xelandernt.github.io/skilly/cli/dependency-scanning/)
for Maven integration details.

## Development

```shell
just install
just lint
just test
just typecheck
```

## License

[MIT](LICENSE)
