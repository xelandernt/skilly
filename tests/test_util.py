from pathlib import Path

from skilly import get_skills_from_directory
from skilly.repository import SkillRepository
from skilly.skills import Skill


def test_get_skills_from_directory_lists_installed_skills(tmp_path: Path) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    repository = SkillRepository()
    repository.install(
        Skill.from_text(
            """---
name: sample-skill
description: Installed skill.
---
Body
""",
            path=tmp_path / "source" / "SKILL.md",
            source="github",
            github_url="https://github.com/example/project/tree/main/skills/sample-skill",
        ),
        directory=install_directory,
    )

    skills = get_skills_from_directory(install_directory)

    assert [skill.directory_name for skill in skills] == ["sample-skill"]
    assert skills[0].name == "sample-skill"
