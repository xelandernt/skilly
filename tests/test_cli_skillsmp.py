from pathlib import Path

import pytest

from skilly.cli import skillsmp as skillsmp_cli
from skilly.repository import SkillRepository
from skilly.skills import Skill


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
    questionary = QuestionaryModule(response="exit")
    monkeypatch.setattr(skillsmp_cli, "questionary", questionary)

    skillsmp_cli.list(directory=install_directory)

    assert questionary.select_calls[0]["choices"] == [
        "skillsmp-skill: skillsmp-skill",
        "exit",
    ]


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


class QuestionaryPrompt:
    def __init__(self, response: str) -> None:
        self._response = response

    def ask(self) -> str:
        return self._response


class QuestionaryModule:
    def __init__(self, response: str) -> None:
        self._response = response
        self.select_calls: list[dict[str, object]] = []

    def select(self, message: str, **kwargs: object) -> QuestionaryPrompt:
        self.select_calls.append({"message": message, **kwargs})
        return QuestionaryPrompt(self._response)
