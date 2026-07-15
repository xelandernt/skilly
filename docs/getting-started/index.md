# Getting started

This guide walks through the end-to-end flow without relying on a TTY or
interactive prompts.

## Prerequisites

-   Python 3.10+ and `uv` (or `pip`)
-   A project directory with a `pyproject.toml` (for dependency scanning)

## 1. Install skilly

```shell
uvx skilly --version
```

## 2. Create a skill

```shell
skilly create deployment-checks \
  --description "Validate deployment readiness before a production release." \
  --instructions "# Instructions\n\nRun the deployment checklist." \
  --yes
```

This creates `deployment-checks/SKILL.md` in the default destination. Use
`--directory` to choose a different location.

## 3. See what's installed

```shell
skilly list
```

If you created the skill with a destination flag (e.g. `--local`), use the same
flag when listing.

## 4. Install skills from a dependency scan

```shell
skilly scan
```

## 5. Install from a repository

```shell
skilly download https://github.com/example/skills-repo --all
```

GitHub and Bitbucket Cloud are detected from their URLs. Use
`--provider bitbucket-data-center` for Bitbucket Data Center.

## 6. Check for updates

```shell
skilly update
```

This checks every installed repository- and SkillsMP-backed skill for newer
versions. Pass `--yes` to apply all discovered updates without prompting.

## Next steps

-   [Installation options](installation.md) — choose your package manager.
-   [Core concepts](core-concepts.md) — skills, resources, origins, statuses.
-   [CLI reference](../cli/index.md) — full command documentation.
-   [Python API](../python/index.md) — programmatic usage.
