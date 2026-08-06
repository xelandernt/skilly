# Core concepts

## Skill

A directory containing at least a `SKILL.md` file conforming to the [Agent
Skills specification](https://agentskills.io/specification). A skill defines
instructions, metadata, and optionally scripts, references, and assets.

## Resources

A skill may bundle supporting files alongside `SKILL.md`:

-   **Scripts** — executable scripts in a `scripts/` directory.
-   **References** — reference documents in a `references/` directory.
-   **Assets** — binary or media files in an `assets/` directory.

Resources are validated during installation: they must not escape the skill
directory (no path traversal).

## Managed metadata

When skilly installs a skill, it writes a `.skilly.toml` file inside the skill
directory. This file records the origin, source URL, version, and installation
timestamp so skilly can detect updates and track provenance.

## Origins

skilly tracks where each installed skill came from:

-   **Repository** — downloaded from GitHub, Bitbucket Cloud, or Bitbucket
    Data Center.
-   **SkillsMP** — installed via the SkillsMP registry.
-   **Dependency** — discovered via `scan` from a Python, Node, or Maven
    dependency.
-   **Local** — created locally with `skilly create`.

## Statuses

When scanning or listing skills, each skill has one of these statuses:

-   **installable** — discovered but not yet installed.
-   **installed** — present and up to date.
-   **updatable** — installed but a newer version is available.

Interactive update checks animate pending rows with dots and use transient
**up to date** and **check failed** labels for successful unchanged and
unsuccessful remote checks. These indicators do not change the installed
skill's persisted status.

## Destinations

skilly can install skills into several agent-specific directories:

| Flag | Path | Scope |
|---|---|---|
| `--local` | `.agents/skills` | Project |
| `--global` | `~/.agents/skills` | User |
| `--claude` | `.claude/skills` | Project |
| `--claude --global` | `~/.claude/skills` | User |
| `--codex` | `.codex/skills` | Project |
| `--codex --global` | `~/.codex/skills` | User |
| `--copilot` | `.github/skills` | Project |
| `--copilot --global` | `~/.copilot/skills` | User |

When no flag is given, skilly uses the `SKILLY_DEFAULT_DIRECTORY` environment
variable or falls back to `.agents/skills`.
