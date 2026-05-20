import posixpath
from os import PathLike, fspath
from pathlib import Path, PurePosixPath

from skilly import (
    FileSystem,
    Skill,
    SkillRepository,
    get_project_skills,
    get_venv_skills,
)

StrPath = str | PathLike[str]


class InMemoryFileSystem(FileSystem):
    def __init__(self, *, cwd: Path = Path("/workspace")) -> None:
        self.cwd = cwd
        self._directories: set[Path] = {self.resolve(Path("/"))}
        self._files: dict[Path, str] = {}

    def seed_file(self, path: StrPath, content: str) -> None:
        resolved = self.resolve(path)
        self.make_dir(resolved.parent, parents=True, exist_ok=True)
        self._files[resolved] = content

    def read_file(self, path: StrPath) -> str:
        resolved = self.resolve(path)
        try:
            return self._files[resolved]
        except KeyError as error:
            raise FileNotFoundError(resolved) from error

    def write_file(self, path: StrPath, content: str) -> None:
        resolved = self.resolve(path)
        if resolved.parent not in self._directories:
            raise FileNotFoundError(resolved.parent)
        if resolved in self._directories:
            raise IsADirectoryError(resolved)
        self._files[resolved] = content

    def list_files(self, path: StrPath) -> list[str]:
        resolved = self.resolve(path)
        if resolved not in self._directories:
            raise FileNotFoundError(resolved)
        entries = set[str]()
        for candidate in self._directories | set(self._files):
            if candidate == resolved:
                continue
            try:
                relative = candidate.relative_to(resolved)
            except ValueError:
                continue
            if len(relative.parts) == 1:
                entries.add(relative.parts[0])
        return sorted(entries)

    def exists(self, path: StrPath) -> bool:
        resolved = self.resolve(path)
        return resolved in self._directories or resolved in self._files

    def is_dir(self, path: StrPath) -> bool:
        return self.resolve(path) in self._directories

    def make_dir(
        self, path: StrPath, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        resolved = self.resolve(path)
        if resolved in self._files:
            raise FileExistsError(resolved)
        if resolved in self._directories:
            if exist_ok:
                return
            raise FileExistsError(resolved)
        parent = resolved.parent
        if parent not in self._directories:
            if not parents:
                raise FileNotFoundError(parent)
            self.make_dir(parent, parents=True, exist_ok=True)
        self._directories.add(resolved)

    def remove_tree(self, path: StrPath) -> None:
        resolved = self.resolve(path)
        if resolved not in self._directories:
            raise FileNotFoundError(resolved)
        self._files = {
            candidate: content
            for candidate, content in self._files.items()
            if not self._is_relative_to(candidate, resolved)
        }
        self._directories = {
            candidate
            for candidate in self._directories
            if not self._is_relative_to(candidate, resolved) or candidate == Path("/")
        }

    def resolve(self, path: StrPath) -> Path:
        raw = fspath(path).replace("\\", "/")
        if not raw.startswith("/"):
            raw = posixpath.join(self.cwd.as_posix(), raw)
        return Path(posixpath.normpath(raw))

    @staticmethod
    def _is_relative_to(path: Path, base: Path) -> bool:
        try:
            path.relative_to(base)
        except ValueError:
            return False
        return True


class CrossHostPosixFileSystem(FileSystem):
    def __init__(self, *, cwd: PurePosixPath = PurePosixPath("/workspace")) -> None:
        self.cwd = cwd
        self._directories: set[str] = {"/"}
        self._files: dict[str, str] = {}

    def seed_file(self, path: PurePosixPath, content: str) -> None:
        resolved = self._resolve_storage_path(path)
        self.make_dir(
            PurePosixPath(posixpath.dirname(resolved)), parents=True, exist_ok=True
        )
        self._files[resolved] = content

    def read_file(self, path: StrPath) -> str:
        resolved = self._lookup_path(path)
        try:
            return self._files[resolved]
        except KeyError as error:
            raise FileNotFoundError(resolved) from error

    def write_file(self, path: StrPath, content: str) -> None:
        resolved = self._lookup_path(path)
        parent = posixpath.dirname(resolved)
        if parent not in self._directories:
            raise FileNotFoundError(parent)
        if resolved in self._directories:
            raise IsADirectoryError(resolved)
        self._files[resolved] = content

    def list_files(self, path: StrPath) -> list[str]:
        resolved = self._lookup_path(path)
        if resolved not in self._directories:
            raise FileNotFoundError(resolved)
        entries = set[str]()
        prefix = "/" if resolved == "/" else f"{resolved}/"
        for candidate in self._directories | set(self._files):
            if candidate == resolved or not candidate.startswith(prefix):
                continue
            remainder = candidate[len(prefix) :]
            if "/" not in remainder:
                entries.add(remainder)
            else:
                entries.add(remainder.split("/", 1)[0])
        return sorted(entries)

    def exists(self, path: StrPath) -> bool:
        resolved = self._lookup_path(path)
        return resolved in self._directories or resolved in self._files

    def is_dir(self, path: StrPath) -> bool:
        return self._lookup_path(path) in self._directories

    def make_dir(
        self, path: StrPath, *, parents: bool = False, exist_ok: bool = False
    ) -> None:
        resolved = self._lookup_path(path)
        if resolved in self._files:
            raise FileExistsError(resolved)
        if resolved in self._directories:
            if exist_ok:
                return
            raise FileExistsError(resolved)
        parent = posixpath.dirname(resolved) or "/"
        if parent not in self._directories:
            if not parents:
                raise FileNotFoundError(parent)
            self.make_dir(PurePosixPath(parent), parents=True, exist_ok=True)
        self._directories.add(resolved)

    def remove_tree(self, path: StrPath) -> None:
        resolved = self._lookup_path(path)
        if resolved not in self._directories:
            raise FileNotFoundError(resolved)
        prefix = "/" if resolved == "/" else f"{resolved}/"
        self._files = {
            candidate: content
            for candidate, content in self._files.items()
            if candidate != resolved and not candidate.startswith(prefix)
        }
        self._directories = {
            candidate
            for candidate in self._directories
            if candidate == "/"
            or (candidate != resolved and not candidate.startswith(prefix))
        }

    def resolve(self, path: StrPath) -> StrPath:
        resolved = self._resolve_storage_path(path)
        return Path(resolved.replace("/", "\\"))

    def _lookup_path(self, path: StrPath) -> str:
        raw = fspath(path)
        if "\\" in raw:
            return raw
        return self._resolve_storage_path(PurePosixPath(raw))

    def _resolve_storage_path(self, path: PurePosixPath | StrPath) -> str:
        raw = fspath(path)
        if raw.startswith("/"):
            return posixpath.normpath(raw)
        return posixpath.normpath(posixpath.join(str(self.cwd), raw.replace("\\", "/")))


def test_skill_from_text_resolves_paths_with_custom_file_system() -> None:
    file_system = InMemoryFileSystem(cwd=Path("/virtual/project"))

    skill = Skill.from_text(
        """---
name: parser-skill
description: Parse structured input.
---
Use the parser.
""",
        path=Path("skills/parser-skill/SKILL.md"),
        file_system=file_system,
    )

    assert skill.path == Path("/virtual/project/skills/parser-skill")
    assert skill.skill_markdown_path == Path(
        "/virtual/project/skills/parser-skill/SKILL.md"
    )


def test_skill_from_dir_loads_resources_with_custom_file_system() -> None:
    file_system = InMemoryFileSystem(cwd=Path("/virtual/project"))
    file_system.seed_file(
        Path("skills/sample-skill/SKILL.md"),
        """---
name: sample-skill
description: Load resources from a custom backend.
---
Body
""",
    )
    file_system.seed_file(
        Path("skills/sample-skill/scripts/run.py"), "print('sample')\n"
    )
    file_system.seed_file(
        Path("skills/sample-skill/references/REFERENCE.md"), "# Reference\n"
    )

    skill = Skill.from_dir(Path("skills/sample-skill"), file_system=file_system)

    assert skill.path == Path("/virtual/project/skills/sample-skill")
    assert [resource.relative_path.as_posix() for resource in skill.resources] == [
        "references/REFERENCE.md",
        "scripts/run.py",
    ]


def test_get_venv_skills_supports_custom_file_system() -> None:
    file_system = InMemoryFileSystem(cwd=Path("/virtual/project"))
    file_system.seed_file(
        Path(
            ".venv/lib/python3.13/site-packages/sample_pkg/.agents/skills/sample-skill/SKILL.md"
        ),
        """---
name: sample-skill
description: Dependency skill.
---
Body
""",
    )
    file_system.seed_file(
        Path(".venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/METADATA"),
        "Metadata-Version: 2.4\nName: sample-pkg\nVersion: 1.2.3\n",
    )
    file_system.seed_file(
        Path(".venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/RECORD"),
        "sample_pkg/.agents/skills/sample-skill/SKILL.md,,\n",
    )

    skills = get_venv_skills(file_system=file_system)

    assert [skill.name for skill in skills] == ["sample-skill"]
    assert skills[0].package_reference() == "sample-pkg==1.2.3"
    assert skills[0].path == Path(
        "/virtual/project/.venv/lib/python3.13/site-packages/sample_pkg/.agents/skills/sample-skill"
    )


def test_get_venv_skills_supports_posix_custom_file_system_on_windows_host() -> None:
    file_system = CrossHostPosixFileSystem(cwd=PurePosixPath("/virtual/project"))
    file_system.seed_file(
        PurePosixPath(
            "/virtual/project/.venv/lib/python3.13/site-packages/sample_pkg/.agents/skills/sample-skill/SKILL.md"
        ),
        """---
name: sample-skill
description: Dependency skill.
---
Body
""",
    )
    file_system.seed_file(
        PurePosixPath(
            "/virtual/project/.venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/METADATA"
        ),
        "Metadata-Version: 2.4\nName: sample-pkg\nVersion: 1.2.3\n",
    )
    file_system.seed_file(
        PurePosixPath(
            "/virtual/project/.venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/RECORD"
        ),
        "sample_pkg/.agents/skills/sample-skill/SKILL.md,,\n",
    )

    skills = get_venv_skills(file_system=file_system)

    assert [skill.name for skill in skills] == ["sample-skill"]
    assert skills[0].path == Path(
        "/virtual/project/.venv/lib/python3.13/site-packages/sample_pkg/.agents/skills/sample-skill"
    )


def test_get_project_skills_supports_custom_file_system() -> None:
    file_system = InMemoryFileSystem(cwd=Path("/virtual/project"))
    file_system.seed_file(
        Path("pyproject.toml"),
        """
[project]
name = "demo"
version = "0.1.0"
dependencies = ["sample-pkg==1.2.3"]
""".strip()
        + "\n",
    )
    file_system.seed_file(
        Path(
            ".venv/lib/python3.13/site-packages/sample_pkg/.agents/skills/sample-skill/SKILL.md"
        ),
        """---
name: sample-skill
description: Dependency skill.
---
Body
""",
    )
    file_system.seed_file(
        Path(".venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/METADATA"),
        "Metadata-Version: 2.4\nName: sample-pkg\nVersion: 1.2.3\n",
    )
    file_system.seed_file(
        Path(".venv/lib/python3.13/site-packages/sample-pkg-1.2.3.dist-info/RECORD"),
        "sample_pkg/.agents/skills/sample-skill/SKILL.md,,\n",
    )

    skills = get_project_skills(file_system=file_system)

    assert [skill.name for skill in skills] == ["sample-skill"]
    assert skills[0].package_name == "sample-pkg"


def test_repository_install_and_remove_support_custom_file_system() -> None:
    file_system = InMemoryFileSystem(cwd=Path("/virtual/project"))
    repository = SkillRepository(file_system=file_system)
    skill = Skill.from_text(
        """---
name: generated-skill
description: Write to a custom filesystem backend.
---
Generated instructions.
""",
        path=Path("source/generated-skill/SKILL.md"),
        file_system=file_system,
    )

    installed = repository.install(skill, directory=Path(".agents/skills"))

    skill_markdown = Path("/virtual/project/.agents/skills/generated-skill/SKILL.md")
    assert installed.path == Path("/virtual/project/.agents/skills/generated-skill")
    assert "skilly-managed-by: skilly" in file_system.read_file(skill_markdown)

    removed = repository.remove("generated-skill", directory=Path(".agents/skills"))

    assert removed.name == "generated-skill"
    assert file_system.exists(Path(".agents/skills/generated-skill")) is False
