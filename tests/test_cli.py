import io
import os
import zipfile
from importlib.metadata import version
from pathlib import Path
import subprocess

from skilly._cli import run_cli
from skilly.repository import SkillRepository
from skilly.skills import Skill, resolve_skills_directory
from helpers import make_venv, write_distribution, write_skill

REPO_ROOT = Path(__file__).resolve().parents[1]


def run_native_cli(
    *args: str, cwd: Path | None = None, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", "run", "--quiet", "--", *args],
        cwd=cwd or REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


def test_run_cli_root_help_describes_commands(capfd) -> None:
    exit_code = run_cli(["--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "-V, --version" in output
    assert (
        "Scan dependency-provided skills from pyproject.toml/.venv and package.json/node_modules"
        in output
    )
    assert "Download one or more skills from a GitHub repository URL" in output
    assert "Browse, update, or remove installed skills" in output


def test_run_cli_version_reflects_package_version(capfd) -> None:
    exit_code = run_cli(["--version"])

    assert exit_code == 0
    assert capfd.readouterr().out == f"skilly {version('skilly')}\n"


def test_run_cli_scan_help_describes_options(capfd) -> None:
    exit_code = run_cli(["scan", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert (
        "Scan dependency-provided skills from pyproject.toml/.venv and package.json/node_modules"
        in output
    )
    assert "Directory where skilly installs managed skills" in output
    assert "Ignore [project].dependencies while scanning" in output
    assert "Include only the named [dependency-groups] entry" in output
    assert "Exclude the named [project.optional-dependencies] extra" in output


def test_run_cli_scan_rejects_conflicting_named_filters(capfd) -> None:
    exit_code = run_cli(["scan", "--group", "dev", "--exclude-group", "docs"])

    assert exit_code == 1
    assert "Include and exclude filters cannot be combined" in capfd.readouterr().err


def test_run_cli_scan_accepts_multiple_extra_flags(capfd) -> None:
    exit_code = run_cli(
        [
            "scan",
            "--extra",
            "dev",
            "--extra",
            "docs",
            "--directory",
            "/tmp/skilly-test-multi-extra",
        ]
    )

    assert exit_code == 0
    assert (
        "Include and exclude filters cannot be combined" not in capfd.readouterr().err
    )


def test_run_cli_scan_accepts_multiple_group_flags(capfd) -> None:
    exit_code = run_cli(
        [
            "scan",
            "--group",
            "dev",
            "--group",
            "test",
            "--directory",
            "/tmp/skilly-test-multi-group",
        ]
    )

    assert exit_code == 0
    assert (
        "Include and exclude filters cannot be combined" not in capfd.readouterr().err
    )


def test_run_cli_scan_accepts_multiple_exclude_extra_flags(capfd) -> None:
    exit_code = run_cli(
        [
            "scan",
            "--exclude-extra",
            "docs",
            "--exclude-extra",
            "lint",
            "--directory",
            "/tmp/skilly-test-multi-exclude-extra",
        ]
    )

    assert exit_code == 0
    assert (
        "Include and exclude filters cannot be combined" not in capfd.readouterr().err
    )


def test_run_cli_scan_accepts_multiple_exclude_group_flags(capfd) -> None:
    exit_code = run_cli(
        [
            "scan",
            "--exclude-group",
            "dev",
            "--exclude-group",
            "test",
            "--directory",
            "/tmp/skilly-test-multi-exclude-group",
        ]
    )

    assert exit_code == 0
    assert (
        "Include and exclude filters cannot be combined" not in capfd.readouterr().err
    )


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


def test_run_cli_create_help_describes_options(capfd) -> None:
    exit_code = run_cli(["create", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "Create a specification-compliant skill" in output
    assert "--description" in output
    assert "--instructions" in output
    assert "--with-references" in output


def test_run_cli_create_non_interactive_creates_skill(tmp_path: Path, capfd) -> None:
    exit_code = run_cli(
        [
            "create",
            "sample-skill",
            "--description",
            "Use when a sample skill is needed.",
            "--instructions",
            "# Instructions\n\nFollow the sample procedure.",
            "--metadata",
            "author=example",
            "--with-references",
            "--directory",
            str(tmp_path),
        ]
    )

    assert exit_code == 0
    skill = Skill.from_dir(tmp_path / "sample-skill")
    assert skill.description == "Use when a sample skill is needed."
    assert skill.metadata["author"] == "example"
    assert (tmp_path / "sample-skill/references").is_dir()
    assert "Created sample-skill" in capfd.readouterr().out


def test_run_cli_create_rejects_invalid_name_without_writing(
    tmp_path: Path, capfd
) -> None:
    exit_code = run_cli(
        [
            "create",
            "../unsafe",
            "--description",
            "Use when testing invalid input.",
            "--directory",
            str(tmp_path),
        ]
    )

    assert exit_code == 1
    assert not (tmp_path.parent / "unsafe").exists()
    assert "invalid skill name" in capfd.readouterr().err


def test_run_cli_list_prints_plain_output_without_terminal(
    tmp_path: Path, capfd
) -> None:
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=tmp_path,
    )

    exit_code = run_cli(["list", "--directory", str(tmp_path)])

    assert exit_code == 0
    assert "sample-skill" in capfd.readouterr().out


def test_run_cli_list_reports_invalid_child_directory_without_failing(
    tmp_path: Path, capfd
) -> None:
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=tmp_path,
    )
    invalid_dir = tmp_path / ".system"
    invalid_dir.mkdir()
    (invalid_dir / "SKILL.md").write_text("not valid frontmatter\n", encoding="utf-8")

    exit_code = run_cli(["list", "--directory", str(tmp_path)])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "sample-skill" in output
    assert ".system: invalid [invalid]" in output


def test_run_cli_list_reports_empty_directory(tmp_path: Path, capfd) -> None:
    exit_code = run_cli(["list", "--directory", str(tmp_path)])

    assert exit_code == 0
    assert f"No skills found in directory {tmp_path}" in capfd.readouterr().out


def test_native_cli_root_help_describes_commands() -> None:
    result = run_native_cli("--help")

    assert result.returncode == 0
    assert "-V, --version" in result.stdout
    assert (
        "Scan dependency-provided skills from pyproject.toml/.venv and package.json/node_modules"
        in result.stdout
    )
    assert "Download one or more skills from a GitHub repository URL" in result.stdout
    assert "Browse, update, or remove installed skills" in result.stdout


def test_native_cli_version_reflects_package_version() -> None:
    result = run_native_cli("--version")

    assert result.returncode == 0
    assert result.stdout == f"skilly {version('skilly')}\n"


def test_native_cli_list_prints_plain_output_without_terminal(tmp_path: Path) -> None:
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=tmp_path,
    )

    result = run_native_cli("list", "--directory", str(tmp_path))

    assert result.returncode == 0
    assert "sample-skill" in result.stdout


def test_native_cli_list_reports_invalid_child_directory_without_failing(
    tmp_path: Path,
) -> None:
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=tmp_path,
    )
    invalid_dir = tmp_path / ".system"
    invalid_dir.mkdir()
    (invalid_dir / "SKILL.md").write_text("not valid frontmatter\n", encoding="utf-8")

    result = run_native_cli("list", "--directory", str(tmp_path))

    assert result.returncode == 0
    assert "sample-skill" in result.stdout
    assert ".system: invalid [invalid]" in result.stdout


def test_native_cli_list_reports_empty_directory(tmp_path: Path) -> None:
    result = run_native_cli("list", "--directory", str(tmp_path))

    assert result.returncode == 0
    assert f"No skills found in directory {tmp_path}" in result.stdout


def test_native_cli_uses_environment_default_directory(tmp_path: Path) -> None:
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=tmp_path,
    )

    env = {**os.environ, "SKILLY_DEFAULT_DIRECTORY": str(tmp_path)}
    result = run_native_cli("list", env=env)

    assert result.returncode == 0
    assert "sample-skill" in result.stdout


def test_native_cli_explicit_directory_overrides_environment_default(
    tmp_path: Path,
) -> None:
    explicit_directory = tmp_path / "explicit-skills"
    SkillRepository().install(
        Skill("sample-skill", "Installed skill."),
        directory=explicit_directory,
    )

    env = {**os.environ, "SKILLY_DEFAULT_DIRECTORY": str(tmp_path)}
    result = run_native_cli("list", "--directory", str(explicit_directory), env=env)

    assert result.returncode == 0
    assert "sample-skill" in result.stdout


def test_resolve_skills_directory_supports_local_agent_flavors() -> None:
    assert resolve_skills_directory() == Path(".agents/skills")
    assert resolve_skills_directory("claude") == Path(".claude/skills")
    assert resolve_skills_directory("codex") == Path(".codex/skills")
    assert resolve_skills_directory("copilot") == Path(".github/skills")


def test_resolve_skills_directory_uses_environment_default(monkeypatch) -> None:
    monkeypatch.setenv("SKILLY_DEFAULT_DIRECTORY", "~/custom-skills")

    assert resolve_skills_directory() == Path.home() / "custom-skills"
    assert resolve_skills_directory("claude") == Path(".claude/skills")


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


# ── configure command tests ────────────────────────────────────────────


def test_configure_help_shows_flags(capfd) -> None:
    exit_code = run_cli(["configure", "--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "--show" in output
    assert "--reset" in output
    assert "--add-global" in output
    assert "--remove-global" in output
    assert "--add-local" in output
    assert "--remove-local" in output


def test_configure_show_prints_toml(capfd, tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    exit_code = run_cli(["configure", "--show"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "default_directory" in output
    assert "[global]" in output
    assert "directories" in output
    assert "~/.agents/skills" in output
    assert "[local]" in output
    assert ".agents/skills" in output


def test_configure_add_global_and_show(capfd, tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    exit_code = run_cli(["configure", "--add-global", "/opt/test-skills", "--show"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "/opt/test-skills" in output


def test_configure_add_local_and_show(capfd, tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    exit_code = run_cli(["configure", "--add-local", ".custom/skills", "--show"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert ".custom/skills" in output


def test_configure_add_local_rejects_absolute(
    capfd, tmp_path: Path, monkeypatch
) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    exit_code = run_cli(["configure", "--add-local", "/bad/path"])

    assert exit_code == 1
    assert "relative path" in capfd.readouterr().err.lower()


def test_configure_remove_global_dir(capfd, tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    # Remove the default agents global dir
    exit_code = run_cli(["configure", "--remove-global", "~/.agents/skills", "--show"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "~/.agents/skills" not in output


def test_configure_reset_restores_defaults(capfd, tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    # First, remove all directories
    run_cli(["configure", "--remove-global", "~/.agents/skills"])
    run_cli(["configure", "--remove-local", ".agents/skills"])

    # Reset
    exit_code = run_cli(["configure", "--reset"])
    assert exit_code == 0

    # Now show should have defaults back
    exit_code = run_cli(["configure", "--show"])
    assert exit_code == 0
    output = capfd.readouterr().out
    assert "~/.agents/skills" in output
    assert ".agents/skills" in output


def test_configure_list_flag_does_not_conflict_with_modify(
    capfd,
    tmp_path: Path,
    monkeypatch,
) -> None:
    monkeypatch.setenv("HOME", str(tmp_path))
    exit_code = run_cli(["configure", "--show", "--add-global", "/opt/x"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "/opt/x" in output


def test_root_help_lists_configure(capfd) -> None:
    exit_code = run_cli(["--help"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "configure" in output
    assert "Configure which directories skilly manages" in output


# ── Node dependency CLI tests ────────────────────────────────────────────


def _write_node_cli_fixture(
    root: Path,
    package_name: str,
    version: str,
    skill_name: str,
    dep_section: str = "dependencies",
) -> tuple[Path, Path]:
    package_json = root / "package.json"
    package_json.write_text(
        f'{{"{dep_section}": {{"{package_name}": "{version}"}}}}',
        encoding="utf-8",
    )
    node_modules = root / "node_modules"
    skill_dir = node_modules / package_name / "skills" / skill_name
    skill_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text(
        f"""---
name: {skill_name}
description: Node dependency skill.
---
Body
""",
        encoding="utf-8",
    )
    pkg_dir = node_modules / package_name
    (pkg_dir / "package.json").write_text(
        f'{{"name": "{package_name}", "version": "{version}"}}',
        encoding="utf-8",
    )
    return package_json, node_modules


def test_run_cli_scan_detects_node_skills_in_non_interactive_mode(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)
    _write_node_cli_fixture(tmp_path, "node-pkg", "1.0.0", "node-skill")

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "node-skill" in output
    assert "node-pkg@1.0.0" in output
    assert "node:dependencies" in output


def test_run_cli_scan_shows_node_dev_dependency_origin(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)
    _write_node_cli_fixture(
        tmp_path, "dev-pkg", "2.0.0", "dev-skill", dep_section="devDependencies"
    )

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "dev-skill" in output
    assert "node:devDependencies" in output


def test_run_cli_scan_mixed_project_shows_both_ecosystems(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)
    # Python
    pyproject_toml = tmp_path / "pyproject.toml"
    pyproject_toml.write_text(
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["sample-pkg==1.0.0"]
""".strip()
        + "\n",
        encoding="utf-8",
    )
    _, site_packages = make_venv(tmp_path)
    write_skill(
        site_packages / "sample_pkg/.agents/skills/python-skill/SKILL.md",
        """---
name: python-skill
description: Python skill.
---
Body
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.0.0",
        record_rows=["sample_pkg/.agents/skills/python-skill/SKILL.md,,"],
    )
    # Node
    _write_node_cli_fixture(tmp_path, "node-pkg", "2.0.0", "node-skill")

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "python-skill" in output
    assert "sample-pkg==1.0.0" in output
    assert "python:project" in output
    assert "node-skill" in output
    assert "node-pkg@2.0.0" in output
    assert "node:dependencies" in output


def test_run_cli_scan_no_skills_found_without_manifests(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "No dependency skills found" in output


# --- Maven CLI tests ---


def _write_maven_cli_fixture(
    root: Path,
    group_id: str,
    artifact_id: str,
    version: str,
    skill_name: str,
) -> None:
    """Create pom.xml and fake Maven JAR for CLI scan tests."""
    pom = root / "pom.xml"
    pom.write_text(
        f"""<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <groupId>com.example</groupId>
    <artifactId>demo</artifactId>
    <version>1.0.0</version>
    <dependencies>
        <dependency>
            <groupId>{group_id}</groupId>
            <artifactId>{artifact_id}</artifactId>
            <version>{version}</version>
        </dependency>
    </dependencies>
</project>""",
        encoding="utf-8",
    )

    jar_dir = (
        Path.home()
        / ".m2"
        / "repository"
        / group_id.replace(".", "/")
        / artifact_id
        / version
    )
    jar_dir.mkdir(parents=True, exist_ok=True)
    jar_path = jar_dir / f"{artifact_id}-{version}.jar"
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr(
            f"skills/{skill_name}/SKILL.md",
            f"---\nname: {skill_name}\ndescription: CLI Maven skill.\n---\nBody\n",
        )
    jar_path.write_bytes(buf.getvalue())


def test_run_cli_scan_detects_maven_skills_in_non_interactive_mode(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)
    _write_maven_cli_fixture(
        tmp_path, "com.example", "maven-cli-lib", "1.0.0", "maven-cli-skill"
    )

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "maven-cli-skill" in output
    assert "com.example:maven-cli-lib" in output
    assert "maven:compile" in output


def test_run_cli_scan_maven_and_python_together(
    tmp_path: Path, monkeypatch, capfd
) -> None:
    monkeypatch.chdir(tmp_path)
    # Python
    pyproject_toml = tmp_path / "pyproject.toml"
    pyproject_toml.write_text(
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["sample-pkg==1.0.0"]
""".strip()
        + "\n",
        encoding="utf-8",
    )
    _, site_packages = make_venv(tmp_path)
    write_skill(
        site_packages / "sample_pkg/.agents/skills/python-skill/SKILL.md",
        """---
name: python-skill
description: Python skill.
---
Body
""",
    )
    write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.0.0",
        record_rows=["sample_pkg/.agents/skills/python-skill/SKILL.md,,"],
    )
    # Maven
    _write_maven_cli_fixture(
        tmp_path, "com.example", "maven-lib", "1.0.0", "maven-skill"
    )

    exit_code = run_cli(["scan"])

    assert exit_code == 0
    output = capfd.readouterr().out
    assert "python-skill" in output
    assert "maven-skill" in output
    assert "python:project" in output
    assert "maven:compile" in output
