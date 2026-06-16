# SkillsMP client

The SkillsMP client provides a typed interface for searching the SkillsMP
registry and resolving skill contents from GitHub.

```python
from skilly.skillsmp import ClientSettings, SkillsMp, SkillsMpSearchQuery

client = SkillsMp(
    settings=ClientSettings(
        base_url="https://skillsmp.com/api/v1",
    )
)

result = client.search(SkillsMpSearchQuery(text="python", limit=5))
for skill in result.data.skills:
    print(skill.name, skill.github_url)
```

## ClientSettings

| Field | Type | Description |
|---|---|---|
| `base_url` | `str \| None` | SkillsMP API base URL. |
| `api_key` | `str \| None` | API key for authenticated requests. |
| `github_token` | `str \| None` | GitHub token for resolving skill contents. |
| `proxy` | `str \| None` | Proxy URL for API requests. |

## SkillsMp (synchronous)

```python
client = SkillsMp(settings=ClientSettings(...))

# Search for skills
result: SkillsMpSearchResult = client.search("python")
result: SkillsMpSearchResult = client.search(
    SkillsMpSearchQuery(text="python", limit=10, page=1)
)

# AI-powered search
result: SkillsMpAiSearchResult = client.ai_search("code review skills")

# GitHub operations
items = client.fetch_github_directory(location, current_path)
blob = client.fetch_github_file(location, path)
snapshot = client.fetch_github_snapshot(location)
ref, sha = client.resolve_github_ref_and_commit_sha(location)
```

## AsyncSkillsMp

Same interface as `SkillsMp` but all methods are async, delegating to the
synchronous client via `asyncio.to_thread`.

```python
from skilly.skillsmp import AsyncSkillsMp

client = AsyncSkillsMp(settings=ClientSettings(...))
result = await client.search("python")
```

## SkillsMpSearchQuery

| Field | Type | Description |
|---|---|---|
| `text` | `str` | Search text. |
| `page` | `int \| None` | Page number for pagination. |
| `limit` | `int \| None` | Results per page. |
| `sort_by` | `str \| None` | Sort field. |
| `category` | `str \| None` | Filter by category. |
| `occupation` | `str \| None` | Filter by occupation. |

## SkillsMpSkill

| Field | Type | Description |
|---|---|---|
| `id` | `str` | Unique skill identifier. |
| `name` | `str` | Display name. |
| `author` | `str` | Author name. |
| `description` | `str` | Description text. |
| `github_url` | `str` | URL to the skill's GitHub source. |
| `skill_url` | `str` | URL to the skill on SkillsMP. |
| `stars` | `int \| None` | Star count. |
| `updated_at` | `str \| int \| None` | Last update timestamp. |

## Result types

### SkillsMpSearchResult

```python
@dataclass(frozen=True)
class SkillsMpSearchResult:
    success: bool
    data: SkillsMpSearchData
    meta: SkillsMpMeta | None = None
```

### SkillsMpSearchData

```python
@dataclass(frozen=True)
class SkillsMpSearchData:
    skills: list[SkillsMpSkill]
    pagination: SkillsMpPagination
    filters: SkillsMpFilters
```

### SkillsMpAiSearchResult

```python
@dataclass(frozen=True)
class SkillsMpAiSearchResult:
    success: bool
    data: SkillsMpAiSearchData
    meta: SkillsMpMeta | None = None
```

### SkillsMpAiSearchData

```python
@dataclass(frozen=True)
class SkillsMpAiSearchData:
    skills: list[SkillsMpSkill]
    results: list[SkillsMpSkill]
```
