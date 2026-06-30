# Installing and managing skills

## Download skills from GitHub

```shell
skilly download https://github.com/example/project
```

When a repository contains multiple skills, select one or download all:

```shell
skilly download https://github.com/example/project --all
skilly download https://github.com/example/project --skill-name code-review
```

### Options

| Option | Description |
|---|---|
| `--skill-name <NAME>` | Select a specific skill from a multi-skill repository. |
| `--all` | Download every skill found at the URL. |
| `--overwrite` | Overwrite existing files during installation. |
| `--github-token <TOKEN>` | GitHub token for API requests (also reads `SKILLY_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`). |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

## Search SkillsMP

```shell
skilly skillsmp search python
skilly skillsmp search "code review"
```

Select a result from the interactive prompt, or use `--overwrite` to replace an
already-installed skill.

### Browse installed SkillsMP skills

```shell
skilly skillsmp list
```

Shows all SkillsMP-installed skills with their update status.

## List installed skills

```shell
skilly list
```

Shows every installed skill, its origin (GitHub, SkillsMP, local), and its
status. In a TTY, this opens a browseable menu where you can select a skill to
update or remove.

## Update skills

```shell
skilly update
```

Checks every installed GitHub- and SkillsMP-backed skill for newer versions.

| Option | Description |
|---|---|
| `-y`, `--yes` | Apply every discovered update without prompting. |
| `--github-token <TOKEN>` | GitHub token for API requests. |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

To update or remove a single skill interactively, use `skilly list`.

## Remove a skill

```shell
skilly remove <name>
```

Removes the skill directory matching the given name. Use `skilly list` to find
the exact directory name.

## Dependency scanning discovery

Skills can also be installed via `scan` — this is covered in detail on the
[dependency scanning](dependency-scanning.md) page.
