# Skills and resources

## Skill

`Skill` is a Rust-backed domain object representing an agent skill conforming
to the [Agent Skills specification](https://agentskills.io/specification).

```python
from skilly import Skill

skill = Skill(
    name="code-review",
    description="Review code for correctness and maintainability.",
    content="# Instructions\n\nReview the proposed change.",
)
```

### Constructor parameters

| Parameter | Type | Description |
|---|---|---|
| `name` | `str` | Skill name (used as directory name). |
| `description` | `str` | What the skill does and when to use it. |
| `content` | `str` | Markdown body for `SKILL.md`. |
| `path` | `StrPath \| None` | Filesystem path if loaded from disk. |
| `license` | `str \| None` | License name or reference. |
| `compatibility` | `str \| None` | Environment requirements. |
| `metadata` | `dict[str, str] \| None` | Additional frontmatter metadata. |
| `allowed_tools` | `str \| None` | Space-separated pre-approved tools. |
| `resources` | `list[SkillResourceLike] \| None` | Bundled resource files. |

### Key properties

| Property | Type | Description |
|---|---|---|
| `name` | `str` | Skill name. |
| `description` | `str` | Description text. |
| `content` | `str` | Raw `SKILL.md` content. |
| `path` | `Path \| None` | Filesystem path if loaded from disk. |
| `license` | `str \| None` | License identifier. |
| `compatibility` | `str \| None` | Environment requirements. |
| `metadata` | `dict[str, str]` | Frontmatter metadata. |
| `allowed_tools` | `str \| None` | Allowed tools string. |
| `resources` | `list[SkillResource]` | Bundled resources. |
| `directory_name` | `str` | Normalized directory name for the skill. |
| `source` | `str` | Origin source identifier. |

### Class methods

```python
# Load from a SKILL.md file
skill = Skill.from_file("path/to/SKILL.md")

# Load from a directory containing SKILL.md
skill = Skill.from_dir("path/to/skill/dir")

# Parse from raw markdown text
skill = Skill.from_text(text)
```

### Instance methods

```python
# Check if the skill is installed
skill.is_installed()

# Check the skill's origin
skill.is_dependency()
skill.is_skillsmp()
skill.is_github()

# Check if a newer version is available
skill.can_update()

# Compare with another skill
skill.matches(other)

# Get a specific resource by path
resource = skill.get_resource("scripts/deploy.sh")

# Install to a directory
skill.install_to(directory=".agents/skills", skill_name="my-skill")
```

## SkillResource

```python
from skilly import ResourceKind, SkillResource

resource = SkillResource(
    relative_path="scripts/deploy.sh",
    kind=ResourceKind("script"),
    content=b"#!/bin/sh\necho Deploying...",
)
```

| Field | Type | Description |
|---|---|---|
| `relative_path` | `PurePosixPath` | Path relative to the skill directory. |
| `kind` | `ResourceKind` | One of `"script"`, `"reference"`, `"asset"`, `"other"`. |
| `content` | `bytes` | Raw file content. |

## ResourceKind

Type alias for `Literal["script", "reference", "asset", "other"]`.

## SkillOrigin

```python
from skilly import SkillOrigin

origin = SkillOrigin(
    source="github",
    github_url="https://github.com/example/project",
    github_commit_sha="abc123",
)
```

| Field | Type | Description |
|---|---|---|
| `source` | `str \| None` | Origin identifier (e.g. `"github"`, `"skillsmp"`). |
| `package_name` | `str \| None` | Name of the source package. |
| `package_version` | `str \| None` | Version of the source package. |
| `package_ecosystem` | `str \| None` | Ecosystem (`"python"`, `"node"`, `"maven"`). |
| `github_url` | `str \| None` | GitHub URL for GitHub-backed skills. |
| `github_commit_sha` | `str \| None` | Commit SHA for GitHub-backed skills. |
| `skillsmp_id` | `str \| None` | SkillsMP registry ID. |
