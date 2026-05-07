from pathlib import Path

from libi.filesystem import FileSystem
from libi.skills import Skill, VenvSkills


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
    assert skill.body == "Use the parser."
    assert skill.path == Path("/workspace/parser-skill/SKILL.md")


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
    assert skill.path == Path("SKILL.md")


def test_skill_from_text_ignores_unknown_fields() -> None:
    skill = Skill.from_text(
        """---
name: relaxed-skill
description: Use when parsing should be permissive.
trigger: pdf
---
Relaxed instructions.
""",
        path=Path("/workspace/relaxed-skill/SKILL.md"),
    )

    assert skill.name == "relaxed-skill"
    assert skill.description == "Use when parsing should be permissive."
    assert skill.body == "Relaxed instructions."


def test_skill_from_dir_reads_skill_md(tmp_path: Path) -> None:
    skill_dir = tmp_path / "folder-skill"
    _write_skill(
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
    assert skill.path == (skill_dir / "SKILL.md").resolve()
    assert skill.body == "Directory based instructions."


def test_venv_skills_from_dir_discovers_recorded_skill(tmp_path: Path) -> None:
    venv_path, site_packages = _make_venv(tmp_path)
    skill_path = site_packages / "sample_pkg/.agents/skills/sample-skill/SKILL.md"
    _write_skill(
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
    _write_distribution(
        site_packages=site_packages,
        package_name="sample-pkg",
        package_version="1.2.3",
        record_rows=["sample_pkg/.agents/skills/sample-skill/SKILL.md,,"],
    )

    info = VenvSkills.from_dir(venv_path)

    assert info.path == venv_path.resolve()
    assert info.site_packages_dir == site_packages
    assert info.warnings == []
    assert [discovered.package_name for discovered in info.skills] == ["sample-pkg"]

    discovered = info.skills[0]
    assert discovered.package_version == "1.2.3"
    assert discovered.skill.name == "sample-skill"
    assert discovered.skill.description.startswith("Parse PDFs and forms.")
    assert discovered.skill.license == "Apache-2.0"
    assert discovered.skill.compatibility == "Requires Python 3.13+ and uv"
    assert discovered.skill.metadata == {"author": "example-org", "version": "1.0"}
    assert discovered.skill.allowed_tools == "Bash(git:*) Read"
    assert discovered.skill.path == skill_path.resolve()
    assert "Use this skill carefully." in discovered.skill.body


def test_venv_skills_from_dir_keeps_skills_with_unknown_fields(
    tmp_path: Path,
) -> None:
    venv_path, site_packages = _make_venv(tmp_path)
    skill_path = site_packages / "broken_pkg/.agents/skills/broken-skill/SKILL.md"
    _write_skill(
        skill_path,
        """---
name: broken-skill
description: This looks valid at first glance.
trigger: pdf
---
Invalid extra field.
""",
    )
    _write_distribution(
        site_packages=site_packages,
        package_name="broken-pkg",
        package_version="9.9.9",
        record_rows=["broken_pkg/.agents/skills/broken-skill/SKILL.md,,"],
    )

    info = VenvSkills.from_dir(venv_path)

    assert info.warnings == []
    assert len(info.skills) == 1
    assert info.skills[0].skill.name == "broken-skill"


def test_venv_skills_from_dir_uses_filesystem_protocol_for_traversal() -> None:
    file_system = FakeFileSystem()
    venv_path = Path("/workspace/.venv")
    site_packages = Path("/workspace/.venv/lib/python3.13/site-packages")
    file_system.add_dir(site_packages)
    file_system.add_file(
        site_packages / "demo-pkg-1.0.0.dist-info/METADATA",
        "\n".join(
            [
                "Metadata-Version: 2.4",
                "Name: demo-pkg",
                "Version: 1.0.0",
                "",
            ]
        ),
    )
    file_system.add_file(
        site_packages / "demo-pkg-1.0.0.dist-info/RECORD",
        "demo_pkg/.agents/skills/demo-skill/SKILL.md,,",
    )
    file_system.add_file(
        site_packages / "demo_pkg/.agents/skills/demo-skill/SKILL.md",
        """---
name: demo-skill
description: Use when the task mentions demos.
---
Demo instructions.
""",
    )

    info = VenvSkills.from_dir(venv_path, file_system=file_system)

    assert [discovered.package_name for discovered in info.skills] == ["demo-pkg"]
    assert info.skills[0].skill.name == "demo-skill"
    assert info.warnings == []


def _make_venv(root: Path) -> tuple[Path, Path]:
    venv_path = root / ".venv"
    site_packages = venv_path / "lib/python3.13/site-packages"
    site_packages.mkdir(parents=True)
    return venv_path, site_packages.resolve()


def _write_distribution(
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


def _write_skill(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


class FakeFileSystem(FileSystem):
    def __init__(self) -> None:
        self._dirs: set[Path] = set()
        self._files: dict[Path, str] = {}

    def add_dir(self, path: Path) -> None:
        current = self.resolve(path)
        while True:
            self._dirs.add(current)
            if current.parent == current:
                break
            current = current.parent

    def add_file(self, path: Path, content: str) -> None:
        resolved = self.resolve(path)
        self.add_dir(resolved.parent)
        self._files[resolved] = content

    def read_file(self, path: Path) -> str:
        resolved = self.resolve(path)
        if resolved not in self._files:
            raise FileNotFoundError(resolved)
        return self._files[resolved]

    def list_files(self, path: Path) -> list[str]:
        resolved = self.resolve(path)
        if resolved not in self._dirs:
            raise FileNotFoundError(resolved)

        children: set[str] = set()
        for directory in self._dirs:
            if directory.parent == resolved and directory != resolved:
                children.add(directory.name)
        for file_path in self._files:
            if file_path.parent == resolved:
                children.add(file_path.name)
        return sorted(children)

    def is_dir(self, path: Path) -> bool:
        return self.resolve(path) in self._dirs

    def resolve(self, path: Path) -> Path:
        return Path("/").joinpath(path).resolve() if not path.is_absolute() else path
