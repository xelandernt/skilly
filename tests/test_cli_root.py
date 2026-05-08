from pathlib import Path, PurePosixPath

import pytest

from skilly.cli import root
from skilly.skills import (
    DiscoveredSkill,
    GitHubContentItem,
    GitHubFileBlob,
    GitHubSkillLocation,
    ManagedSkills,
    SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY,
    SKILLY_SOURCE_DEPENDENCY,
    Skill,
)


def test_scan_installs_selected_dependency_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    discovered_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
    )
    monkeypatch.setattr(
        root,
        "get_project_discovered_skills",
        lambda **kwargs: [discovered_skill],
    )
    monkeypatch.setattr(
        root,
        "questionary",
        _QuestionaryModule(["sample-skill [sample-pkg==1.2.3]"]),
    )

    install_directory = tmp_path / ".agents" / "skills"
    root.scan(directory=install_directory)

    installed_skill = ManagedSkills().find_installed_skill(
        "sample-skill",
        directory=install_directory,
    )
    assert installed_skill is not None
    assert installed_skill.source == SKILLY_SOURCE_DEPENDENCY
    assert installed_skill.dependency_package_name == "sample-pkg"
    assert installed_skill.dependency_package_version == "1.2.3"
    assert (install_directory / "sample-skill" / "scripts" / "extract.py").read_text(
        encoding="utf-8"
    ) == "print('sample')\n"
    assert "Installed sample-skill" in capsys.readouterr().out


def test_scan_exit_choice_skips_install(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    discovered_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
    )
    monkeypatch.setattr(
        root,
        "get_project_discovered_skills",
        lambda **kwargs: [discovered_skill],
    )
    monkeypatch.setattr(root, "questionary", _QuestionaryModule(["exit"]))

    install_directory = tmp_path / ".agents" / "skills"
    root.scan(directory=install_directory)

    assert not (install_directory / "sample-skill").exists()


def test_list_removes_selected_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    _write_file(
        install_directory / "dependency-skill" / "SKILL.md",
        """---
name: dependency-skill
description: From a dependency.
metadata:
  skilly-managed-by: skilly
  skilly-source: dependency
  skilly-package-name: dep-pkg
  skilly-package-version: 1.2.3
---
Body
""",
    )
    monkeypatch.setattr(
        root,
        "questionary",
        _QuestionaryModule(
            [
                "dependency-skill: dependency-skill [dependency] (dep-pkg==1.2.3)",
                "remove",
            ]
        ),
    )

    root.list(directory=install_directory)

    assert not (install_directory / "dependency-skill").exists()
    assert "Removed dependency-skill" in capsys.readouterr().out


def test_list_updates_dependency_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    original_skill = _make_discovered_skill(
        tmp_path,
        package_name="dep-pkg",
        package_version="1.2.3",
        skill_name="dependency-skill",
        body="Original body.\n",
    )
    ManagedSkills().install_discovered_skill(
        original_skill, directory=install_directory
    )
    updated_skill = _make_discovered_skill(
        tmp_path,
        package_name="dep-pkg",
        package_version="1.2.4",
        skill_name="dependency-skill",
        body="Updated body.\n",
    )
    monkeypatch.setattr(root, "get_project_discovered_skills", lambda: [updated_skill])
    monkeypatch.setattr(
        root,
        "questionary",
        _QuestionaryModule(
            [
                "dependency-skill: dependency-skill [dependency] (dep-pkg==1.2.3)",
                "update",
                "exit",
            ]
        ),
    )

    root.list(directory=install_directory)

    installed_skill = ManagedSkills().find_installed_skill(
        "dependency-skill",
        directory=install_directory,
    )
    assert installed_skill is not None
    assert installed_skill.dependency_package_version == "1.2.4"
    assert "Updated dependency-skill to 1.2.4" in capsys.readouterr().out


def test_list_updates_skillsmp_skill(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    _write_file(
        install_directory / "skillsmp-skill" / "SKILL.md",
        """---
name: skillsmp-skill
description: From SkillsMP.
metadata:
  skilly-managed-by: skilly
  skilly-source: skillsmp
  skilly-skillsmp-id: skill-1
  skilly-github-url: https://github.com/example/project/tree/main/.agents/skills/skillsmp-skill
---
Body
""",
    )
    monkeypatch.setattr(
        root,
        "questionary",
        _QuestionaryModule(
            [
                "skillsmp-skill: skillsmp-skill [skillsmp] (id=skill-1)",
                "update",
                "exit",
            ]
        ),
    )
    monkeypatch.setattr(root, "SkillsMp", lambda: _FakeGitHubFetcher())

    root.list(directory=install_directory)

    assert (install_directory / "skillsmp-skill" / "scripts" / "updated.py").read_text(
        encoding="utf-8"
    ) == "print('updated')\n"
    assert "Updated skillsmp-skill with 2 files" in capsys.readouterr().out


def test_update_force_updates_outdated_dependency_skills(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    original_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
        body="Original body.\n",
    )
    ManagedSkills().install_discovered_skill(
        original_skill, directory=install_directory
    )

    updated_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.4",
        skill_name="sample-skill",
        body="Updated body.\n",
    )
    monkeypatch.setattr(root, "get_project_discovered_skills", lambda: [updated_skill])

    root.update(directory=install_directory, force=True)

    installed_skill = ManagedSkills().find_installed_skill(
        "sample-skill",
        directory=install_directory,
    )
    assert installed_skill is not None
    assert installed_skill.dependency_package_version == "1.2.4"
    assert (install_directory / "sample-skill" / "SKILL.md").read_text(
        encoding="utf-8"
    ).find(f"{SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY}: 1.2.4") != -1
    output = capsys.readouterr().out
    assert "sample-skill: sample-pkg 1.2.3 -> 1.2.4" in output
    assert "Updated sample-skill to 1.2.4" in output


def test_update_without_force_only_previews_changes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    original_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.3",
        skill_name="sample-skill",
        body="Original body.\n",
    )
    ManagedSkills().install_discovered_skill(
        original_skill, directory=install_directory
    )

    updated_skill = _make_discovered_skill(
        tmp_path,
        package_name="sample-pkg",
        package_version="1.2.4",
        skill_name="sample-skill",
        body="Updated body.\n",
    )
    monkeypatch.setattr(root, "get_project_discovered_skills", lambda: [updated_skill])

    root.update(directory=install_directory)

    installed_skill = ManagedSkills().find_installed_skill(
        "sample-skill",
        directory=install_directory,
    )
    assert installed_skill is not None
    assert installed_skill.dependency_package_version == "1.2.3"
    output = capsys.readouterr().out
    assert "sample-skill: sample-pkg 1.2.3 -> 1.2.4" in output
    assert "Run with --force to apply these updates" in output


def _make_discovered_skill(
    root_path: Path,
    *,
    package_name: str,
    package_version: str,
    skill_name: str,
    body: str = "Use this skill.\n",
) -> DiscoveredSkill:
    source_directory = root_path / package_name / ".agents" / "skills" / skill_name
    _write_file(
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
    _write_file(source_directory / "scripts" / "extract.py", "print('sample')\n")
    return DiscoveredSkill(
        package_name=package_name,
        package_version=package_version,
        skill=Skill.from_dir(source_directory),
    )


def _write_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


class _QuestionaryPrompt:
    def __init__(self, response: list[str] | str) -> None:
        self._response = response

    def ask(self) -> list[str] | str:
        return self._response


class _QuestionaryModule:
    def __init__(self, responses: list[str] | list[list[str]]) -> None:
        self._responses = list(responses)

    def _next_response(self) -> list[str] | str:
        if not self._responses:
            raise AssertionError("No more questionary responses configured")
        return self._responses.pop(0)

    def checkbox(self, *args: object, **kwargs: object) -> _QuestionaryPrompt:
        del args, kwargs
        return _QuestionaryPrompt(self._next_response())

    def select(self, *args: object, **kwargs: object) -> _QuestionaryPrompt:
        del args, kwargs
        return _QuestionaryPrompt(self._next_response())


class _FakeGitHubFetcher:
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
                content=(
                    b"---\nname: skillsmp-skill\ndescription: Updated.\n---\nUpdated body\n"
                ),
                size=61,
            )
        return GitHubFileBlob(
            path=path,
            content=b"print('updated')\n",
            size=17,
        )
