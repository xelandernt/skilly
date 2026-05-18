from pathlib import Path

from skilly._cli import run_cli
from skilly.repository import SkillRepository
from skilly.skills import Skill


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
    assert "Include development dependencies while scanning" in output


def test_run_cli_skillsmp_search_help_describes_options(capfd) -> None:
    exit_code = run_cli(["skillsmp", "search", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Search SkillsMP and install a selected result" in output
    assert "Search query sent to SkillsMP" in output
    assert "Overwrite existing files when installing the selected skill" in output


def test_run_cli_update_force_updates_dependency_skill(
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

    exit_code = run_cli(["update", "--force"])

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


def make_venv(
    root: Path,
    *,
    site_packages_relative: Path = Path("lib/python3.13/site-packages"),
) -> tuple[Path, Path]:
    venv_path = root / ".venv"
    site_packages = venv_path / site_packages_relative
    site_packages.mkdir(parents=True)
    return venv_path, site_packages.resolve()


def write_distribution(
    *,
    site_packages: Path,
    package_name: str,
    package_version: str,
    record_rows: list[str],
) -> None:
    dist_info = site_packages / f"{package_name}-{package_version}.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "\n".join(
            [
                "Metadata-Version: 2.4",
                f"Name: {package_name}",
                f"Version: {package_version}",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (dist_info / "RECORD").write_text("\n".join(record_rows), encoding="utf-8")


def write_skill(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
