# Skills and resources

## Skill

`Skill` is a Rust-backed domain object representing an agent skill conforming
to the [Agent Skills specification](https://agentskills.io/specification).

```python
from skilly import Skill

skill = Skill(
    name="code-review",
    description="Review code for correctness and maintainability.",
    body="# Instructions\n\nReview the proposed change.",
)
```

### Constructor parameters

| Parameter | Type | Description |
|---|---|---|
| `name` | `str` | Skill name (used as directory name). |
| `description` | `str` | What the skill does and when to use it. |
| `body` | `str` | Markdown body for a programmatically constructed `SKILL.md`. |
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
| `text` | `str` | Canonical reconstructed `SKILL.md` text. |
| `raw` | `bytes` | Exact source bytes for a discovered `SKILL.md`; canonical bytes for a constructed skill. |
| `is_text()` | `bool` | Whether `raw` is valid UTF-8. |
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

### File content

Use `raw` when exact source bytes matter (for example, for a hash or archive)
and `text` when you need a readable `SKILL.md` reconstruction. Every bundled
file is exposed through `resources`, `scripts`, `references`, or `assets` and
uses the same `raw`, `text`, and `is_text()` interface.

```python
skill.raw
skill.text

for resource in skill.resources:
    if resource.is_text():
        print(resource.text)
    else:
        save_bytes(resource.raw)
```

### Repository discovery

```python
from collections.abc import Mapping

from skilly import RepositoryDiscoveryClient, parse_repository_location


class ApplicationTransport:
    def get(
        self,
        url: str,
        *,
        headers: Mapping[str, str],
        params: Mapping[str, str],
    ) -> bytes:
        # Delegate to the application's vetted HTTP client. It must enforce
        # its DNS, redirect, timeout, response-size, concurrency, and status
        # policies before returning the complete body.
        return application_http_client.get_bytes(
            url, headers=headers, params=params
        )

location = parse_repository_location("https://bitbucket.org/example/skills")
discovery = RepositoryDiscoveryClient(ApplicationTransport())
skills = discovery.discover("https://bitbucket.org/example/skills")

data_center = parse_repository_location(
    "https://git.example.com/bitbucket/projects/ENG/repos/skills",
    provider="bitbucket-data-center",
)
```

Supported providers are `"github"`, `"bitbucket-cloud"`, and
`"bitbucket-data-center"`. GitHub and Bitbucket Cloud are detected from their
public URLs. Pass `provider="bitbucket-data-center"` for Bitbucket Data Center.

`RepositoryDiscoveryClient` has no built-in network fallback. Its transport
receives every request made during ref resolution, provider traversal,
pagination, and archive/file retrieval. The transport must reject unwanted
redirects and non-public resolved addresses, apply connect/read/total timeouts,
cap the complete response body before returning it, limit its own concurrency,
and raise for unsuccessful responses. It also owns authentication and proxy/TLS
policy. Skilly continues to validate provider payloads, repository paths, and
archive/resource limits.

### Class methods

```python
# Load from a SKILL.md file
skill = Skill.from_file("path/to/SKILL.md")

# Load from a directory containing SKILL.md
skill = Skill.from_dir("path/to/skill/dir")

# Parse from raw markdown text
skill = Skill.from_text(text)

# Load a complete bundle from exact in-memory bytes
skill = Skill.from_bundle(skill_markdown, resources)
```

### In-memory bundles

`Skill.from_bundle(skill_markdown, resources=())` loads and validates a complete
bundle without reading or writing a filesystem, executing a bundled script,
importing modules, invoking a shell, or making network requests. It preserves
the supplied `SKILL.md` bytes in `skill.raw` and each resource's bytes in
`resource.raw`.

```python
from pathlib import PurePosixPath

from skilly import Skill, SkillBundleError, SkillResource

try:
    skill = Skill.from_bundle(
        b"---\nname: on-call\ndescription: Runbook instructions.\n---\nRead first.\n",
        (
            SkillResource(
                relative_path=PurePosixPath("references/runbook.md"),
                kind="reference",
                raw=b"# Runbook\n",
            ),
        ),
    )
except SkillBundleError as error:
    print(error.code, error.path, error.field)
```

`SkillBundleError` is a `ValueError` with stable `code`, `path`, and optional
`field` attributes. Current codes are `invalid_utf8`, `invalid_frontmatter`,
`invalid_field`, `invalid_resource_path`, and `duplicate_resource_path`.
Resource `kind` remains caller-provided metadata; it is not inferred from or
checked against the path.

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
    raw=b"#!/bin/sh\necho Deploying...",
)
```

| Field | Type | Description |
|---|---|---|
| `relative_path` | `PurePosixPath` | Path relative to the skill directory. |
| `kind` | `ResourceKind` | One of `"script"`, `"reference"`, `"asset"`, `"other"`. |
| `raw` | `bytes` | Exact file bytes. |
| `text` | `str` | UTF-8 decoded content; raises `UnicodeDecodeError` for binary files. |
| `is_text()` | `bool` | Whether `text` can be read without a decoding error. |

## ResourceKind

Type alias for `Literal["script", "reference", "asset", "other"]`.

## Repository provenance

Repository discovery records `repository_provider`, `repository_url`, and
`repository_commit_sha` automatically. Use `RepositoryDiscoveryClient` for all
remote skills, then install the selected result with `SkillRepository`. Pass the
same client to `SkillRepository(discovery_client=...)` when repository-backed
updates must use that network boundary too.
