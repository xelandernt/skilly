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

`skilly configure` lets you choose which directories skilly manages and which
one opens by default.

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
```

### Interactive behavior

In a TTY without flags, `skilly configure` opens a two-tab TUI (Global / Local)
showing all known agent directories as toggleable checkboxes:

-   **Space** — toggles a directory on or off, or removes a custom one.
-   **Enter** — sets the highlighted directory as the default (marked with ★).
-   **Ctrl+S** — saves; you must have a default directory selected.

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

## GitHub authentication

Set a token for higher API rate limits (first available wins:
`SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`):

```shell
export SKILLY_GITHUB_TOKEN=ghp_your_token
```

All GitHub-fetching commands also accept `--github-token <TOKEN>`.
