# Dependency scanning

`skilly scan` discovers skills shipped by your project's dependencies across
three ecosystems.

## Usage

```shell
skilly scan [OPTIONS]
```

All three ecosystems are scanned by default when the corresponding manifest and
package directories exist. Results show source (e.g. `python:project`,
`node:dependencies`) and status (`installable`, `installed`, `updatable`).

## Options

| Option | Description |
|---|---|
| `--no-project-dependencies` | Ignore `[project].dependencies` while scanning. |
| `--group <NAME>` | Include only the named `[dependency-groups]` entry (repeatable). |
| `--exclude-group <NAME>` | Exclude the named `[dependency-groups]` entry. |
| `--extra <NAME>` | Include only the named `[project.optional-dependencies]` extra. |
| `--exclude-extra <NAME>` | Exclude the named `[project.optional-dependencies]` extra. |
| *(destination flags)* | See [destinations reference](destinations-and-configuration.md). |

## Python

Reads `pyproject.toml` and scans the project's `.venv` for packages that bundle
skills in a `skills/` directory. Supports multiple dependency groups and
optional dependency extras.

```shell
# Scan all Python dependencies (default)
skilly scan

# Include specific groups and extras
skilly scan --group dev --group test --exclude-extra docs

# Skip project-level dependencies
skilly scan --no-project-dependencies
```

## Node

Reads `package.json` (`dependencies`, `devDependencies`,
`optionalDependencies`) and scans `node_modules/` for packages that bundle
skills in a `skills/` directory.

```shell
skilly scan
```

Node scanning is automatic when `package.json` and `node_modules/` exist.

## Maven

Reads direct `<dependencies>` from `pom.xml` and scans JAR artifacts in the
local Maven repository (`~/.m2/repository` by default) for bundled skills.

### How it works

-   Reads only direct dependencies — profiles, plugins, and
    `<dependencyManagement>` are ignored.
-   Resolves `${property}` references defined in the same file's
    `<properties>` block.
-   Loads skills from recognized archive layouts:
    `.agents/skills/<name>/SKILL.md` and `skills/<name>/SKILL.md`.
-   Preserves binary resources inside JARs.
-   Rejects coordinates with path traversal components.

### Scopes

Controlled via `include_*_scope` flags (default: compile, runtime, test;
provided and system are excluded).

### Limitations

-   Only the local Maven repository is used; no remote artifact resolution.
-   No POM inheritance or effective-model merging.
-   No Gradle build file support.
-   Build execution and dependency graph traversal are not performed.

## Output

Each discovered skill shows:

-   **Source** — e.g. `python:dev`, `node:dependencies`, `maven:compile`.
-   **Status** — `installable`, `installed`, or `updatable`.
-   **Name and description** — from the skill's `SKILL.md`.
