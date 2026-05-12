import io
import json
import tarfile
from pathlib import Path

import niquests
import pytest

from skilly.filesystem import FileSystem
from skilly.repository import SkillRepository
from skilly.skills import (
    SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY,
    SKILLY_GITHUB_URL_METADATA_KEY,
    SKILLY_MANAGED_METADATA_KEY,
    SKILLY_MANAGED_METADATA_VALUE,
    SKILLY_SKILLSMP_ID_METADATA_KEY,
    Skill,
    discover_github_skills,
    parse_github_skill_url,
)
from skilly.skillsmp.client import (
    AsyncSkillsMp,
    SkillsMp,
    SkillsMpAiSearchApiResponse,
    SkillsMpSearchApiResponse,
)


SEARCH_RESPONSE = {
    "success": True,
    "data": {
        "skills": [
            {
                "id": "skill-1",
                "name": "python-production",
                "author": "idossha",
                "description": "Python production code patterns.",
                "githubUrl": "https://github.com/example/project/tree/main/skills/python",
                "skillUrl": "https://skillsmp.com/skills/skill-1",
                "stars": 42,
                "updatedAt": "1778091502",
            }
        ],
        "pagination": {
            "page": 2,
            "limit": 3,
            "total": 1000,
            "totalPages": 3,
            "hasNext": True,
            "hasPrev": True,
            "totalIsExact": False,
        },
        "filters": {
            "search": "python",
            "sortBy": "stars",
        },
    },
    "meta": {
        "requestId": "abc-123",
        "responseTimeMs": 120,
    },
}

AI_SEARCH_RESPONSE = {
    "success": True,
    "data": {
        "results": [
            {
                "id": "skill-2",
                "name": "semantic-python",
                "author": "skillsmp",
                "description": "Semantic Python search result.",
                "githubUrl": "https://github.com/example/project/tree/main/skills/semantic",
                "skillUrl": "https://skillsmp.com/skills/skill-2",
                "stars": 5,
                "updatedAt": "1778091600",
                "score": 0.98,
            }
        ]
    },
    "meta": {
        "requestId": "def-456",
        "responseTimeMs": 75,
    },
}


class FakeResponse:
    def __init__(
        self,
        payload: object | None = None,
        *,
        status_code: int = 200,
        content: bytes | None = None,
        text: str | None = None,
    ) -> None:
        self._payload = payload
        self.status_code = status_code
        self.ok = status_code < 400
        self.is_redirect = False
        self.request = None
        if payload is not None:
            self.content = json.dumps(payload).encode()
            self.text = json.dumps(payload)
        else:
            self.content = content or b""
            self.text = text or ""

    def json(self, **kwargs: object) -> object:
        if self._payload is None:
            raise AssertionError("No JSON payload configured")
        return self._payload

    def raise_for_status(self) -> None:
        if self.status_code >= 400:
            raise niquests.HTTPError(f"HTTP {self.status_code}")

    def iter_raw(self, chunk_size: int = 0):  # pragma: no cover - wrapper passthrough
        yield self.content

    def iter_content(
        self, chunk_size: int = 0, decode_unicode: bool = False
    ):  # pragma: no cover - wrapper passthrough
        yield self.text if decode_unicode else self.content

    def iter_lines(
        self,
        chunk_size: int = 0,
        decode_unicode: bool = False,
        delimiter: str | bytes | None = None,
    ):  # pragma: no cover - wrapper passthrough
        del chunk_size, delimiter
        yield self.text if decode_unicode else self.content


class FakeSession:
    def __init__(
        self,
        response: FakeResponse | None = None,
        *,
        responses: dict[tuple[str, str], FakeResponse] | None = None,
    ) -> None:
        self.response = response
        self.responses = responses or {}
        self.calls: list[dict[str, object]] = []

    def get(self, url: str, **kwargs: object) -> FakeResponse:
        self.calls.append({"url": url, **kwargs})
        key = (url, json.dumps(kwargs.get("params") or {}, sort_keys=True))
        if key in self.responses:
            return self.responses[key]
        if self.response is None:
            raise AssertionError(f"Unexpected GET request: {url} {kwargs}")
        return self.response


class FakeAsyncSession:
    def __init__(
        self,
        response: FakeResponse | None = None,
        *,
        responses: dict[tuple[str, str], FakeResponse] | None = None,
    ) -> None:
        self.response = response
        self.responses = responses or {}
        self.calls: list[dict[str, object]] = []

    async def get(self, url: str, **kwargs: object) -> FakeResponse:
        self.calls.append({"url": url, **kwargs})
        key = (url, json.dumps(kwargs.get("params") or {}, sort_keys=True))
        if key in self.responses:
            return self.responses[key]
        if self.response is None:
            raise AssertionError(f"Unexpected GET request: {url} {kwargs}")
        return self.response


class FakeFileSystem(FileSystem):
    def __init__(self) -> None:
        self._dirs: set[Path] = {Path("/")}
        self._files: dict[Path, str] = {}

    def add_dir(self, path: Path) -> None:
        current = self.resolve(path)
        while True:
            self._dirs.add(current)
            if current.parent == current:
                break
            current = current.parent

    def read_file(self, path: Path) -> str:
        resolved = self.resolve(path)
        if resolved not in self._files:
            raise FileNotFoundError(resolved)
        return self._files[resolved]

    def write_file(self, path: Path, content: str) -> None:
        resolved = self.resolve(path)
        self.add_dir(resolved.parent)
        self._files[resolved] = content

    def list_files(self, path: Path) -> list[str]:
        resolved = self.resolve(path)
        if resolved not in self._dirs:
            raise FileNotFoundError(resolved)

        children: set[str] = set()
        for directory in self._dirs:
            if directory.parent == resolved and directory != resolved:
                children.add(directory.name)
        for file_path in self._files:
            if file_path.parent == resolved:
                children.add(file_path.name)
        return sorted(children)

    def exists(self, path: Path) -> bool:
        resolved = self.resolve(path)
        return resolved in self._dirs or resolved in self._files

    def is_dir(self, path: Path) -> bool:
        return self.resolve(path) in self._dirs

    def make_dir(
        self, path: Path, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        del exist_ok
        resolved = self.resolve(path)
        if parents:
            self.add_dir(resolved)
            return
        if resolved.parent not in self._dirs:
            raise FileNotFoundError(resolved.parent)
        self._dirs.add(resolved)

    def remove_tree(self, path: Path) -> None:
        resolved = self.resolve(path)
        file_paths = [
            file_path for file_path in self._files if file_path.is_relative_to(resolved)
        ]
        for file_path in file_paths:
            del self._files[file_path]
        dir_paths = [
            directory for directory in self._dirs if directory.is_relative_to(resolved)
        ]
        for directory in sorted(dir_paths, reverse=True):
            if directory != Path("/"):
                self._dirs.discard(directory)

    def resolve(self, path: Path) -> Path:
        return Path("/").joinpath(path).resolve() if not path.is_absolute() else path


def _download_responses(
    api_base: str,
    *,
    ref: str = "main",
    skill_dir: str = ".agents/skills/python",
) -> dict[tuple[str, str], FakeResponse]:
    commit_sha = "0123456789abcdef0123456789abcdef01234567"
    tarball = _build_tarball_bytes(
        {
            f"{skill_dir}/SKILL.md": "---\nname: python\ndescription: Use python.\n---\nBody\n",
            f"{skill_dir}/scripts/extract.py": "print('hi')\n",
        },
        root_dir=f"example-project-{commit_sha[:7]}",
    )
    return {
        (
            f"{api_base}/commits/{ref}",
            json.dumps({}, sort_keys=True),
        ): FakeResponse(
            {
                "sha": commit_sha,
            }
        ),
        (
            f"{api_base}/tarball/{commit_sha}",
            json.dumps({}, sort_keys=True),
        ): FakeResponse(content=tarball),
    }


def _download_repo_responses(api_base: str) -> dict[tuple[str, str], FakeResponse]:
    commit_sha = "0123456789abcdef0123456789abcdef01234567"
    tarball = _build_tarball_bytes(
        {
            "README.md": "# README\n",
            "skills/alpha/SKILL.md": "---\nname: alpha\ndescription: Use alpha.\n---\nBody\n",
            "skills/alpha/scripts/extract.py": "print('alpha')\n",
            "skills/beta/SKILL.md": "---\nname: beta\ndescription: Use beta.\n---\nBody\n",
        },
        root_dir=f"example-project-{commit_sha[:7]}",
    )
    return {
        (
            api_base,
            json.dumps({}, sort_keys=True),
        ): FakeResponse(
            {
                "default_branch": "main",
            }
        ),
        (
            f"{api_base}/commits/main",
            json.dumps({}, sort_keys=True),
        ): FakeResponse(
            {
                "sha": commit_sha,
            }
        ),
        (
            f"{api_base}/tarball/{commit_sha}",
            json.dumps({}, sort_keys=True),
        ): FakeResponse(content=tarball),
    }


def _build_tarball_bytes(files: dict[str, str], *, root_dir: str) -> bytes:
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tar:
        for path, content in sorted(files.items()):
            data = content.encode("utf-8")
            info = tarfile.TarInfo(name=f"{root_dir}/{path}")
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return archive.getvalue()


def test_skillsmp_search_builds_request_and_parses_response() -> None:
    session = FakeSession(FakeResponse(SEARCH_RESPONSE))
    client = SkillsMp(session)

    response = client.search(
        "python",
        page=2,
        limit=3,
        sort_by="stars",
        category="data-ai",
    )

    assert isinstance(response.parsed_data, SkillsMpSearchApiResponse)
    assert response.parsed_data.data.skills[0].name == "python-production"
    assert session.calls == [
        {
            "url": "https://skillsmp.com/api/v1/skills/search",
            "headers": {"Accept": "application/json"},
            "params": {
                "q": "python",
                "page": "2",
                "limit": "3",
                "sortBy": "stars",
                "category": "data-ai",
            },
            "proxies": None,
            "stream": False,
        }
    ]


def test_parse_github_url_returns_skill_location() -> None:
    location = parse_github_skill_url(
        "https://github.com/example/project/tree/main/.agents/skills/python"
    )

    assert location.owner == "example"
    assert location.repo == "project"
    assert location.ref == "main"
    assert location.path.as_posix() == ".agents/skills/python"
    assert location.skill_name == "python"


def test_parse_github_url_accepts_repository_root() -> None:
    location = parse_github_skill_url("https://github.com/example/project")

    assert location.owner == "example"
    assert location.repo == "project"
    assert location.ref is None
    assert location.path.as_posix() == "."
    assert location.skill_name == "project"


def test_download_skill_downloads_all_files(tmp_path: Path) -> None:
    skill_url = "https://github.com/example/project/tree/main/.agents/skills/python"
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(responses=_download_responses(api_base))
    client = SkillsMp(session)
    repository = SkillRepository()

    installed = repository.install(
        Skill.from_github(client, skill_url), directory=tmp_path
    )

    assert installed.directory == (tmp_path / "python").resolve()
    skill_md = installed.directory.joinpath("SKILL.md").read_text(encoding="utf-8")
    assert "name: python" in skill_md
    assert "metadata:" in skill_md
    assert (
        f"  {SKILLY_MANAGED_METADATA_KEY}: {SKILLY_MANAGED_METADATA_VALUE}" in skill_md
    )
    assert f"  {SKILLY_GITHUB_URL_METADATA_KEY}: {skill_url}" in skill_md
    assert (
        f"  {SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY}: "
        "0123456789abcdef0123456789abcdef01234567"
    ) in skill_md
    assert installed.github_commit_sha == "0123456789abcdef0123456789abcdef01234567"
    assert (
        installed.directory.joinpath("scripts/extract.py").read_text(encoding="utf-8")
        == "print('hi')\n"
    )
    assert [call["url"] for call in session.calls] == [
        f"{api_base}/commits/main",
        f"{api_base}/tarball/0123456789abcdef0123456789abcdef01234567",
    ]


def test_download_skill_uses_custom_skill_name(tmp_path: Path) -> None:
    skill_url = "https://github.com/example/project/tree/main/.agents/skills/python"
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(responses=_download_responses(api_base))
    client = SkillsMp(session)
    repository = SkillRepository()

    installed = repository.install(
        Skill.from_github(client, skill_url),
        directory=tmp_path,
        skill_name="custom-skill",
    )

    assert installed.directory == (tmp_path / "custom-skill").resolve()
    assert (tmp_path / "custom-skill" / "SKILL.md").exists()


def test_discover_github_skills_from_repository_root() -> None:
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(responses=_download_repo_responses(api_base))
    client = SkillsMp(session)

    skills = discover_github_skills(client, "https://github.com/example/project")

    assert [skill.name for skill in skills] == ["alpha", "beta"]
    assert (
        skills[0].github_url
        == "https://github.com/example/project/tree/main/skills/alpha"
    )
    assert skills[0].can_update() is True
    assert [resource.relative_path.as_posix() for resource in skills[0].resources] == [
        "scripts/extract.py"
    ]
    assert (
        skills[1].github_url
        == "https://github.com/example/project/tree/main/skills/beta"
    )
    assert [call["url"] for call in session.calls] == [
        api_base,
        f"{api_base}/commits/main",
        f"{api_base}/tarball/0123456789abcdef0123456789abcdef01234567",
    ]


def test_download_skill_uses_github_authentication(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    skill_url = "https://github.com/example/project/tree/main/.agents/skills/python"
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(responses=_download_responses(api_base))
    monkeypatch.setenv("GITHUB_TOKEN", "ghp_test_token")

    installed = SkillRepository().install(
        Skill.from_github(SkillsMp(session), skill_url),
        directory=tmp_path,
    )

    assert installed.name == "python"
    assert session.calls == [
        {
            "url": f"{api_base}/commits/main",
            "headers": {
                "Accept": "application/vnd.github+json",
                "Authorization": "Bearer ghp_test_token",
            },
            "params": None,
            "proxies": None,
            "stream": False,
        },
        {
            "url": f"{api_base}/tarball/0123456789abcdef0123456789abcdef01234567",
            "headers": {
                "Accept": "application/vnd.github+json",
                "Authorization": "Bearer ghp_test_token",
            },
            "params": None,
            "proxies": None,
            "stream": False,
        },
    ]


def test_install_skill_stores_skillsmp_metadata(tmp_path: Path) -> None:
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    repository = SkillRepository()
    skill = client.search("python").parsed_data.data.skills[0]

    installed = repository.install(
        Skill.from_skillsmp(client, skill), directory=tmp_path
    )

    skill_md = installed.directory.joinpath("SKILL.md").read_text(encoding="utf-8")
    assert (
        f"  {SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY}: "
        "0123456789abcdef0123456789abcdef01234567"
    ) in skill_md
    assert f"  {SKILLY_SKILLSMP_ID_METADATA_KEY}: {skill.id}" in skill_md
    assert f"  {SKILLY_GITHUB_URL_METADATA_KEY}: {skill.githubUrl}" in skill_md


def test_list_update_and_remove_installed_skill(tmp_path: Path) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    api_base = "https://api.github.com/repos/example/project"
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    repository = SkillRepository()
    skill = client.search("python").parsed_data.data.skills[0]

    repository.install(Skill.from_skillsmp(client, skill), directory=install_directory)

    installed_skills = repository.list(install_directory)
    assert [installed_skill.directory_name for installed_skill in installed_skills] == [
        "python"
    ]
    assert installed_skills[0].is_installed() is True
    assert installed_skills[0].skillsmp_id == skill.id

    updated = repository.install(
        Skill.from_github(
            client,
            installed_skills[0].github_url,
            source=installed_skills[0].source,
            skillsmp_id=installed_skills[0].skillsmp_id,
        ),
        directory=install_directory,
        skill_name="python",
        replace=True,
    )
    assert updated.directory == (install_directory / "python").resolve()

    removed = repository.remove("python", directory=install_directory)
    assert removed.directory_name == "python"
    assert repository.list(install_directory) == []


def test_skillsmp_client_uses_injected_file_system() -> None:
    install_directory = Path("/workspace/.agents/skills")
    api_base = "https://api.github.com/repos/example/project"
    file_system = FakeFileSystem()
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    repository = SkillRepository(file_system=file_system)
    skill = client.search("python").parsed_data.data.skills[0]

    installed = repository.install(
        Skill.from_skillsmp(client, skill), directory=install_directory
    )

    skill_md_path = installed.directory / "SKILL.md"
    assert skill_md_path == Path("/workspace/.agents/skills/python/SKILL.md")
    skill_md = file_system.read_file(skill_md_path)
    assert f"  {SKILLY_SKILLSMP_ID_METADATA_KEY}: {skill.id}" in skill_md
    assert repository.list(install_directory)[0].directory == Path(
        "/workspace/.agents/skills/python"
    )

    repository.remove("python", directory=install_directory)
    assert repository.list(install_directory) == []


def test_skillsmp_ai_search_requires_api_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("SKILLSMP_API_KEY", raising=False)
    client = SkillsMp(FakeSession(FakeResponse(AI_SEARCH_RESPONSE)))

    with pytest.raises(ValueError, match="API key is required"):
        client.ai_search("python")


@pytest.mark.asyncio
async def test_async_skillsmp_ai_search_uses_response_wrapper(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("SKILLSMP_API_KEY", "sk_live_test")
    session = FakeAsyncSession(FakeResponse(AI_SEARCH_RESPONSE))
    client = AsyncSkillsMp(session)

    response = await client.ai_search("semantic python")
    parsed = await response.parsed_data

    assert isinstance(parsed, SkillsMpAiSearchApiResponse)
    assert parsed.data.results[0].name == "semantic-python"
    assert session.calls == [
        {
            "url": "https://skillsmp.com/api/v1/skills/ai-search",
            "headers": {
                "Accept": "application/json",
                "Authorization": "Bearer sk_live_test",
            },
            "params": {"q": "semantic python"},
            "proxies": None,
            "stream": False,
        }
    ]
