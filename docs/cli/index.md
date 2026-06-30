# CLI

`skilly` provides a set of commands for managing Agent Skills. All commands
work in both interactive terminals and non-interactive automation. When a TTY
is detected, some commands enhance their output with a TUI; otherwise output is
suitable for non-interactive automation.

## Global options

| Option | Description |
|---|---|
| `-h`, `--help` | Print help for the command |
| `-V`, `--version` | Print the installed version |

## Command overview

| Command | Purpose |
|---|---|
| `create` | Create a specification-compliant skill. |
| `scan` | Scan dependencies for skills from `pyproject.toml`, `package.json`, and `pom.xml`. |
| `download` | Download one or more skills from a GitHub repository URL. |
| `list` | Browse, update, or remove installed skills. |
| `update` | Check installed skill updates in bulk and optionally apply them. |
| `remove` | Remove one installed skill by directory name. |
| `skillsmp search` | Search SkillsMP and install a selected result. |
| `skillsmp list` | Browse installed SkillsMP skills and manage updates. |
| `util dependencies` | Print dependency names resolved from `pyproject.toml`. |
| `util venv` | List skills discovered inside a virtual environment. |
| `configure` | Configure which directories skilly manages. |

## Destination options

Every management command (`scan`, `download`, `list`, `update`, `remove`,
`create`, `skillsmp search`, `skillsmp list`) accepts destination flags:

| Flag | Resolved path |
|---|---|
| *(none)* | `$SKILLY_DEFAULT_DIRECTORY` or `.agents/skills` |
| `--local` | `.agents/skills` |
| `--global` | `~/.agents/skills` |
| `--claude` | `.claude/skills` |
| `--claude --global` | `~/.claude/skills` |
| `--codex` | `.codex/skills` |
| `--codex --global` | `~/.codex/skills` |
| `--copilot` | `.github/skills` |
| `--copilot --global` | `~/.copilot/skills` |
| `--directory <path>` | Literal path (after `~` expansion) |

`--directory` overrides all other destination flags and
`SKILLY_DEFAULT_DIRECTORY`.

## Workflow guides

-   [Creating skills](creating-skills.md) — create skills from the CLI.
-   [Installing and managing skills](installing-and-managing.md) — download from
    GitHub, SkillsMP, dependency scanning, updates, and removal.
-   [Dependency scanning](dependency-scanning.md) — discover skills in your
    project's Python, Node, and Maven dependencies.
-   [Destinations and configuration](destinations-and-configuration.md) —
    configure directories and manage agent paths.
