# Creating skills

`skilly create` creates a specification-compliant skill directory with a
`SKILL.md` file.

## Usage

```shell
skilly create [OPTIONS] [NAME]
```

The `NAME` argument is required in non-interactive runs.

## Options

| Option | Description |
|---|---|
| `--description <TEXT>` | Describe what the skill does and when to use it. |
| `--instructions <MARKDOWN>` | Markdown body for the `SKILL.md`. |
| `--license <NAME>` | License name or bundled reference. |
| `--compatibility <REQS>` | Environment requirements for the skill. |
| `--metadata <KEY=VALUE>` | Add frontmatter metadata (repeatable). |
| `--allowed-tools <TOOLS>` | Space-separated pre-approved tools. |
| `--with-scripts` | Create an empty `scripts/` directory. |
| `--with-references` | Create an empty `references/` directory. |
| `--with-assets` | Create an empty `assets/` directory. |
| `--overwrite` | Replace an existing skill atomically. |
| `-y`, `--yes` | Create without confirmation. |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

## Examples

### Minimal non-interactive creation

```shell
skilly create code-review \
  --description "Review code for correctness and maintainability." \
  --instructions "# Instructions\n\nReview the proposed change." \
  --yes
```

### With metadata and scripts

```shell
skilly create deployment-checks \
  --description "Validate deployment readiness." \
  --instructions "# Instructions\n\nRun the deployment checklist." \
  --metadata owner=platform \
  --metadata severity=high \
  --with-scripts \
  --with-references \
  --yes
```

### Overwrite an existing skill

```shell
skilly create code-review --overwrite --yes
```
