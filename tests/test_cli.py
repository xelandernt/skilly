from pathlib import Path

from skilly._cli import run_cli
from skilly.repository import SkillRepository
from skilly.skills import Skill, resolve_skills_directory
from helpers import make_venv, write_distribution, write_skill


def test_run_cli_root_help_describes_commands(capfd) -> None:
    exit_code = run_cli(["--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Scan dependency-provided skills from pyproject.toml and .venv" in output
    assert "Download one or more skills from a GitHub repository URL" in output
    assert "Browse, update, or remove installed skills" in output


def test_run_cli_scan_help_describes_options(capfd) -> None:
    exit_code = run_cli(["scan", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Scan dependency-provided skills from pyproject.toml and .venv" in output
    assert "Directory where skilly installs managed skills" in output
    assert "Ignore [project].dependencies while scanning" in output
    assert "Ignore [dependency-groups] while scanning" in output
    assert "Ignore [project.optional-dependencies] while scanning" in output


def test_run_cli_skillsmp_search_help_describes_options(capfd) -> None:
    exit_code = run_cli(["skillsmp", "search", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Search SkillsMP and install a selected result" in output
    assert "Search query sent to SkillsMP" in output
    assert "Overwrite existing files when installing the selected skill" in output
    assert "--global" in output
    assert "--claude" in output


def test_run_cli_update_help_describes_options(capfd) -> None:
    exit_code = run_cli(["update", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Check installed skill updates in bulk and optionally apply them" in output
    assert "use `skilly list` to review or update one skill at a time" in output
    assert "--yes" in output
    assert "-y" in output
    assert (
        "GitHub token used when checking for updates to GitHub-backed skills" in output
    )


def test_resolve_skills_directory_supports_local_agent_flavors() -> None:
    assert resolve_skills_directory() == Path(".agents/skills")
    assert resolve_skills_directory("claude") == Path(".claude/skills")
    assert resolve_skills_directory("codex") == Path(".codex/skills")
    assert resolve_skills_directory("copilot") == Path(".github/skills")


def test_resolve_skills_directory_supports_global_agent_flavors() -> None:
    assert resolve_skills_directory(global_=True) == Path.home() / ".agents/skills"
    assert (
        resolve_skills_directory("claude", global_=True)
        == Path.home() / ".claude/skills"
    )
    assert (
        resolve_skills_directory("codex", global_=True) == Path.home() / ".codex/skills"
    )
    assert (
        resolve_skills_directory("copilot", global_=True)
        == Path.home() / ".copilot/skills"
    )


def test_run_cli_update_previews_dependency_skill_updates_by_default(
    tmp_path: Path,
    monkeypatch,
    capfd,
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("pyproject.toml").write_text(
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["sample-pkg==1.2.4"]
""".strip()
        + "\n",
        encoding="utf-8",
    )
    venv_path, site_packages = make_venv(tmp_path)
    skill_path = site_packages / "sample_pkg/.agents/skills/sample-skill/SKILL.md"
    write_skill(
        skill_path,
        """---
name: sample-skill
description: Dependency skill.
---
Body
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.2.4",
        record_rows=["sample_pkg/.agents/skills/sample-skill/SKILL.md,,"],
    )

    install_directory = tmp_path / ".agents" / "skills"
    SkillRepository().install(
        Skill.from_text(
            """---
name: sample-skill
description: Installed dependency skill.
---
Body
""",
            path=tmp_path / "source",
            source="dependency",
            package_name="sample-pkg",
            package_version="1.2.3",
        ),
        directory=install_directory,
    )

    exit_code = run_cli(["update"])

    assert exit_code == 0
    refreshed = SkillRepository().require("sample-skill", directory=install_directory)
    assert refreshed.package_version == "1.2.3"
    output = capfd.readouterr().out
    assert "Available skill updates:" in output
    assert "sample-skill [dependency]: sample-pkg 1.2.3 -> 1.2.4" in output
    assert "Use `skilly list` to review or apply updates one skill at a time." in output
    assert "Re-run with --yes to apply these updates" in output
    assert venv_path.exists()


def test_run_cli_update_yes_updates_dependency_skill(
    tmp_path: Path,
    monkeypatch,
    capfd,
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("pyproject.toml").write_text(
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["sample-pkg==1.2.4"]
""".strip()
        + "\n",
        encoding="utf-8",
    )
    venv_path, site_packages = make_venv(tmp_path)
    skill_path = site_packages / "sample_pkg/.agents/skills/sample-skill/SKILL.md"
    write_skill(
        skill_path,
        """---
name: sample-skill
description: Dependency skill.
---
Body
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.2.4",
        record_rows=["sample_pkg/.agents/skills/sample-skill/SKILL.md,,"],
    )

    install_directory = tmp_path / ".agents" / "skills"
    SkillRepository().install(
        Skill.from_text(
            """---
name: sample-skill
description: Installed dependency skill.
---
Body
""",
            path=tmp_path / "source",
            source="dependency",
            package_name="sample-pkg",
            package_version="1.2.3",
        ),
        directory=install_directory,
    )

    exit_code = run_cli(["update", "--yes"])

    assert exit_code == 0
    refreshed = SkillRepository().require("sample-skill", directory=install_directory)
    assert refreshed.package_version == "1.2.4"
    assert "Updated sample-skill to 1.2.4" in capfd.readouterr().out
    assert venv_path.exists()


def test_run_cli_remove_removes_installed_skill(tmp_path: Path, capfd) -> None:
    install_directory = tmp_path / ".agents" / "skills"
    SkillRepository().install(
        Skill.from_text(
            """---
name: removable-skill
description: Remove me.
---
Body
""",
            path=tmp_path / "source",
        ),
        directory=install_directory,
    )

    exit_code = run_cli(
        ["remove", "removable-skill", "--directory", str(install_directory)]
    )

    assert exit_code == 0
    assert not (install_directory / "removable-skill").exists()
    assert "Removed removable-skill" in capfd.readouterr().out
