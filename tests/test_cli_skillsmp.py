from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path
from typing import cast

import pytest

from skilly.cli import skillsmp as skillsmp_cli
from skilly.cli.ui import Menu, MenuValue
from skilly.repository import SkillRepository
from skilly.skills import Skill
from skilly.skillsmp.client import SkillsMpSearchApiResponse


def test_skillsmp_list_only_shows_skillsmp_installed_skills(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = SkillRepository()
    repository.install(
        Skill.from_text(
            """---
name: dependency-skill
description: From a dependency.
---
Body
""",
            path=tmp_path / "dep" / "SKILL.md",
            source="dependency",
            package_name="dep-pkg",
            package_version="1.2.3",
        ),
        directory=install_directory,
        skill_name="dependency-skill",
    )
    repository.install(
        Skill.from_text(
            """---
name: skillsmp-skill
description: From SkillsMP.
---
Body
""",
            path=tmp_path / "skillsmp" / "SKILL.md",
            source="skillsmp",
            github_url="https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill",
            skillsmp_id="skill-1",
        ),
        directory=install_directory,
        skill_name="skillsmp-skill",
    )
    ui = FakeInteractiveUi([None])
    monkeypatch.setattr(skillsmp_cli, "cli_ui", ui)

    skillsmp_cli.list(directory=install_directory)

    assert [item.label for item in ui.menus[0].items] == [
        "skillsmp-skill: skillsmp-skill",
        skillsmp_cli.EXIT_CHOICE,
    ]
    assert "SkillsMP Id: skill-1" in ui.menus[0].items[0].preview_lines


def test_skillsmp_list_reports_when_no_skillsmp_skills_exist(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    SkillRepository().install(
        Skill.from_text(
            """---
name: dependency-skill
description: From a dependency.
---
Body
""",
            path=tmp_path / "dep" / "SKILL.md",
            source="dependency",
            package_name="dep-pkg",
            package_version="1.2.3",
        ),
        directory=install_directory,
        skill_name="dependency-skill",
    )

    skillsmp_cli.list(directory=install_directory)

    assert (
        capsys.readouterr().out.strip()
        == f"No SkillsMP-installed skills found in {install_directory.resolve()}"
    )


def test_skillsmp_list_skips_update_when_commit_matches(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    installed = SkillRepository().install(
        Skill.from_text(
            """---
name: skillsmp-skill
description: From SkillsMP.
---
Body
""",
            path=tmp_path / "skillsmp" / "SKILL.md",
            source="skillsmp",
            github_url="https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill",
            github_commit_sha="0123456789abcdef0123456789abcdef01234567",
            skillsmp_id="skill-1",
        ),
        directory=install_directory,
        skill_name="skillsmp-skill",
    )
    ui = FakeInteractiveUi([installed, "update", None])
    monkeypatch.setattr(skillsmp_cli, "SkillsMp", lambda: FakePinnedSkillsMpClient())
    monkeypatch.setattr(skillsmp_cli, "cli_ui", ui)

    skillsmp_cli.list(directory=install_directory)

    assert (
        "skillsmp-skill is already up to date "
        "(0123456789abcdef0123456789abcdef01234567)" in capsys.readouterr().out
    )


def test_skillsmp_download_installs_all_skills_from_repository_root(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    monkeypatch.setattr(skillsmp_cli, "SkillsMp", lambda: FakeRepoDownloadClient())

    skillsmp_cli.download(
        "https://github.com/example/project",
        directory=install_directory,
    )

    installed_skills = SkillRepository().list(install_directory)
    assert [skill.directory_name for skill in installed_skills] == ["alpha", "beta"]
    output = capsys.readouterr().out
    assert "Downloaded alpha with 2 files to" in output
    assert "Downloaded beta with 1 files to" in output


@pytest.mark.asyncio
async def test_skillsmp_search_installs_selected_skill_with_live_preview(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    search_result = SkillsMpSearchApiResponse.model_validate(
        {
            "success": True,
            "data": {
                "skills": [
                    {
                        "id": "skill-1",
                        "name": "skillsmp-skill",
                        "author": "skillsmp",
                        "description": "From SkillsMP.",
                        "githubUrl": "https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill",
                        "skillUrl": "https://skillsmp.com/skills/skill-1",
                    }
                ],
                "pagination": {
                    "page": 1,
                    "limit": 10,
                    "total": 1,
                    "totalPages": 1,
                    "hasNext": False,
                    "hasPrev": False,
                },
                "filters": {},
            },
        }
    )
    search_client = FakeSkillsMpClient(search_result)
    ui = FakeInteractiveUi([search_result.data.skills[0], "install", None])
    monkeypatch.setattr(skillsmp_cli, "SkillsMp", lambda: search_client)
    monkeypatch.setattr(skillsmp_cli, "cli_ui", ui)

    await skillsmp_cli.search("demo", directory=install_directory)

    installed_skill = SkillRepository().find(
        "skillsmp-skill", directory=install_directory
    )
    assert installed_skill is not None
    assert installed_skill.skillsmp_id == "skill-1"
    assert (install_directory / "skillsmp-skill" / "scripts" / "updated.py").read_text(
        encoding="utf-8"
    ) == "print('updated')\n"
    assert "Description: From SkillsMP." in ui.menus[0].items[0].preview_lines
    assert ui.menus[0].items[-1].label == skillsmp_cli.EXIT_CHOICE
    assert "Files:" in ui.menus[1].items[0].preview_lines
    assert "  scripts/updated.py" in ui.menus[1].items[0].preview_lines
    assert "Installed skillsmp-skill to" in capsys.readouterr().out


class FakeSearchResponse:
    def __init__(self, parsed_data: SkillsMpSearchApiResponse) -> None:
        self._parsed_data = parsed_data

    @property
    def parsed_data(self) -> SkillsMpSearchApiResponse:
        return self._parsed_data


class FakeSkillsMpClient:
    def __init__(self, parsed_data: SkillsMpSearchApiResponse) -> None:
        self._parsed_data = parsed_data
        self.queries: list[str] = []

    def search(self, query: str) -> FakeSearchResponse:
        self.queries.append(query)
        return FakeSearchResponse(self._parsed_data)

    def fetch_github_directory(
        self, location: object, current_path: object
    ) -> list[object]:
        from skilly.skills import GitHubContentItem

        if str(current_path) == ".agents/skills/skillsmp-skill":
            return [
                GitHubContentItem(
                    type="file",
                    name="SKILL.md",
                    path=location.path / "SKILL.md",
                ),
                GitHubContentItem(
                    type="dir",
                    name="scripts",
                    path=location.path / "scripts",
                ),
            ]
        return [
            GitHubContentItem(
                type="file",
                name="updated.py",
                path=location.path / "scripts" / "updated.py",
            )
        ]

    def fetch_github_file(self, location: object, path: object) -> object:
        from skilly.skills import GitHubFileBlob

        del location
        if str(path).endswith("SKILL.md"):
            return GitHubFileBlob(
                path=path,
                content="---\nname: skillsmp-skill\ndescription: From SkillsMP.\n---\nBody\n",
                size=58,
            )
        return GitHubFileBlob(
            path=path,
            content="print('updated')\n",
            size=17,
        )


class FakeRepoDownloadClient:
    def fetch_github_directory(
        self, location: object, current_path: object
    ) -> list[object]:
        from skilly.skills import GitHubContentItem

        current_path_text = str(current_path)
        if current_path_text == ".":
            return [
                GitHubContentItem(
                    type="dir", name="skills", path=location.path / "skills"
                )
            ]
        if current_path_text == "skills":
            return [
                GitHubContentItem(
                    type="dir", name="alpha", path=current_path / "alpha"
                ),
                GitHubContentItem(type="dir", name="beta", path=current_path / "beta"),
            ]
        if current_path_text == "skills/alpha":
            return [
                GitHubContentItem(
                    type="file",
                    name="SKILL.md",
                    path=current_path / "SKILL.md",
                ),
                GitHubContentItem(
                    type="dir",
                    name="scripts",
                    path=current_path / "scripts",
                ),
            ]
        if current_path_text == "skills/alpha/scripts":
            return [
                GitHubContentItem(
                    type="file",
                    name="extract.py",
                    path=current_path / "extract.py",
                )
            ]
        if current_path_text == "skills/beta":
            return [
                GitHubContentItem(
                    type="file",
                    name="SKILL.md",
                    path=current_path / "SKILL.md",
                )
            ]
        raise AssertionError(
            f"Unexpected GitHub directory request: {current_path_text}"
        )

    def fetch_github_file(self, location: object, path: object) -> object:
        from skilly.skills import GitHubFileBlob

        del location
        path_text = str(path)
        if path_text == "skills/alpha/SKILL.md":
            return GitHubFileBlob(
                path=path,
                content="---\nname: alpha\ndescription: Alpha skill.\n---\nBody\n",
                size=48,
            )
        if path_text == "skills/alpha/scripts/extract.py":
            return GitHubFileBlob(
                path=path,
                content="print('alpha')\n",
                size=15,
            )
        if path_text == "skills/beta/SKILL.md":
            return GitHubFileBlob(
                path=path,
                content="---\nname: beta\ndescription: Beta skill.\n---\nBody\n",
                size=46,
            )
        raise AssertionError(f"Unexpected GitHub file request: {path_text}")


class FakePinnedSkillsMpClient:
    def fetch_github_directory(
        self, location: object, current_path: object
    ) -> list[object]:
        from skilly.skills import GitHubContentItem

        return [
            GitHubContentItem(
                type="file",
                name="SKILL.md",
                path=current_path / "SKILL.md",
                commit_sha="0123456789abcdef0123456789abcdef01234567",
            )
        ]

    def fetch_github_file(self, location: object, path: object) -> object:
        from skilly.skills import GitHubFileBlob

        del location
        return GitHubFileBlob(
            path=path,
            content="---\nname: skillsmp-skill\ndescription: From SkillsMP.\n---\nBody\n",
            size=58,
            commit_sha="0123456789abcdef0123456789abcdef01234567",
        )


class FakeInteractiveUi:
    def __init__(self, responses: Sequence[object | None]) -> None:
        self._responses = list(responses)
        self.menus: list[object] = []

    def select(self, menu: Menu[MenuValue]) -> MenuValue | None:
        self.menus.append(menu)
        if not self._responses:
            raise AssertionError("No more UI responses configured")
        return cast(MenuValue | None, self._responses.pop(0))

    async def select_async(self, menu: Menu[MenuValue]) -> MenuValue | None:
        return self.select(menu)
