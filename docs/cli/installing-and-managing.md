# Installing and managing skills

## Download skills from a repository

```shell
skilly download https://github.com/example/project
```

GitHub and Bitbucket Cloud URLs are detected automatically. Use
`--provider bitbucket-data-center` for Bitbucket Data Center:

```shell
skilly download https://bitbucket.org/example/project
skilly download https://git.example.com/bitbucket/projects/ENG/repos/skills \
  --provider bitbucket-data-center
```

GitLab, generic Git URLs, and SSH URLs are not supported.

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
| `--provider <PROVIDER>` | Explicit provider: `github`, `bitbucket-cloud`, or `bitbucket-data-center`; required for Data Center. |
| `--token <TOKEN>` | One-off token for the selected/detected provider. Saved credentials and environment fallbacks are described in the [authentication reference](destinations-and-configuration.md#repository-authentication). |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

## Search SkillsMP

```shell
skilly skillsmp search python
skilly skillsmp search "code review"
```

Use `--overwrite` to replace an already-installed skill.

| Option | Description |
|---|---|
| `--overwrite` | Replace an installed skill with the selected result. |
| `--github-token <TOKEN>` | GitHub token for resolving SkillsMP skill contents. |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

### Browse installed SkillsMP skills

```shell
skilly skillsmp list
```

Shows all SkillsMP-installed skills with their update status.

`skilly skillsmp list` accepts `--github-token <TOKEN>` when checking updates.

## List installed skills

```shell
skilly list
```

Shows every installed skill, its origin, and its status. Repository-backed
skills display their provider (`github`, `bitbucket-cloud`, or
`bitbucket-data-center`) as the origin.

In an interactive terminal, select a skill and choose **View files** to inspect
its `SKILL.md` and bundled resources. Press `/` to filter by filename;
directories containing a matching file remain visible. Press `Esc` to clear
the filter and return to the full tree.

## Update skills

```shell
skilly update
```

Checks every installed repository- and SkillsMP-backed skill for newer versions.

| Option | Description |
|---|---|
| `-y`, `--yes` | Apply every discovered update without prompting. |
| `--token <TOKEN>` | One-off token for repository-backed skill update checks; saved credentials and environment fallbacks also apply. |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

## Remove a skill

```shell
skilly remove <name>
```

Removes the skill directory matching the given name. Use `skilly list` to find
the exact directory name.

## Dependency scanning discovery

Skills can also be installed via `scan` — this is covered in detail on the
[dependency scanning](dependency-scanning.md) page.
