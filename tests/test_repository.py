from pathlib import Path

import pytest

from skilly.repository import ProjectSettings, SkillRepository
from skilly.skills import Skill
from helpers import make_venv, write_distribution, write_skill


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
        project=ProjectSettings(
            pyproject_toml_path=pyproject_toml, venv_path=venv_path
        ),
    )

    assert len(matches) == 1
    assert matches[0].installed == installed
    assert matches[0].available.package_version == "1.2.4"
    assert matches[0].status.value == "updatable"

    excluded = repository.scan_project(
        directory=install_directory,
        project=ProjectSettings(
            pyproject_toml_path=pyproject_toml,
            venv_path=venv_path,
            include_project_dependencies=False,
        ),
    )

    assert excluded == []


def test_repository_updates_include_dependency_skill(
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
    repository.install(
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

    updates = repository.updates(
        directory=install_directory,
        project=ProjectSettings(
            pyproject_toml_path=pyproject_toml, venv_path=venv_path
        ),
    )

    assert len(updates) == 1
    assert updates[0].installed.directory_name == "sample-skill"
    assert updates[0].available.package_version == "1.2.4"


def test_repository_scan_project_filters_named_groups_and_extras(
    tmp_path: Path,
) -> None:
    pyproject_toml = tmp_path / "pyproject.toml"
    pyproject_toml.write_text(
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["base-pkg==1.0.0", "shared-pkg==1.0.0"]

[project.optional-dependencies]
docs = ["docs-pkg==1.0.0", "shared-pkg==1.0.0"]

[dependency-groups]
dev = ["dev-pkg==1.0.0", "shared-pkg==1.0.0"]
""".strip()
        + "\n",
        encoding="utf-8",
    )

    venv_path, site_packages = make_venv(tmp_path)
    for package_name, skill_name in [
        ("base-pkg", "base-skill"),
        ("shared-pkg", "shared-skill"),
        ("dev-pkg", "dev-skill"),
        ("docs-pkg", "docs-skill"),
    ]:
        module_name = package_name.replace("-", "_")
        skill_path = (
            site_packages / f"{module_name}/.agents/skills/{skill_name}/SKILL.md"
        )
        write_skill(
            skill_path,
            f"""---
name: {skill_name}
description: Dependency skill.
---
Body
""",
        )
        write_distribution(
            site_packages=site_packages,
            package_name=package_name,
            package_version="1.0.0",
            record_rows=[f"{module_name}/.agents/skills/{skill_name}/SKILL.md,,"],
        )

    matches = SkillRepository().scan_project(
        project=ProjectSettings(
            pyproject_toml_path=pyproject_toml,
            venv_path=venv_path,
            dependency_groups=("dev",),
            exclude_optional_dependencies=("docs",),
        )
    )

    assert [match.available.name for match in matches] == [
        "base-skill",
        "dev-skill",
        "shared-skill",
    ]


def test_repository_available_update_detects_github_skill_update(
    tmp_path: Path,
    monkeypatch,
) -> None:
    installed = Skill.from_text(
        """---
name: sample-skill
description: Installed GitHub skill.
---
Body
""",
        path=tmp_path / "installed",
        source="github",
        github_url="https://github.com/example/project/tree/main/skills/sample-skill",
        github_commit_sha="0123456789abcdef0123456789abcdef01234567",
    )
    refreshed = Skill.from_text(
        """---
name: sample-skill
description: Refreshed GitHub skill.
---
Updated body
""",
        path=tmp_path / "refreshed",
        source="github",
        github_url="https://github.com/example/project/tree/main/skills/sample-skill",
        github_commit_sha="89abcdef0123456789abcdef0123456789abcdef",
    )

    class FakeFetcher:
        base_url = None
        api_key = None
        github_token = None
        proxy = None

    monkeypatch.setattr(
        "skilly.repository.discover_github_skills",
        lambda fetcher, github_url, *, origin=None: [refreshed],
    )

    update = SkillRepository().available_update(installed, github_fetcher=FakeFetcher())

    assert update == refreshed


def test_repository_uses_bound_directory(tmp_path: Path) -> None:
    repository = SkillRepository(directory=tmp_path)
    installed = repository.install(Skill("sample-skill", "Installed skill."))

    assert repository.list() == [installed]


def test_repository_reports_malformed_installed_skill(tmp_path: Path) -> None:
    malformed = tmp_path / "malformed"
    malformed.mkdir()
    (malformed / "SKILL.md").write_text("not frontmatter\n", encoding="utf-8")

    repository = SkillRepository(directory=tmp_path)

    with pytest.raises(RuntimeError, match="Invalid installed skill.*malformed"):
        repository.list()
