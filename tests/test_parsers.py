from pathlib import Path, PurePosixPath

import pytest

from skilly.repository import SkillRepository
from skilly.skills import Skill, SkillResource, discover_venv_skills
from helpers import make_venv, write_distribution, write_skill


def test_skill_from_text_returns_skill_object() -> None:
    skill = Skill.from_text(
        """---
name: parser-skill
description: Parse structured input. Use when the task mentions parsing.
metadata:
  owner: docs
---
Use the parser.
""",
        path=Path("/workspace/parser-skill/SKILL.md"),
    )

    assert skill.name == "parser-skill"
    assert skill.description.startswith("Parse structured input.")
    assert skill.metadata == {"owner": "docs"}
    assert skill.content == "Use the parser."
    assert skill.path == Path("/workspace/parser-skill")
    assert skill.skill_markdown_path == Path("/workspace/parser-skill/SKILL.md")


def test_skill_from_text_allows_in_memory_parsing_without_path() -> None:
    skill = Skill.from_text(
        """---
name: in-memory-skill
description: Use when parsing a skill from memory.
---
In-memory instructions.
"""
    )

    assert skill.name == "in-memory-skill"
    assert skill.path is None
    assert skill.skill_markdown_path is None
    assert skill.directory is None
    assert skill.directory_name == "in-memory-skill"
    assert skill.package_reference() is None


def test_skill_constructor_accepts_optional_path_and_installs(tmp_path: Path) -> None:
    skill = Skill(
        name="generated-skill",
        description="Use when creating a skill in memory.",
        path=None,
        content="Generated instructions.\n",
    )

    installed = skill.install_to(tmp_path, skill_name="saved-skill")

    assert skill.path is None
    assert skill.directory is None
    assert skill.directory_name == "generated-skill"
    assert installed.path == (tmp_path / "saved-skill").resolve()
    assert (
        installed.skill_markdown_path
        == (tmp_path / "saved-skill" / "SKILL.md").resolve()
    )
    assert installed.content == "Generated instructions."


def test_skill_from_text_ignores_unknown_fields() -> None:
    skill = Skill.from_text(
        """---
name: relaxed-skill
description: Use when parsing should be permissive.
trigger: pdf
---
Relaxed instructions.
""",
        path=Path("/workspace/relaxed-skill"),
    )

    assert skill.name == "relaxed-skill"
    assert skill.description == "Use when parsing should be permissive."
    assert skill.content == "Relaxed instructions."


def test_skill_from_text_parses_literal_block_scalars() -> None:
    skill = Skill.from_text(
        """---
name: block-skill
description: |
  First line.
  Second line.
metadata:
  owner: docs
---
Block instructions.
""",
        path=Path("/workspace/block-skill"),
    )

    assert skill.description == "First line.\nSecond line."
    assert skill.metadata == {"owner": "docs"}


def test_skill_from_text_accepts_unquoted_colons_in_scalar_values() -> None:
    skill = Skill.from_text(
        """---
name: relaxed-colon-skill
description: Padroniza composição FastAPI: app factory, lifespan e dependency injection.
metadata:
  owner: docs: platform
---
Relaxed instructions.
""",
        path=Path("/workspace/relaxed-colon-skill"),
    )

    assert skill.name == "relaxed-colon-skill"
    assert skill.description == (
        "Padroniza composição FastAPI: app factory, lifespan e dependency injection."
    )
    assert skill.metadata == {"owner": "docs: platform"}
    assert skill.content == "Relaxed instructions."


def test_skill_from_text_supports_nested_yaml_metadata() -> None:
    skill = Skill.from_text(
        """---
name: binance-agentic-wallet
description: |
  Use when the user mentions connect/disconnect wallet, sign in, sign out, web3 wallet.
metadata:
  author: binance-web3-team
  version: "1.0.1"
  requiredCliVersion: "1.0.9"
  openclaw:
    requires:
      bins:
        - baw
    install:
      - kind: node
        package: '@binance/agentic-wallet'
        bins: [baw]
---
Use this skill carefully.
""",
        path=Path("/workspace/binance-agentic-wallet"),
    )

    assert skill.name == "binance-agentic-wallet"
    assert skill.description == (
        "Use when the user mentions connect/disconnect wallet, sign in, sign out, "
        "web3 wallet."
    )
    assert skill.metadata == {
        "author": "binance-web3-team",
        "version": "1.0.1",
        "requiredCliVersion": "1.0.9",
    }
    assert skill.content == "Use this skill carefully."


def test_skill_from_dir_reads_skill_md(tmp_path: Path) -> None:
    skill_dir = tmp_path / "folder-skill"
    write_skill(
        skill_dir / "SKILL.md",
        """---
name: folder-skill
description: Use when reading a skill from a directory.
---
Directory based instructions.
""",
    )

    skill = Skill.from_dir(skill_dir)

    assert skill.name == "folder-skill"
    assert skill.path == skill_dir.resolve()
    assert skill.skill_markdown_path == (skill_dir / "SKILL.md").resolve()
    assert skill.content == "Directory based instructions."


def test_skill_from_dir_collects_bundled_resources(tmp_path: Path) -> None:
    skill_dir = tmp_path / "folder-skill"
    write_skill(
        skill_dir / "SKILL.md",
        """---
name: folder-skill
description: Use when reading a skill from a directory.
---
Directory based instructions.
""",
    )
    write_skill(skill_dir / "scripts/extract.py", "print('extract')\n")
    write_skill(skill_dir / "references/REFERENCE.md", "# Reference\n")
    write_skill(skill_dir / "assets/template.txt", "template\n")
    write_skill(skill_dir / "tool-config.json", "{}\n")

    skill = Skill.from_dir(skill_dir)

    assert skill.directory == skill_dir.resolve()
    assert [resource.relative_path.as_posix() for resource in skill.resources] == [
        "assets/template.txt",
        "references/REFERENCE.md",
        "scripts/extract.py",
        "tool-config.json",
    ]
    assert [resource.kind for resource in skill.resources] == [
        "asset",
        "reference",
        "script",
        "other",
    ]
    assert [resource.content for resource in skill.resources] == [
        "template\n",
        "# Reference\n",
        "print('extract')\n",
        "{}\n",
    ]
    assert [resource.relative_path.as_posix() for resource in skill.scripts] == [
        "scripts/extract.py"
    ]
    assert [resource.relative_path.as_posix() for resource in skill.references] == [
        "references/REFERENCE.md"
    ]
    assert [resource.relative_path.as_posix() for resource in skill.assets] == [
        "assets/template.txt"
    ]


def test_discover_venv_returns_dependency_skills(tmp_path: Path) -> None:
    venv_path, site_packages = make_venv(tmp_path)
    skill_path = site_packages / "sample_pkg/.agents/skills/sample-skill/SKILL.md"
    write_skill(
        skill_path,
        """---
name: sample-skill
description: Parse PDFs and forms. Use when the task mentions PDFs, forms, or extraction.
license: Apache-2.0
compatibility: Requires Python 3.13+ and uv
metadata:
  author: example-org
  version: "1.0"
allowed-tools: Bash(git:*) Read
---
# Instructions
Use this skill carefully.
""",
    )
    write_skill(skill_path.parent / "scripts/extract.py", "print('sample')\n")
    write_skill(skill_path.parent / "references/REFERENCE.md", "# Sample reference\n")
    write_skill(skill_path.parent / "assets/form.json", "{}\n")
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.2.3",
        record_rows=["sample_pkg/.agents/skills/sample-skill/SKILL.md,,"],
    )

    skills = discover_venv_skills(venv_path)

    assert len(skills) == 1
    skill = skills[0]
    assert skill.package_name == "sample-pkg"
    assert skill.package_version == "1.2.3"
    assert skill.package_reference() == "sample-pkg==1.2.3"
    assert skill.is_dependency() is True
    assert skill.name == "sample-skill"
    assert skill.description.startswith("Parse PDFs and forms.")
    assert skill.license == "Apache-2.0"
    assert skill.compatibility == "Requires Python 3.13+ and uv"
    assert skill.metadata == {"author": "example-org", "version": "1.0"}
    assert skill.allowed_tools == "Bash(git:*) Read"
    assert skill.path == skill_path.parent.resolve()
    assert skill.skill_markdown_path == skill_path.resolve()
    assert "Use this skill carefully." in skill.content
    assert [resource.relative_path.as_posix() for resource in skill.resources] == [
        "assets/form.json",
        "references/REFERENCE.md",
        "scripts/extract.py",
    ]


def test_discover_venv_keeps_skills_with_unknown_fields(tmp_path: Path) -> None:
    venv_path, site_packages = make_venv(tmp_path)
    skill_path = site_packages / "broken_pkg/.agents/skills/broken-skill/SKILL.md"
    write_skill(
        skill_path,
        """---
name: broken-skill
description: This looks valid at first glance.
trigger: pdf
---
Invalid extra field.
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="broken-pkg",
        package_version="9.9.9",
        record_rows=["broken_pkg/.agents/skills/broken-skill/SKILL.md,,"],
    )

    skills = discover_venv_skills(venv_path)

    assert len(skills) == 1
    assert skills[0].name == "broken-skill"


def test_discover_venv_supports_windows_site_packages_and_record_paths(
    tmp_path: Path,
) -> None:
    venv_path, site_packages = make_venv(
        tmp_path, site_packages_relative=Path("Lib/site-packages")
    )
    skill_path = site_packages / "sample_pkg/.agents/skills/sample-skill/SKILL.md"
    write_skill(
        skill_path,
        """---
name: sample-skill
description: Parse PDFs and forms. Use when the task mentions PDFs, forms, or extraction.
---
Use this skill carefully.
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.2.3",
        record_rows=[r"sample_pkg\.agents\skills\sample-skill\SKILL.md,,"],
    )

    skills = discover_venv_skills(venv_path)

    assert [skill.name for skill in skills] == ["sample-skill"]
    assert skills[0].path == skill_path.parent.resolve()
    assert skills[0].skill_markdown_path == skill_path.resolve()


def test_installed_skill_state_comes_from_metadata(tmp_path: Path) -> None:
    skill_dir = tmp_path / "installed-skill"
    write_skill(
        skill_dir / "SKILL.md",
        """---
name: installed-skill
description: Installed skill.
metadata:
  skilly-managed-by: skilly
  skilly-source: skillsmp
  skilly-github-url: https://github.com/example/project/tree/main/.agents/skills/installed-skill
  skilly-github-commit-sha: 0123456789abcdef0123456789abcdef01234567
  skilly-skillsmp-id: skill-1
---
Body
""",
    )

    skill = Skill.from_dir(skill_dir)

    assert skill.is_installed() is True
    assert skill.is_skillsmp() is True
    assert skill.can_update() is True
    assert (
        skill.github_url
        == "https://github.com/example/project/tree/main/.agents/skills/installed-skill"
    )
    assert skill.github_commit_sha == "0123456789abcdef0123456789abcdef01234567"
    assert skill.skillsmp_id == "skill-1"


def test_install_rejects_paths_outside_skill_directory(tmp_path: Path) -> None:
    skill = Skill(
        "safe-skill",
        "Safe skill.",
        resources=[SkillResource(PurePosixPath("../escaped.txt"), "other", "escaped")],
    )

    with pytest.raises(RuntimeError, match="relative resource path"):
        skill.install_to(tmp_path)

    assert not (tmp_path / "escaped.txt").exists()
    assert not (tmp_path / "safe-skill").exists()


def test_repository_replace_removes_stale_resources(tmp_path: Path) -> None:
    repository = SkillRepository()
    original = Skill(
        "sample-skill",
        "Original skill.",
        resources=[
            SkillResource(PurePosixPath("references/stale.md"), "reference", "stale\n")
        ],
    )
    replacement = Skill("sample-skill", "Replacement skill.")

    repository.install(original, directory=tmp_path)
    repository.install(replacement, directory=tmp_path, replace=True)

    assert not (tmp_path / "sample-skill/references/stale.md").exists()
