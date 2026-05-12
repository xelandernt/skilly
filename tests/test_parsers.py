from pathlib import Path

from skilly.filesystem import FileSystem
from skilly.skills import Skill, discover_venv_skills


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
    assert skill.package_reference() is None


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
        path=Path("/workspace/block-skill/SKILL.md"),
    )

    assert skill.description == "First line.\nSecond line."
    assert skill.metadata == {"owner": "docs"}


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
        path=Path("/workspace/binance-agentic-wallet/SKILL.md"),
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
    assert skill.path == (skill_dir / "SKILL.md").resolve()
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
    assert skill.path == skill_path.resolve()
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


def test_discover_venv_uses_filesystem_protocol_for_traversal() -> None:
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
    file_system.add_file(
        site_packages / "demo_pkg/.agents/skills/demo-skill/scripts/demo.py",
        "print('demo')\n",
    )
    file_system.add_file(
        site_packages / "demo_pkg/.agents/skills/demo-skill/references/REFERENCE.md",
        "# Demo reference\n",
    )

    skills = discover_venv_skills(venv_path, file_system=file_system)

    assert [skill.package_name for skill in skills] == ["demo-pkg"]
    assert skills[0].name == "demo-skill"
    assert [resource.relative_path.as_posix() for resource in skills[0].resources] == [
        "references/REFERENCE.md",
        "scripts/demo.py",
    ]


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
    assert skills[0].path == skill_path.resolve()


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

    def write_file(self, path: Path, content: str) -> None:
        self.add_file(path, content)

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

    def exists(self, path: Path) -> bool:
        resolved = self.resolve(path)
        return resolved in self._dirs or resolved in self._files

    def is_dir(self, path: Path) -> bool:
        return self.resolve(path) in self._dirs

    def make_dir(
        self, path: Path, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        del exist_ok
        resolved = self.resolve(path)
        if parents:
            self.add_dir(resolved)
            return
        if resolved.parent not in self._dirs:
            raise FileNotFoundError(resolved.parent)
        self._dirs.add(resolved)

    def remove_tree(self, path: Path) -> None:
        resolved = self.resolve(path)
        self._files = {
            file_path: content
            for file_path, content in self._files.items()
            if not file_path.is_relative_to(resolved)
        }
        self._dirs = {
            directory
            for directory in self._dirs
            if not directory.is_relative_to(resolved) or directory == Path("/")
        }

    def resolve(self, path: Path) -> Path:
        return Path("/").joinpath(path).resolve() if not path.is_absolute() else path
