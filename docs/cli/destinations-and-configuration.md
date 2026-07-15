# Destinations and configuration

## Destination resolution

Every management command accepts the same destination flags. When no flag is
given, skilly checks `SKILLY_DEFAULT_DIRECTORY`, then falls back to
`.agents/skills`.

| Flag(s) | Resolved path | Scope |
|---|---|---|
| *(none)* | `$SKILLY_DEFAULT_DIRECTORY` or `.agents/skills` | — |
| `--directory <PATH>` | That path (after `~` expansion) | — |
| `--local` | `.agents/skills` | Project |
| `--global` | `~/.agents/skills` | User |
| `--claude` | `.claude/skills` | Project |
| `--claude --global` | `~/.claude/skills` | User |
| `--codex` | `.codex/skills` | Project |
| `--codex --global` | `~/.codex/skills` | User |
| `--copilot` | `.github/skills` | Project |
| `--copilot --global` | `~/.copilot/skills` | User |

`--directory` overrides all other destination flags and
`SKILLY_DEFAULT_DIRECTORY`.

Set a default destination:

```shell
export SKILLY_DEFAULT_DIRECTORY="$HOME/.config/skilly/skills"
```

## Configure command

`skilly configure` manages the directories Skilly uses and reusable repository
credentials.

### Non-interactive usage

```shell
# Print current configuration as TOML
skilly configure --show

# Restore defaults (agents global and local directories only)
skilly configure --reset

# Add custom directories
skilly configure --add-global /opt/skills
skilly configure --add-local .project/skills

# Remove custom directories
skilly configure --remove-global /opt/skills
skilly configure --remove-local .project/skills

# Store a repository credential
skilly configure --add-provider bitbucket-data-center \
  --provider-url https://git.example.com/bitbucket \
  --provider-token "$BITBUCKET_TOKEN"

# Remove a repository credential
skilly configure --remove-provider bitbucket-data-center \
  --provider-url https://git.example.com/bitbucket
```

### Configuration file

Settings are stored in `~/.skilly.toml`:

```toml
default_directory = ".agents/skills"

[global]
directories = ["~/.agents/skills", "/opt/skills"]

[local]
directories = [".agents/skills", ".project/skills"]
```

The default directory opens first in interactive menus (`list`, `scan`, etc.).

## Repository authentication

`download`, `list`, and `update` resolve credentials in this order:

1. An explicit `--token <TOKEN>`.
2. A saved credential whose provider and base URL exactly match the repository.
3. A provider environment variable.

| Provider | Environment variables |
|---|---|
| GitHub | `SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN` |
| Bitbucket Cloud | `SKILLY_BITBUCKET_CLOUD_TOKEN` |
| Bitbucket Data Center | `SKILLY_BITBUCKET_DATA_CENTER_TOKEN` |

Provider credentials contain a provider type, base URL, and repository-read
token. Saving a provider in `skilly configure` persists it immediately. Tokens
are redacted from `skilly configure --show`.

Tokens are never written to installed skill metadata. On Unix, the
configuration file is owner-only.
