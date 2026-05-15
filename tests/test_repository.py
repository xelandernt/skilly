from pathlib import Path

from skilly.repository import SkillRepository
from skilly.skills import Skill


def test_repository_scan_project_detects_updatable_dependency_skill(
    tmp_path: Path,
) -> None:
    pyproject_toml = tmp_path / "pyproject.toml"
    pyproject_toml.write_text(
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
    repository = SkillRepository()
    installed = repository.install(
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

    matches = repository.scan_project(
        directory=install_directory,
        pyproject_toml_path=pyproject_toml,
        venv_path=venv_path,
    )

    assert len(matches) == 1
    assert matches[0].installed == installed
    assert matches[0].available.package_version == "1.2.4"
    assert matches[0].status.value == "updatable"


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
