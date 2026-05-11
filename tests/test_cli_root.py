from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path, PurePosixPath
from typing import cast

import pytest

from skilly.cli import root
from skilly.cli.ui import Menu, MenuValue
from skilly.repository import SkillMatch, SkillRepository
from skilly.skills import GitHubContentItem, GitHubFileBlob, GitHubSkillLocation, Skill


def test_scan_installs_selected_dependency_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    available = make_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
    )
    repository = FakeRepository(matches=[SkillMatch(available=available)])
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([repository.matches[0], "install"])
    monkeypatch.setattr(root, "cli_ui", ui)

    install_directory = tmp_path / ".agents" / "skills"
    root.scan(directory=install_directory)

    installed_skill = SkillRepository().find(
        "sample-skill", directory=install_directory
    )
    assert installed_skill is not None
    assert installed_skill.is_dependency() is True
    assert installed_skill.package_name == "sample-pkg"
    assert installed_skill.package_version == "1.2.3"
    assert (install_directory / "sample-skill" / "scripts" / "extract.py").read_text(
        encoding="utf-8"
    ) == "print('sample')\n"
    assert "Installed sample-skill" in capsys.readouterr().out


def test_scan_cancel_skips_install(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    available = make_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
    )
    repository = FakeRepository(matches=[SkillMatch(available=available)])
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([None])
    monkeypatch.setattr(root, "cli_ui", ui)

    install_directory = tmp_path / ".agents" / "skills"
    root.scan(directory=install_directory)

    assert not (install_directory / "sample-skill").exists()
    assert capsys.readouterr().out == ""
    assert ui.menus[0].items[0].label == root.scan_choice_label(repository.matches[0])
    assert ui.menus[0].items[-1].label == root.EXIT_CHOICE


def test_scan_preview_includes_selected_skill_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    available = make_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
    )
    repository = FakeRepository(matches=[SkillMatch(available=available)])
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([None])
    monkeypatch.setattr(root, "cli_ui", ui)

    install_directory = tmp_path / ".agents" / "skills"
    root.scan(directory=install_directory)

    preview_lines = ui.menus[0].items[0].preview_lines
    assert "Files:" in preview_lines
    assert "  SKILL.md" in preview_lines
    assert "  scripts/extract.py" in preview_lines


def test_list_removes_selected_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        make_skill(
            tmp_path,
            package_name="dep-pkg",
            package_version="1.2.3",
            skill_name="dependency-skill",
        ),
        directory=install_directory,
    )
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([installed, "remove"])
    monkeypatch.setattr(root, "cli_ui", ui)

    root.list(directory=install_directory)

    assert not (install_directory / "dependency-skill").exists()
    assert "Removed dependency-skill" in capsys.readouterr().out


def test_list_updates_dependency_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    original = make_skill(
        tmp_path,
        package_name="dep-pkg",
        package_version="1.2.3",
        skill_name="dependency-skill",
        body="Original body.\n",
    )
    installed = repository.install(original, directory=install_directory)
    repository.available_skill = make_skill(
        tmp_path,
        package_name="dep-pkg",
        package_version="1.2.4",
        skill_name="dependency-skill",
        body="Updated body.\n",
    )
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([installed, "update", None])
    monkeypatch.setattr(root, "cli_ui", ui)

    root.list(directory=install_directory)

    refreshed = SkillRepository().find("dependency-skill", directory=install_directory)
    assert refreshed is not None
    assert refreshed.package_version == "1.2.4"
    assert "Updated dependency-skill to 1.2.4" in capsys.readouterr().out


def test_list_updates_skillsmp_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        Skill.from_text(
            """---
name: skillsmp-skill
description: From SkillsMP.
---
Body
""",
            path=tmp_path / "skillsmp-source" / "SKILL.md",
            source="skillsmp",
            github_url="https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill",
            skillsmp_id="skill-1",
        ),
        directory=install_directory,
    )
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([installed, "update", None])
    monkeypatch.setattr(root, "cli_ui", ui)
    monkeypatch.setattr(root, "SkillsMp", lambda: FakeGitHubFetcher())

    root.list(directory=install_directory)

    assert (install_directory / "skillsmp-skill" / "scripts" / "updated.py").read_text(
        encoding="utf-8"
    ) == "print('updated')\n"
    assert "Updated skillsmp-skill with 2 files" in capsys.readouterr().out


def test_list_skips_github_update_when_commit_matches(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        Skill.from_text(
            """---
name: skillsmp-skill
description: From SkillsMP.
---
Body
""",
            path=tmp_path / "skillsmp-source" / "SKILL.md",
            source="skillsmp",
            github_url="https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill",
            github_commit_sha="0123456789abcdef0123456789abcdef01234567",
            skillsmp_id="skill-1",
        ),
        directory=install_directory,
    )
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([installed, "update", None])
    monkeypatch.setattr(root, "cli_ui", ui)
    monkeypatch.setattr(root, "SkillsMp", lambda: FakePinnedGitHubFetcher())

    root.list(directory=install_directory)

    assert not (
        install_directory / "skillsmp-skill" / "scripts" / "updated.py"
    ).exists()
    assert (
        "skillsmp-skill is already up to date "
        "(0123456789abcdef0123456789abcdef01234567)" in capsys.readouterr().out
    )


def test_list_preview_includes_installed_skill_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        make_skill(
            tmp_path,
            package_name="dep-pkg",
            package_version="1.2.3",
            skill_name="dependency-skill",
        ),
        directory=install_directory,
    )
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)
    ui = FakeInteractiveUi([None])
    monkeypatch.setattr(root, "cli_ui", ui)

    root.list(directory=install_directory)

    assert ui.menus[0].items[0].label == root.installed_skill_label(installed)
    assert "  scripts/extract.py" in ui.menus[0].items[0].preview_lines
    assert ui.menus[0].items[-1].label == root.EXIT_CHOICE


def test_update_force_updates_outdated_dependency_skills(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        make_skill(
            tmp_path,
            package_name="sample-pkg",
            package_version="1.2.3",
            skill_name="sample-skill",
            body="Original body.\n",
        ),
        directory=install_directory,
    )
    available = make_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.4",
        skill_name="sample-skill",
        body="Updated body.\n",
    )
    repository.matches = [SkillMatch(available=available, installed=installed)]
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)

    root.update(directory=install_directory, force=True)

    refreshed = SkillRepository().find("sample-skill", directory=install_directory)
    assert refreshed is not None
    assert refreshed.package_version == "1.2.4"
    output = capsys.readouterr().out
    assert "sample-skill: sample-pkg 1.2.3 -> 1.2.4" in output
    assert "Updated sample-skill to 1.2.4" in output


def test_update_without_force_only_previews_changes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = FakeRepository()
    installed = repository.install(
        make_skill(
            tmp_path,
            package_name="sample-pkg",
            package_version="1.2.3",
            skill_name="sample-skill",
            body="Original body.\n",
        ),
        directory=install_directory,
    )
    available = make_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.4",
        skill_name="sample-skill",
        body="Updated body.\n",
    )
    repository.matches = [SkillMatch(available=available, installed=installed)]
    monkeypatch.setattr(root, "SkillRepository", lambda: repository)

    root.update(directory=install_directory)

    refreshed = SkillRepository().find("sample-skill", directory=install_directory)
    assert refreshed is not None
    assert refreshed.package_version == "1.2.3"
    output = capsys.readouterr().out
    assert "sample-skill: sample-pkg 1.2.3 -> 1.2.4" in output
    assert "Run with --force to apply these updates" in output


def make_skill(
    root_path: Path,
    *,
    package_name: str,
    package_version: str,
    skill_name: str,
    body: str = "Use this skill.\n",
) -> Skill:
    source_directory = root_path / package_name / ".agents" / "skills" / skill_name
    write_file(
        source_directory / "SKILL.md",
        "\n".join(
            [
                "---",
                f"name: {skill_name}",
                "description: Example dependency skill.",
                "---",
                body.rstrip("\n"),
                "",
            ]
        ),
    )
    write_file(source_directory / "scripts" / "extract.py", "print('sample')\n")
    return Skill.from_dir(
        source_directory,
        source="dependency",
        package_name=package_name,
        package_version=package_version,
    )


def write_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


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


class FakeGitHubFetcher:
    def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
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

    def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        del location
        if str(path).endswith("SKILL.md"):
            return GitHubFileBlob(
                path=path,
                content="---\nname: skillsmp-skill\ndescription: Updated.\n---\nUpdated body\n",
                size=61,
            )
        return GitHubFileBlob(
            path=path,
            content="print('updated')\n",
            size=17,
        )


class FakePinnedGitHubFetcher:
    def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        del location
        return [
            GitHubContentItem(
                type="file",
                name="SKILL.md",
                path=current_path / "SKILL.md",
                commit_sha="0123456789abcdef0123456789abcdef01234567",
            )
        ]

    def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        del location
        return GitHubFileBlob(
            path=path,
            content="---\nname: skillsmp-skill\ndescription: From SkillsMP.\n---\nBody\n",
            size=58,
            commit_sha="0123456789abcdef0123456789abcdef01234567",
        )


class FakeRepository:
    def __init__(self, matches: list[SkillMatch] | None = None) -> None:
        self.delegate = SkillRepository()
        self.matches = matches or []
        self.available_skill: Skill | None = None

    def scan_project(self, **kwargs: object) -> list[SkillMatch]:
        del kwargs
        return self.matches

    def list(self, directory: Path) -> list[Skill]:
        return self.delegate.list(directory)

    def install(self, skill: Skill, **kwargs: object) -> Skill:
        return self.delegate.install(skill, **kwargs)

    def remove(self, name: str, *, directory: Path) -> Skill:
        return self.delegate.remove(name, directory=directory)

    def dependency_updates(self, **kwargs: object) -> list[SkillMatch]:
        del kwargs
        return self.matches

    def available_dependency_skill(
        self, skill: Skill, **kwargs: object
    ) -> Skill | None:
        del skill, kwargs
        return self.available_skill
