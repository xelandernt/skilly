import csv
from dataclasses import dataclass, field
from email.parser import Parser
from pathlib import Path, PurePosixPath
from typing import Protocol, Sequence
from urllib.parse import unquote, urlparse

from packaging.requirements import Requirement

from .constants import (
    DEFAULT_SKILLS_PATH,
    SKILLY_GITHUB_URL_METADATA_KEY,
    SKILLY_SKILLSMP_ID_METADATA_KEY,
    SKILLY_MANAGED_METADATA_KEY,
    SKILLY_MANAGED_METADATA_VALUE,
    SKILLY_SOURCE_METADATA_KEY,
    SKILLY_SOURCE_DEPENDENCY,
    SKILLY_SOURCE_SKILLSMP,
    SKILLY_UNKNOWN_SOURCE,
    SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY,
    SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY,
)
from .filesystem import DEFAULT_FILE_SYSTEM, FileSystem


@dataclass(frozen=True)
class SkillResource:
    path: Path
    relative_path: PurePosixPath
    kind: str
    content: str = ""


@dataclass(frozen=True)
class Skill:
    name: str
    description: str
    path: Path
    content: str = ""
    license: str | None = None
    compatibility: str | None = None
    metadata: dict[str, str] = field(default_factory=dict)
    allowed_tools: str | None = None
    resources: list[SkillResource] = field(default_factory=list)
    resource_warnings: list[str] = field(default_factory=list)

    @property
    def directory(self) -> Path:
        return self.path.parent

    @property
    def scripts(self) -> list[SkillResource]:
        return [resource for resource in self.resources if resource.kind == "script"]

    @property
    def references(self) -> list[SkillResource]:
        return [resource for resource in self.resources if resource.kind == "reference"]

    @property
    def assets(self) -> list[SkillResource]:
        return [resource for resource in self.resources if resource.kind == "asset"]

    def get_resource(
        self, relative_path: str | Path | PurePosixPath
    ) -> SkillResource | None:
        normalized_path = _to_relative_resource_path(relative_path)
        for resource in self.resources:
            if resource.relative_path == normalized_path:
                return resource
        return None

    @classmethod
    def from_text(
        cls,
        text: str,
        *,
        path: Path | None = None,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    ) -> "Skill":
        if path is None:
            skill_path = Path("SKILL.md")
        else:
            skill_path = file_system.resolve(path)

        frontmatter, body = _split_frontmatter(text)
        raw_metadata = _parse_frontmatter(frontmatter)

        raw_skill_metadata = raw_metadata.get("metadata")
        if isinstance(raw_skill_metadata, dict):
            skill_metadata = {
                str(key): value
                for key, value in raw_skill_metadata.items()
                if isinstance(value, str)
            }
        else:
            skill_metadata = {}

        skill_resources: list[SkillResource] = []
        resource_warnings: list[str] = []
        if path is not None:
            skill_resources, resource_warnings = _load_skill_resources(
                skill_path, file_system=file_system
            )

        return cls(
            name=raw_metadata.get("name")
            if isinstance(raw_metadata.get("name"), str)
            else "",
            description=raw_metadata.get("description")
            if isinstance(raw_metadata.get("description"), str)
            else "",
            path=skill_path,
            content=body,
            license=raw_metadata.get("license")
            if isinstance(raw_metadata.get("license"), str)
            else None,
            compatibility=raw_metadata.get("compatibility")
            if isinstance(raw_metadata.get("compatibility"), str)
            else None,
            metadata=skill_metadata,
            allowed_tools=raw_metadata.get("allowed-tools")
            if isinstance(raw_metadata.get("allowed-tools"), str)
            else None,
            resources=skill_resources,
            resource_warnings=resource_warnings,
        )

    @classmethod
    def from_file(
        cls,
        path: Path,
        *,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    ) -> "Skill":
        text = file_system.read_file(path)
        return cls.from_text(text, path=path, file_system=file_system)

    @classmethod
    def from_dir(
        cls,
        path: Path,
        *,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    ) -> "Skill":
        return cls.from_file(path / "SKILL.md", file_system=file_system)


@dataclass(frozen=True)
class DiscoveredSkill:
    package_name: str
    skill: Skill
    package_version: str = ""


@dataclass(frozen=True)
class VenvSkills:
    skills: list[DiscoveredSkill]
    path: Path
    site_packages_dir: Path | None = None
    warnings: list[str] = field(default_factory=list)

    def filter_skills(self, packages: Sequence[Requirement]) -> list[DiscoveredSkill]:
        package_names = [p.name for p in packages]
        return [skill for skill in self.skills if skill.package_name in package_names]

    @classmethod
    def from_dir(
        cls,
        path: Path = Path(".venv"),
        *,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    ) -> "VenvSkills":
        venv_path = file_system.resolve(path)
        site_packages_dir = _get_site_packages_dir(venv_path, file_system=file_system)
        warnings: list[str] = []
        skills: list[DiscoveredSkill] = []
        seen_skill_dirs: set[Path] = set()

        if site_packages_dir is None or not file_system.is_dir(site_packages_dir):
            warnings.append(f"Site-packages directory not found for virtualenv: {path}")
            return cls(
                skills=[],
                path=venv_path,
                site_packages_dir=site_packages_dir,
                warnings=warnings,
            )

        for dist_info in _find_dist_info_dirs(
            site_packages_dir, file_system=file_system
        ):
            distribution = _read_distribution_info(dist_info, file_system=file_system)
            if distribution is None:
                warnings.append(f"Skipping invalid distribution metadata: {dist_info}")
                continue

            found = _scan_distribution_records(
                site_packages_dir=site_packages_dir,
                dist_info=dist_info,
                package_name=distribution.name,
                package_version=distribution.version,
                seen_skill_dirs=seen_skill_dirs,
                file_system=file_system,
            )
            skills.extend(found.skills)
            warnings.extend(found.warnings)

        skills.sort(
            key=lambda item: (item.package_name, item.package_version, item.skill.name)
        )
        warnings.sort()
        return cls(
            skills=skills,
            path=venv_path,
            site_packages_dir=site_packages_dir,
            warnings=warnings,
        )


@dataclass(frozen=True)
class GitHubSkillLocation:
    owner: str
    repo: str
    ref: str
    path: PurePosixPath
    url: str

    @property
    def skill_name(self) -> str:
        return self.path.name


@dataclass(frozen=True)
class GitHubContentItem:
    type: str
    name: str
    path: PurePosixPath


@dataclass(frozen=True)
class GitHubFileBlob:
    path: PurePosixPath
    content: bytes
    size: int


@dataclass(frozen=True)
class DownloadedSkillFile:
    source_path: PurePosixPath
    destination_path: Path
    size: int


@dataclass(frozen=True)
class DownloadedSkill:
    source: GitHubSkillLocation
    destination: Path
    files: list[DownloadedSkillFile]


@dataclass(frozen=True)
class InstalledSkill:
    directory: Path
    skill: Skill

    @property
    def directory_name(self) -> str:
        return self.directory.name

    @property
    def github_url(self) -> str | None:
        return self.skill.metadata.get(SKILLY_GITHUB_URL_METADATA_KEY)

    @property
    def skillsmp_id(self) -> str | None:
        return self.skill.metadata.get(SKILLY_SKILLSMP_ID_METADATA_KEY)

    @property
    def managed_by_skilly(self) -> bool:
        return (
            self.skill.metadata.get(SKILLY_MANAGED_METADATA_KEY)
            == SKILLY_MANAGED_METADATA_VALUE
        )

    @property
    def source(self) -> str:
        source = self.skill.metadata.get(SKILLY_SOURCE_METADATA_KEY)
        if source in {SKILLY_SOURCE_DEPENDENCY, SKILLY_SOURCE_SKILLSMP}:
            return source
        if self.skillsmp_id is not None:
            return SKILLY_SOURCE_SKILLSMP
        return SKILLY_UNKNOWN_SOURCE

    @property
    def dependency_package_name(self) -> str | None:
        return self.skill.metadata.get(SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY)

    @property
    def dependency_package_version(self) -> str | None:
        return self.skill.metadata.get(SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY)


@dataclass(frozen=True)
class DependencySkillUpdate:
    installed_skill: InstalledSkill
    discovered_skill: DiscoveredSkill

    @property
    def package_name(self) -> str:
        return self.discovered_skill.package_name

    @property
    def installed_version(self) -> str:
        return self.installed_skill.dependency_package_version or ""

    @property
    def available_version(self) -> str:
        return self.discovered_skill.package_version


class GitHubSkillFetcher(Protocol):
    def fetch_github_directory(
        self,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
    ) -> list[GitHubContentItem]:
        """Fetch directory entries for a GitHub skill path."""

    def fetch_github_file(
        self,
        location: GitHubSkillLocation,
        path: PurePosixPath,
    ) -> GitHubFileBlob:
        """Fetch a GitHub skill file."""


class SkillsMpInstallableSkill(Protocol):
    id: str
    githubUrl: str


@dataclass(frozen=True)
class ManagedSkills:
    file_system: FileSystem = DEFAULT_FILE_SYSTEM

    def parse_github_url(self, github_url: str) -> GitHubSkillLocation:
        parsed_url = urlparse(github_url)
        if parsed_url.netloc != "github.com":
            raise ValueError(f"Unsupported GitHub URL host: {parsed_url.netloc}")

        parts = [unquote(part) for part in parsed_url.path.split("/") if part]
        if len(parts) < 5 or parts[2] != "tree":
            raise ValueError(
                "GitHub skill URLs must look like "
                "https://github.com/<owner>/<repo>/tree/<ref>/<path>"
            )

        path = PurePosixPath(*parts[4:])
        if str(path) in {"", "."}:
            raise ValueError("GitHub skill URL must include a directory path")

        return GitHubSkillLocation(
            owner=parts[0],
            repo=parts[1],
            ref=parts[3],
            path=path,
            url=github_url,
        )

    def download_skill(
        self,
        fetcher: GitHubSkillFetcher,
        github_url: str,
        *,
        directory: Path | None = None,
        skill_name: str | None = None,
        overwrite: bool = False,
        skillsmp_id: str | None = None,
    ) -> DownloadedSkill:
        location = self.parse_github_url(github_url)
        destination = self._get_download_destination(
            location,
            directory=directory,
            skill_name=skill_name,
        )
        files = self._download_github_directory(
            fetcher,
            location=location,
            current_path=location.path,
            destination=destination,
            overwrite=overwrite,
            skillsmp_id=skillsmp_id,
        )
        return DownloadedSkill(source=location, destination=destination, files=files)

    def install_skill(
        self,
        fetcher: GitHubSkillFetcher,
        skill: SkillsMpInstallableSkill,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        skill_name: str | None = None,
        overwrite: bool = False,
    ) -> DownloadedSkill:
        return self.download_skill(
            fetcher,
            skill.githubUrl,
            directory=directory,
            skill_name=skill_name,
            overwrite=overwrite,
            skillsmp_id=skill.id,
        )

    def install_discovered_skill(
        self,
        discovered_skill: DiscoveredSkill,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
        skill_name: str | None = None,
        overwrite: bool = False,
    ) -> InstalledSkill:
        destination = self.file_system.resolve(
            self.file_system.resolve(directory)
            / (skill_name or discovered_skill.skill.name)
        )
        metadata_updates = {
            SKILLY_MANAGED_METADATA_KEY: SKILLY_MANAGED_METADATA_VALUE,
            SKILLY_SOURCE_METADATA_KEY: SKILLY_SOURCE_DEPENDENCY,
            SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY: discovered_skill.package_name,
            SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY: discovered_skill.package_version,
        }
        self._copy_local_skill_directory(
            source_directory=discovered_skill.skill.directory,
            current_directory=discovered_skill.skill.directory,
            destination=destination,
            overwrite=overwrite,
            metadata_updates=metadata_updates,
        )
        return InstalledSkill(
            directory=destination,
            skill=Skill.from_dir(destination, file_system=self.file_system),
        )

    def list_installed_skills(
        self, directory: Path = DEFAULT_SKILLS_PATH
    ) -> list[InstalledSkill]:
        root = self.file_system.resolve(directory)
        if not self.file_system.exists(root):
            return []
        if not self.file_system.is_dir(root):
            raise NotADirectoryError(root)

        installed_skills: list[InstalledSkill] = []
        for child_name in sorted(self.file_system.list_files(root)):
            child = self.file_system.resolve(root / child_name)
            if not self.file_system.is_dir(child):
                continue
            skill_md_path = self._find_skill_md_path(child)
            if skill_md_path is None:
                continue
            installed_skills.append(
                InstalledSkill(
                    directory=child,
                    skill=Skill.from_file(skill_md_path, file_system=self.file_system),
                )
            )
        return installed_skills

    def find_installed_skill(
        self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH
    ) -> InstalledSkill | None:
        try:
            return self._resolve_installed_skill(name, directory=directory)
        except FileNotFoundError:
            return None

    def update_installed_skill(
        self,
        fetcher: GitHubSkillFetcher,
        name: str,
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
    ) -> DownloadedSkill:
        installed_skill = self._resolve_installed_skill(name, directory=directory)
        if installed_skill.github_url is None:
            raise ValueError(
                f"Installed skill {installed_skill.directory_name} does not have a stored GitHub URL"
            )
        return self.download_skill(
            fetcher,
            installed_skill.github_url,
            directory=directory,
            skill_name=installed_skill.directory_name,
            overwrite=True,
            skillsmp_id=installed_skill.skillsmp_id,
        )

    def list_dependency_skill_updates(
        self,
        discovered_skills: Sequence[DiscoveredSkill],
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
    ) -> list[DependencySkillUpdate]:
        discovered_by_key = {
            (
                discovered_skill.package_name,
                discovered_skill.skill.name,
            ): discovered_skill
            for discovered_skill in discovered_skills
        }
        updates: list[DependencySkillUpdate] = []
        for installed_skill in self.list_installed_skills(directory):
            if installed_skill.source != SKILLY_SOURCE_DEPENDENCY:
                continue
            package_name = installed_skill.dependency_package_name
            if package_name is None:
                continue
            discovered_skill = discovered_by_key.get(
                (package_name, installed_skill.skill.name)
            )
            if discovered_skill is None:
                continue
            if (
                discovered_skill.package_version
                == installed_skill.dependency_package_version
            ):
                continue
            updates.append(
                DependencySkillUpdate(
                    installed_skill=installed_skill,
                    discovered_skill=discovered_skill,
                )
            )
        updates.sort(
            key=lambda item: (
                item.package_name,
                item.installed_skill.directory_name,
                item.available_version,
            )
        )
        return updates

    def update_dependency_skills(
        self,
        discovered_skills: Sequence[DiscoveredSkill],
        *,
        directory: Path = DEFAULT_SKILLS_PATH,
    ) -> list[InstalledSkill]:
        updated_skills: list[InstalledSkill] = []
        for dependency_update in self.list_dependency_skill_updates(
            discovered_skills,
            directory=directory,
        ):
            self.file_system.remove_tree(dependency_update.installed_skill.directory)
            updated_skills.append(
                self.install_discovered_skill(
                    dependency_update.discovered_skill,
                    directory=directory,
                    skill_name=dependency_update.installed_skill.directory_name,
                    overwrite=True,
                )
            )
        return updated_skills

    def remove_installed_skill(
        self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH
    ) -> InstalledSkill:
        installed_skill = self._resolve_installed_skill(name, directory=directory)
        self.file_system.remove_tree(installed_skill.directory)
        return installed_skill

    def _get_download_destination(
        self,
        location: GitHubSkillLocation,
        *,
        directory: Path | None,
        skill_name: str | None,
    ) -> Path:
        download_directory = (
            self.file_system.resolve(directory)
            if directory is not None
            else self.file_system.resolve(Path("."))
        )
        return self.file_system.resolve(
            download_directory / (skill_name or location.skill_name)
        )

    def _copy_local_skill_directory(
        self,
        *,
        source_directory: Path,
        current_directory: Path,
        destination: Path,
        overwrite: bool,
        metadata_updates: dict[str, str],
    ) -> None:
        for child_name in sorted(self.file_system.list_files(current_directory)):
            child = self.file_system.resolve(current_directory / child_name)
            if self.file_system.is_dir(child):
                self._copy_local_skill_directory(
                    source_directory=source_directory,
                    current_directory=child,
                    destination=destination,
                    overwrite=overwrite,
                    metadata_updates=metadata_updates,
                )
                continue

            relative_path = child.relative_to(source_directory)
            destination_path = self.file_system.resolve(destination / relative_path)
            self._write_managed_file(
                content=self.file_system.read_bytes(child),
                destination_path=destination_path,
                overwrite=overwrite,
                metadata_updates=metadata_updates,
            )

    def _download_github_directory(
        self,
        fetcher: GitHubSkillFetcher,
        *,
        location: GitHubSkillLocation,
        current_path: PurePosixPath,
        destination: Path,
        overwrite: bool,
        skillsmp_id: str | None,
    ) -> list[DownloadedSkillFile]:
        downloaded_files: list[DownloadedSkillFile] = []
        for entry in fetcher.fetch_github_directory(location, current_path):
            if entry.type == "dir":
                downloaded_files.extend(
                    self._download_github_directory(
                        fetcher,
                        location=location,
                        current_path=entry.path,
                        destination=destination,
                        overwrite=overwrite,
                        skillsmp_id=skillsmp_id,
                    )
                )
                continue
            if entry.type != "file":
                continue

            file_blob = fetcher.fetch_github_file(location, entry.path)
            relative_path = file_blob.path.relative_to(location.path)
            destination_path = self.file_system.resolve(
                destination / Path(*relative_path.parts)
            )
            metadata_updates = {
                SKILLY_MANAGED_METADATA_KEY: SKILLY_MANAGED_METADATA_VALUE,
            }
            if location.url is not None:
                metadata_updates[SKILLY_GITHUB_URL_METADATA_KEY] = location.url
            if skillsmp_id is not None:
                metadata_updates[SKILLY_SKILLSMP_ID_METADATA_KEY] = skillsmp_id
                metadata_updates[SKILLY_SOURCE_METADATA_KEY] = SKILLY_SOURCE_SKILLSMP
            self._write_managed_file(
                content=file_blob.content,
                destination_path=destination_path,
                overwrite=overwrite,
                metadata_updates=metadata_updates,
            )
            downloaded_files.append(
                DownloadedSkillFile(
                    source_path=file_blob.path,
                    destination_path=destination_path,
                    size=file_blob.size,
                )
            )
        return downloaded_files

    def _write_managed_file(
        self,
        *,
        content: bytes,
        destination_path: Path,
        overwrite: bool,
        metadata_updates: dict[str, str],
    ) -> None:
        self.file_system.make_dir(destination_path.parent, parents=True, exist_ok=True)
        if self.file_system.exists(destination_path) and not overwrite:
            raise FileExistsError(
                f"Refusing to overwrite existing file: {destination_path}"
            )
        if _is_skill_md_name(destination_path.name):
            content = _update_skill_md_metadata(content, metadata_updates)
        self.file_system.write_bytes(destination_path, content)

    def _find_skill_md_path(self, directory: Path) -> Path | None:
        if not self.file_system.is_dir(directory):
            return None
        for child_name in sorted(self.file_system.list_files(directory)):
            child = self.file_system.resolve(directory / child_name)
            if not self.file_system.is_dir(child) and _is_skill_md_name(child_name):
                return child
        return None

    def _resolve_installed_skill(
        self, name: str, *, directory: Path = DEFAULT_SKILLS_PATH
    ) -> InstalledSkill:
        installed_skills = self.list_installed_skills(directory)
        for installed_skill in installed_skills:
            if installed_skill.directory_name == name:
                return installed_skill

        matching_skills = [
            installed_skill
            for installed_skill in installed_skills
            if installed_skill.skill.name == name
        ]
        if len(matching_skills) == 1:
            return matching_skills[0]
        if len(matching_skills) > 1:
            raise ValueError(f"Multiple installed skills match name: {name}")
        raise FileNotFoundError(f"Installed skill not found: {name}")


def _is_skill_md_name(name: str) -> bool:
    return name.lower() == "skill.md"


def _update_skill_md_metadata(
    content: bytes, metadata_updates: dict[str, str]
) -> bytes:
    text = content.decode("utf-8")
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise ValueError("SKILL.md must start with YAML frontmatter")

    closing_index: int | None = None
    for index, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            closing_index = index
            break

    if closing_index is None:
        raise ValueError("SKILL.md has unterminated YAML frontmatter")

    frontmatter = lines[1:closing_index]
    body = lines[closing_index + 1 :]
    metadata_index: int | None = None
    metadata_indent = "  "
    insert_index = len(frontmatter)

    line_index = 0
    while line_index < len(frontmatter):
        line = frontmatter[line_index]
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            line_index += 1
            continue
        if stripped == "metadata:":
            metadata_index = line_index
            insert_index = line_index + 1
            while insert_index < len(frontmatter):
                current_line = frontmatter[insert_index]
                current_stripped = current_line.strip()
                if not current_stripped or current_stripped.startswith("#"):
                    insert_index += 1
                    continue
                if not current_line.startswith((" ", "\t")):
                    break
                if current_line.startswith("  "):
                    metadata_indent = current_line[
                        : len(current_line) - len(current_line.lstrip())
                    ]
                insert_index += 1
            break
        line_index += 1

    if metadata_index is not None:
        existing_indices: dict[str, int] = {}
        for index in range(metadata_index + 1, insert_index):
            candidate = frontmatter[index].strip()
            for key in metadata_updates:
                if candidate.startswith(f"{key}:"):
                    existing_indices[key] = index

        pending_insertions: list[str] = []
        for key, value in metadata_updates.items():
            metadata_entry = f"{metadata_indent}{key}: {value}"
            existing_index = existing_indices.get(key)
            if existing_index is not None:
                frontmatter[existing_index] = metadata_entry
            else:
                pending_insertions.append(metadata_entry)
        if pending_insertions:
            frontmatter[insert_index:insert_index] = pending_insertions
    else:
        frontmatter.extend(["metadata:"])
        frontmatter.extend(
            [
                f"{metadata_indent}{key}: {value}"
                for key, value in metadata_updates.items()
            ]
        )

    updated_lines = ["---", *frontmatter, "---", *body]
    updated_text = "\n".join(updated_lines)
    if text.endswith("\n"):
        updated_text += "\n"
    return updated_text.encode("utf-8")


@dataclass(frozen=True)
class _DistributionInfo:
    name: str
    version: str


@dataclass(frozen=True)
class _ScanResult:
    skills: list[DiscoveredSkill] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


_RESOURCE_KIND_BY_DIRECTORY = {
    "scripts": "script",
    "references": "reference",
    "assets": "asset",
}


def _get_site_packages_dir(venv_path: Path, *, file_system: FileSystem) -> Path | None:
    windows_site_packages = venv_path / "Lib" / "site-packages"
    if file_system.is_dir(windows_site_packages):
        return windows_site_packages

    for lib_name in ("lib", "lib64"):
        lib_dir = venv_path / lib_name
        if not file_system.is_dir(lib_dir):
            continue
        for child in _child_paths(lib_dir, file_system=file_system, reverse=True):
            site_packages = child / "site-packages"
            if (
                child.name.startswith("python")
                and file_system.is_dir(child)
                and file_system.is_dir(site_packages)
            ):
                return site_packages

    return None


def _read_distribution_info(
    dist_info: Path,
    *,
    file_system: FileSystem,
) -> _DistributionInfo | None:
    metadata_path = dist_info / "METADATA"
    try:
        metadata_text = file_system.read_file(metadata_path)
    except OSError:
        return None

    metadata = Parser().parsestr(metadata_text)
    name = metadata.get("Name")
    if not isinstance(name, str) or not name:
        return None

    version = metadata.get("Version", "")
    return _DistributionInfo(
        name=name, version=version if isinstance(version, str) else ""
    )


def _scan_distribution_records(
    *,
    site_packages_dir: Path,
    dist_info: Path,
    package_name: str,
    package_version: str,
    seen_skill_dirs: set[Path],
    file_system: FileSystem,
) -> _ScanResult:
    record_path = dist_info / "RECORD"
    try:
        record_text = file_system.read_file(record_path)
    except OSError:
        return _ScanResult()

    skills: list[DiscoveredSkill] = []
    warnings: list[str] = []
    for row in csv.reader(record_text.splitlines()):
        if not row:
            continue

        installed_path = row[0]
        if not _is_skill_file_record(installed_path):
            continue

        skill_path = _resolve_record_path(
            site_packages_dir,
            installed_path,
            file_system=file_system,
        )
        skill_dir = skill_path.parent
        if skill_dir in seen_skill_dirs:
            continue
        seen_skill_dirs.add(skill_dir)

        discovered_skill, warning = _load_discovered_skill(
            skill_path=skill_path,
            package_name=package_name,
            package_version=package_version,
            file_system=file_system,
        )
        if discovered_skill is not None:
            skills.append(discovered_skill)
            warnings.extend(discovered_skill.skill.resource_warnings)
        elif warning is not None:
            warnings.append(warning)

    return _ScanResult(skills=skills, warnings=warnings)


def _is_skill_file_record(installed_path: str) -> bool:
    parts = PurePosixPath(installed_path).parts
    for index, part in enumerate(parts):
        if part != ".agents":
            continue
        if (
            len(parts) > index + 3
            and parts[index + 1] == "skills"
            and parts[-1] == "SKILL.md"
        ):
            return True
    return False


def _load_discovered_skill(
    *,
    skill_path: Path,
    package_name: str,
    package_version: str,
    file_system: FileSystem,
) -> tuple[DiscoveredSkill | None, str | None]:
    try:
        skill = Skill.from_file(skill_path, file_system=file_system)
    except (OSError, UnicodeDecodeError) as exc:
        return None, f"{skill_path}: could not read SKILL.md ({exc})"
    except ValueError as exc:
        return None, f"{skill_path}: {exc}"

    return (
        DiscoveredSkill(
            package_name=package_name,
            package_version=package_version,
            skill=skill,
        ),
        None,
    )


def _load_skill_resources(
    skill_path: Path, *, file_system: FileSystem
) -> tuple[list[SkillResource], list[str]]:
    skill_dir = skill_path.parent
    if not file_system.is_dir(skill_dir):
        return [], []
    try:
        child_names = sorted(file_system.list_files(skill_dir))
    except OSError as exc:
        return [], [f"{skill_dir}: could not list bundled resources ({exc})"]

    resources: list[SkillResource] = []
    warnings: list[str] = []
    for child_name in child_names:
        if child_name == "SKILL.md":
            continue
        _collect_skill_resources(
            root_dir=skill_dir,
            current_path=skill_dir / child_name,
            file_system=file_system,
            resources=resources,
            warnings=warnings,
        )

    resources.sort(key=lambda resource: resource.relative_path.as_posix())
    warnings.sort()
    return resources, warnings


def _collect_skill_resources(
    *,
    root_dir: Path,
    current_path: Path,
    file_system: FileSystem,
    resources: list[SkillResource],
    warnings: list[str],
) -> None:
    if file_system.is_dir(current_path):
        try:
            child_names = sorted(file_system.list_files(current_path))
        except OSError as exc:
            warnings.append(f"{current_path}: could not list bundled resources ({exc})")
            return
        for child_name in child_names:
            _collect_skill_resources(
                root_dir=root_dir,
                current_path=current_path / child_name,
                file_system=file_system,
                resources=resources,
                warnings=warnings,
            )
        return

    relative_path = PurePosixPath(*current_path.relative_to(root_dir).parts)
    try:
        content = file_system.read_file(current_path)
    except (OSError, UnicodeDecodeError) as exc:
        warnings.append(f"{current_path}: could not read bundled resource ({exc})")
        return
    resources.append(
        SkillResource(
            path=file_system.resolve(current_path),
            relative_path=relative_path,
            kind=_resource_kind(relative_path),
            content=content,
        )
    )


def _resource_kind(relative_path: PurePosixPath) -> str:
    if not relative_path.parts:
        return "other"
    return _RESOURCE_KIND_BY_DIRECTORY.get(relative_path.parts[0], "other")


def _to_relative_resource_path(
    path: str | Path | PurePosixPath,
) -> PurePosixPath:
    if isinstance(path, PurePosixPath):
        return path
    if isinstance(path, Path):
        return PurePosixPath(*path.parts)
    return PurePosixPath(path)


def _split_frontmatter(text: str) -> tuple[list[str], str]:
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise ValueError("missing YAML frontmatter")

    for index, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            return lines[1:index], "\n".join(lines[index + 1 :])

    raise ValueError("unterminated YAML frontmatter")


def _parse_frontmatter(lines: list[str]) -> dict[str, object]:
    metadata: dict[str, object] = {}
    line_index = 0
    while line_index < len(lines):
        line = lines[line_index]
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            line_index += 1
            continue
        if line[:1].isspace():
            raise ValueError("top-level frontmatter fields must not be indented")

        key, separator, raw_value = line.partition(":")
        if not separator:
            raise ValueError(f"invalid frontmatter line: {line}")

        key = key.strip()
        value = raw_value.lstrip()
        if key == "metadata" and not value:
            parsed_metadata, line_index = _parse_metadata_block(
                lines,
                start_index=line_index + 1,
            )
            metadata[key] = parsed_metadata
            continue

        parsed_value = _parse_scalar(value)
        if not isinstance(parsed_value, str):
            raise ValueError(f"{key} must be a string")
        metadata[key] = parsed_value
        line_index += 1

    return metadata


def _parse_metadata_block(
    lines: list[str],
    *,
    start_index: int,
) -> tuple[dict[str, str], int]:
    metadata: dict[str, str] = {}
    line_index = start_index

    while line_index < len(lines):
        line = lines[line_index]
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            line_index += 1
            continue
        if not line.startswith((" ", "\t")):
            break
        if line.startswith("\t"):
            raise ValueError("metadata entries must be indented with spaces")

        indentation = len(line) - len(line.lstrip(" "))
        if indentation < 2:
            raise ValueError("metadata entries must be indented by at least two spaces")

        key, separator, raw_value = line[indentation:].partition(":")
        if not separator:
            raise ValueError(f"invalid metadata line: {line.strip()}")

        parsed_value = _parse_scalar(raw_value.lstrip())
        if not isinstance(parsed_value, str):
            raise ValueError("metadata values must be strings")
        metadata[key.strip()] = parsed_value
        line_index += 1

    return metadata, line_index


def _parse_scalar(value: str) -> str | None:
    value = value.strip()
    if value == "null":
        return None
    if value.startswith("'") and value.endswith("'") and len(value) >= 2:
        return value[1:-1].replace("''", "'")
    if value.startswith('"') and value.endswith('"') and len(value) >= 2:
        return bytes(value[1:-1], "utf-8").decode("unicode_escape")
    return value


def _find_dist_info_dirs(
    site_packages_dir: Path,
    *,
    file_system: FileSystem,
) -> list[Path]:
    return [
        child
        for child in _child_paths(site_packages_dir, file_system=file_system)
        if child.name.endswith(".dist-info") and file_system.is_dir(child)
    ]


def _child_paths(
    path: Path,
    *,
    file_system: FileSystem,
    reverse: bool = False,
) -> list[Path]:
    try:
        names = sorted(file_system.list_files(path), reverse=reverse)
    except OSError:
        return []
    return [path / name for name in names]


def _resolve_record_path(
    site_packages_dir: Path,
    installed_path: str,
    *,
    file_system: FileSystem,
) -> Path:
    path = site_packages_dir
    for part in PurePosixPath(installed_path).parts:
        path /= part
    return file_system.resolve(path)
