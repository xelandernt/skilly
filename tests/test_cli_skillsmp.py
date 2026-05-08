from pathlib import Path

import pytest

from skilly.cli import skillsmp as skillsmp_cli


def test_skillsmp_list_only_shows_skillsmp_installed_skills(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
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
    questionary = _QuestionaryModule(response="exit")
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

    skillsmp_cli.list(directory=install_directory)

    assert (
        capsys.readouterr().out.strip()
        == f"No SkillsMP-installed skills found in {install_directory.resolve()}"
    )


def _write_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


class _QuestionaryPrompt:
    def __init__(self, response: str) -> None:
        self._response = response

    def ask(self) -> str:
        return self._response


class _QuestionaryModule:
    def __init__(self, response: str) -> None:
        self._response = response
        self.select_calls: list[dict[str, object]] = []

    def select(self, message: str, **kwargs: object) -> _QuestionaryPrompt:
        self.select_calls.append({"message": message, **kwargs})
        return _QuestionaryPrompt(self._response)
