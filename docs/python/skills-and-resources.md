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
| `repository_provider` | `str \| None` | Repository provider for repository-backed installs. |
| `repository_url` | `str \| None` | Canonical repository skill URL used for updates. |
| `repository_commit_sha` | `str \| None` | Immutable repository revision used for installation. |

### Repository discovery

```python
from skilly import discover_repository_skills, parse_repository_location

location = parse_repository_location("https://bitbucket.org/example/skills")
skills = discover_repository_skills("https://bitbucket.org/example/skills")

data_center = parse_repository_location(
    "https://git.example.com/bitbucket/projects/ENG/repos/skills",
    provider="bitbucket-data-center",
)
```

Supported providers are `"github"`, `"bitbucket-cloud"`, and
`"bitbucket-data-center"`. GitHub and Bitbucket Cloud are detected from their
public URLs. Pass `provider="bitbucket-data-center"` for Bitbucket Data Center.
Pass `token=` for a one-off credential or use a provider environment variable.

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

# Check whether a skill can be refreshed
skill.is_dependency()
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
from skilly import SkillResource

resource = SkillResource(
    relative_path="scripts/deploy.sh",
    kind="script",
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

Repository discovery records `repository_provider`, `repository_url`, and
`repository_commit_sha` automatically. Use `discover_repository_skills()` for
all remote skills, then install the selected result with `SkillRepository`.
