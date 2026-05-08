import json
from pathlib import Path

import niquests
import pytest

from skilly.filesystem import FileSystem
from skilly.skills import (
    DownloadedSkill,
    ManagedSkills,
    SKILLY_GITHUB_URL_METADATA_KEY,
    SKILLY_MANAGED_METADATA_KEY,
    SKILLY_MANAGED_METADATA_VALUE,
    SKILLY_SKILLSMP_ID_METADATA_KEY,
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
    def __init__(self, payload: object, *, status_code: int = 200) -> None:
        self._payload = payload
        self.status_code = status_code
        self.ok = status_code < 400
        self.is_redirect = False
        self.request = None
        self.content = json.dumps(payload).encode()
        self.text = json.dumps(payload)

    def json(self, **kwargs: object) -> object:
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
        key = (url, json.dumps(kwargs.get("params", {}), sort_keys=True))
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
        key = (url, json.dumps(kwargs.get("params", {}), sort_keys=True))
        if key in self.responses:
            return self.responses[key]
        if self.response is None:
            raise AssertionError(f"Unexpected GET request: {url} {kwargs}")
        return self.response


class FakeFileSystem(FileSystem):
    def __init__(self) -> None:
        self._dirs: set[Path] = {Path("/")}
        self._files: dict[Path, bytes] = {}

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
        return self._files[resolved].decode("utf-8")

    def write_bytes(self, path: Path, content: bytes) -> None:
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

    def read_bytes(self, path: Path) -> bytes:
        resolved = self.resolve(path)
        if resolved not in self._files:
            raise FileNotFoundError(resolved)
        return self._files[resolved]


def _download_responses(
    api_base: str,
    *,
    ref: str = "main",
    skill_dir: str = ".agents/skills/python",
) -> dict[tuple[str, str], FakeResponse]:
    return {
        (
            f"{api_base}/{skill_dir}",
            json.dumps({"ref": ref}, sort_keys=True),
        ): FakeResponse(
            [
                {
                    "type": "file",
                    "name": "SKILL.md",
                    "path": f"{skill_dir}/SKILL.md",
                    "url": f"{api_base}/{skill_dir}/SKILL.md",
                },
                {
                    "type": "dir",
                    "name": "scripts",
                    "path": f"{skill_dir}/scripts",
                    "url": f"{api_base}/{skill_dir}/scripts",
                },
            ]
        ),
        (
            f"{api_base}/{skill_dir}/scripts",
            json.dumps({"ref": ref}, sort_keys=True),
        ): FakeResponse(
            [
                {
                    "type": "file",
                    "name": "extract.py",
                    "path": f"{skill_dir}/scripts/extract.py",
                    "url": f"{api_base}/{skill_dir}/scripts/extract.py",
                }
            ]
        ),
        (
            f"{api_base}/{skill_dir}/SKILL.md",
            json.dumps({"ref": ref}, sort_keys=True),
        ): FakeResponse(
            {
                "type": "file",
                "name": "SKILL.md",
                "path": f"{skill_dir}/SKILL.md",
                "encoding": "base64",
                "content": "LS0tCm5hbWU6IHB5dGhvbgpkZXNjcmlwdGlvbjogVXNlIHB5dGhvbi4KLS0tCkJvZHkK",
            }
        ),
        (
            f"{api_base}/{skill_dir}/scripts/extract.py",
            json.dumps({"ref": ref}, sort_keys=True),
        ): FakeResponse(
            {
                "type": "file",
                "name": "extract.py",
                "path": f"{skill_dir}/scripts/extract.py",
                "encoding": "base64",
                "content": "cHJpbnQoJ2hpJykK",
            }
        ),
    }


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
    managed_skills = ManagedSkills()

    location = managed_skills.parse_github_url(
        "https://github.com/example/project/tree/main/.agents/skills/python"
    )

    assert location.owner == "example"
    assert location.repo == "project"
    assert location.ref == "main"
    assert location.path.as_posix() == ".agents/skills/python"
    assert location.skill_name == "python"


def test_download_skill_downloads_all_files(tmp_path: Path) -> None:
    skill_url = "https://github.com/example/project/tree/main/.agents/skills/python"
    api_base = "https://api.github.com/repos/example/project/contents"
    session = FakeSession(responses=_download_responses(api_base))
    client = SkillsMp(session)
    managed_skills = ManagedSkills()

    downloaded = managed_skills.download_skill(client, skill_url, directory=tmp_path)

    assert isinstance(downloaded, DownloadedSkill)
    assert downloaded.destination == (tmp_path / "python").resolve()
    assert [file.source_path.as_posix() for file in downloaded.files] == [
        ".agents/skills/python/SKILL.md",
        ".agents/skills/python/scripts/extract.py",
    ]
    skill_md = (tmp_path / "python" / "SKILL.md").read_text(encoding="utf-8")
    assert "name: python" in skill_md
    assert "metadata:" in skill_md
    assert (
        f"  {SKILLY_MANAGED_METADATA_KEY}: {SKILLY_MANAGED_METADATA_VALUE}" in skill_md
    )
    assert f"  {SKILLY_GITHUB_URL_METADATA_KEY}: {skill_url}" in skill_md
    assert (tmp_path / "python" / "scripts/extract.py").read_text(
        encoding="utf-8"
    ) == "print('hi')\n"


def test_download_skill_uses_custom_skill_name(tmp_path: Path) -> None:
    skill_url = "https://github.com/example/project/tree/main/.agents/skills/python"
    api_base = "https://api.github.com/repos/example/project/contents"
    session = FakeSession(responses=_download_responses(api_base))
    client = SkillsMp(session)
    managed_skills = ManagedSkills()

    downloaded = managed_skills.download_skill(
        client,
        skill_url,
        directory=tmp_path,
        skill_name="custom-skill",
    )

    assert downloaded.destination == (tmp_path / "custom-skill").resolve()
    assert (tmp_path / "custom-skill" / "SKILL.md").exists()


def test_install_skill_stores_skillsmp_metadata(tmp_path: Path) -> None:
    api_base = "https://api.github.com/repos/example/project/contents"
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    managed_skills = ManagedSkills()
    skill = client.search("python").parsed_data.data.skills[0]

    downloaded = managed_skills.install_skill(client, skill, directory=tmp_path)

    skill_md = downloaded.destination.joinpath("SKILL.md").read_text(encoding="utf-8")
    assert f"  {SKILLY_SKILLSMP_ID_METADATA_KEY}: {skill.id}" in skill_md
    assert f"  {SKILLY_GITHUB_URL_METADATA_KEY}: {skill.githubUrl}" in skill_md


def test_list_update_and_remove_installed_skill(tmp_path: Path) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    api_base = "https://api.github.com/repos/example/project/contents"
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    managed_skills = ManagedSkills()
    skill = client.search("python").parsed_data.data.skills[0]

    managed_skills.install_skill(client, skill, directory=install_directory)

    installed_skills = managed_skills.list_installed_skills(install_directory)
    assert [installed_skill.directory_name for installed_skill in installed_skills] == [
        "python"
    ]
    assert installed_skills[0].managed_by_skilly is True
    assert installed_skills[0].skillsmp_id == skill.id

    updated = managed_skills.update_installed_skill(
        client, "python", directory=install_directory
    )
    assert updated.destination == (install_directory / "python").resolve()

    removed = managed_skills.remove_installed_skill(
        "python", directory=install_directory
    )
    assert removed.directory_name == "python"
    assert managed_skills.list_installed_skills(install_directory) == []


def test_skillsmp_client_uses_injected_file_system() -> None:
    install_directory = Path("/workspace/.agents/skills")
    api_base = "https://api.github.com/repos/example/project/contents"
    file_system = FakeFileSystem()
    session = FakeSession(
        response=FakeResponse(SEARCH_RESPONSE),
        responses=_download_responses(api_base, skill_dir="skills/python"),
    )
    client = SkillsMp(session)
    managed_skills = ManagedSkills(file_system=file_system)
    skill = client.search("python").parsed_data.data.skills[0]

    downloaded = managed_skills.install_skill(
        client, skill, directory=install_directory
    )

    skill_md_path = downloaded.destination / "SKILL.md"
    assert skill_md_path == Path("/workspace/.agents/skills/python/SKILL.md")
    skill_md = file_system.read_file(skill_md_path)
    assert f"  {SKILLY_SKILLSMP_ID_METADATA_KEY}: {skill.id}" in skill_md
    assert managed_skills.list_installed_skills(install_directory)[0].directory == Path(
        "/workspace/.agents/skills/python"
    )

    managed_skills.remove_installed_skill("python", directory=install_directory)
    assert managed_skills.list_installed_skills(install_directory) == []


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
