import csv
import json
from dataclasses import dataclass, field, replace
from email.parser import Parser
from pathlib import Path, PurePosixPath
from typing import Protocol
from urllib.parse import unquote, urlparse

from yaml import BaseLoader, YAMLError, load

from .constants import (
    DEFAULT_SKILLS_PATH,
    RESOURCE_KIND_ASSET,
    RESOURCE_KIND_OTHER,
    RESOURCE_KIND_REFERENCE,
    RESOURCE_KIND_SCRIPT,
    ResourceKind,
    SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY,
    SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY,
    SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY,
    SKILLY_GITHUB_URL_METADATA_KEY,
    SKILLY_MANAGED_METADATA_KEY,
    SKILLY_MANAGED_METADATA_VALUE,
    SKILLY_SKILLSMP_ID_METADATA_KEY,
    SKILLY_SOURCE_DEPENDENCY,
    SKILLY_SOURCE_GITHUB,
    SKILLY_SOURCE_METADATA_KEY,
    SKILLY_SOURCE_SKILLSMP,
    SKILLY_UNKNOWN_SOURCE,
)
from .filesystem import DEFAULT_FILE_SYSTEM, FileSystem


@dataclass(frozen=True)
class SkillResource:
    """A file bundled with a skill."""

    path: Path
    relative_path: PurePosixPath
    kind: ResourceKind
    content: str = ""


@dataclass(frozen=True)
class GitHubSkillLocation:
    """A parsed GitHub location for a skill directory."""

    owner: str
    repo: str
    ref: str | None
    path: PurePosixPath
    url: str

    @property
    def skill_name(self) -> str:
        """Return the skill directory name from the GitHub path."""
        return self.path.name if str(self.path) not in {"", "."} else self.repo


@dataclass(frozen=True)
class GitHubContentItem:
    """A single item returned from the GitHub contents API."""

    type: str
    name: str
    path: PurePosixPath
    commit_sha: str | None = None


@dataclass(frozen=True)
class GitHubFileBlob:
    """A text file fetched from GitHub."""

    path: PurePosixPath
    content: str
    size: int
    commit_sha: str | None = None


@dataclass(frozen=True)
class GitHubRepositorySnapshot:
    """A GitHub repository snapshot resolved to a specific commit."""

    ref: str
    commit_sha: str
    files: dict[PurePosixPath, GitHubFileBlob]


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
class Skill:
    """A skill definition with its content, resources, and source metadata."""

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
    source: str = SKILLY_UNKNOWN_SOURCE
    package_name: str | None = None
    package_version: str | None = None
    github_url: str | None = None
    github_commit_sha: str | None = None
    skillsmp_id: str | None = None

    @property
    def directory(self) -> Path:
        """Return the directory containing the skill file."""
        return self.path.parent

    @property
    def directory_name(self) -> str:
        """Return the installed directory name for the skill."""
        return self.directory.name

    @property
    def scripts(self) -> list[SkillResource]:
        """Return script resources bundled with the skill."""
        return [
            resource
            for resource in self.resources
            if resource.kind == RESOURCE_KIND_SCRIPT
        ]

    @property
    def references(self) -> list[SkillResource]:
        """Return reference resources bundled with the skill."""
        return [
            resource
            for resource in self.resources
            if resource.kind == RESOURCE_KIND_REFERENCE
        ]

    @property
    def assets(self) -> list[SkillResource]:
        """Return asset resources bundled with the skill."""
        return [
            resource
            for resource in self.resources
            if resource.kind == RESOURCE_KIND_ASSET
        ]

    def get_resource(
        self, relative_path: str | Path | PurePosixPath
    ) -> SkillResource | None:
        """Return a bundled resource by its relative path.

        Args:
            relative_path: Resource path relative to the skill directory.

        Returns:
            The matching resource, or None when no resource exists at that path.
        """
        wanted_path = to_relative_path(relative_path)
        for resource in self.resources:
            if resource.relative_path == wanted_path:
                return resource
        return None

    def is_installed(self) -> bool:
        """Return whether the skill carries Skilly installation metadata."""
        return (
            self.metadata.get(SKILLY_MANAGED_METADATA_KEY)
            == SKILLY_MANAGED_METADATA_VALUE
        )

    def is_dependency(self) -> bool:
        """Return whether the skill came from a dependency package."""
        return self.source == SKILLY_SOURCE_DEPENDENCY

    def is_skillsmp(self) -> bool:
        """Return whether the skill came from SkillsMP."""
        return self.source == SKILLY_SOURCE_SKILLSMP or self.skillsmp_id is not None

    def is_github(self) -> bool:
        """Return whether the skill came directly from GitHub."""
        return self.source == SKILLY_SOURCE_GITHUB

    def can_update(self) -> bool:
        """Return whether the skill has enough source information to update."""
        return self.is_dependency() or self.github_url is not None

    def matches(self, other: "Skill") -> bool:
        """Return whether two skills represent the same logical skill.

        Args:
            other: Another skill to compare against.

        Returns:
            True when both skills represent the same package-backed, GitHub-backed,
            or name-based skill identity.
        """
        if self.package_name is not None and other.package_name is not None:
            return (self.package_name, self.name) == (other.package_name, other.name)
        if self.github_url is not None and other.github_url is not None:
            return self.github_url == other.github_url
        return self.name == other.name

    def package_reference(self) -> str | None:
        """Return a display-friendly package reference.

        Returns:
            `<package>==<version>` when both values are present, the package name
            when only the name is known, or None when the skill is not package-backed.
        """
        if self.package_name is None:
            return None
        if self.package_version:
            return f"{self.package_name}=={self.package_version}"
        return self.package_name

    def managed_metadata(self) -> dict[str, str]:
        """Return metadata that should be written for installed skills."""
        metadata = dict(self.metadata)
        metadata[SKILLY_MANAGED_METADATA_KEY] = SKILLY_MANAGED_METADATA_VALUE
        if self.source in {
            SKILLY_SOURCE_DEPENDENCY,
            SKILLY_SOURCE_GITHUB,
            SKILLY_SOURCE_SKILLSMP,
        }:
            metadata[SKILLY_SOURCE_METADATA_KEY] = self.source
        if self.package_name is not None:
            metadata[SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY] = self.package_name
        if self.package_version is not None:
            metadata[SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY] = (
                self.package_version
            )
        if self.github_url is not None:
            metadata[SKILLY_GITHUB_URL_METADATA_KEY] = self.github_url
        if self.github_commit_sha is not None:
            metadata[SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY] = self.github_commit_sha
        if self.skillsmp_id is not None:
            metadata[SKILLY_SKILLSMP_ID_METADATA_KEY] = self.skillsmp_id
        return metadata

    def render(self, metadata: dict[str, str] | None = None) -> str:
        """Render the skill as SKILL.md content.

        Args:
            metadata: Extra metadata values to merge into the frontmatter.

        Returns:
            The serialized SKILL.md content.
        """
        combined_metadata = dict(self.metadata)
        if metadata is not None:
            combined_metadata.update(metadata)

        frontmatter = [
            f"name: {format_scalar(self.name)}",
            f"description: {format_scalar(self.description)}",
        ]
        if self.license is not None:
            frontmatter.append(f"license: {format_scalar(self.license)}")
        if self.compatibility is not None:
            frontmatter.append(f"compatibility: {format_scalar(self.compatibility)}")
        if self.allowed_tools is not None:
            frontmatter.append(f"allowed-tools: {format_scalar(self.allowed_tools)}")
        if combined_metadata:
            frontmatter.append("metadata:")
            for key in sorted(combined_metadata):
                frontmatter.append(f"  {key}: {format_scalar(combined_metadata[key])}")

        header = "\n".join(["---", *frontmatter, "---"])
        return f"{header}\n{self.content}" if self.content else f"{header}\n"

    def install_to(
        self,
        directory: Path = DEFAULT_SKILLS_PATH,
        *,
        skill_name: str | None = None,
        overwrite: bool = False,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
    ) -> "Skill":
        """Write the skill and its resources into a target directory.

        Args:
            directory: Root directory where skills are installed.
            skill_name: Optional directory name override.
            overwrite: Whether existing files may be overwritten.
            file_system: File system abstraction used for all file operations.

        Returns:
            The installed skill reloaded from the destination directory.
        """
        root = file_system.resolve(directory / (skill_name or self.name))
        file_system.make_dir(root, parents=True, exist_ok=True)

        skill_path = root / "SKILL.md"
        write_text_file(
            skill_path,
            self.render(metadata=self.managed_metadata()),
            overwrite=overwrite,
            file_system=file_system,
        )
        for resource in self.resources:
            destination = root / Path(*resource.relative_path.parts)
            write_text_file(
                destination,
                resource.content,
                overwrite=overwrite,
                file_system=file_system,
            )
        return Skill.from_dir(root, file_system=file_system)

    @classmethod
    def from_text(
        cls,
        text: str,
        *,
        path: Path | None = None,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
        source: str | None = None,
        package_name: str | None = None,
        package_version: str | None = None,
        github_url: str | None = None,
        github_commit_sha: str | None = None,
        skillsmp_id: str | None = None,
    ) -> "Skill":
        """Build a skill from SKILL.md text.

        Args:
            text: Full SKILL.md content.
            path: Optional path used to resolve bundled resources.
            file_system: File system abstraction used for file access.
            source: Explicit source override.
            package_name: Explicit dependency package name.
            package_version: Explicit dependency package version.
            github_url: Explicit GitHub source URL.
            github_commit_sha: Explicit GitHub commit SHA.
            skillsmp_id: Explicit SkillsMP identifier.

        Returns:
            The parsed skill.
        """
        skill_path = Path("SKILL.md") if path is None else file_system.resolve(path)
        frontmatter, body = split_frontmatter(text)
        parsed = parse_frontmatter(frontmatter)
        metadata_value = parsed.get("metadata")
        metadata = (
            {
                str(key): normalized
                for key, value in metadata_value.items()
                if (normalized := normalize_string_field(value)) is not None
            }
            if isinstance(metadata_value, dict)
            else {}
        )

        skill_source = source or infer_source(metadata)
        skill_package_name = package_name or metadata.get(
            SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY
        )
        skill_package_version = package_version or metadata.get(
            SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY
        )
        skill_github_url = github_url or metadata.get(SKILLY_GITHUB_URL_METADATA_KEY)
        skill_github_commit_sha = github_commit_sha or metadata.get(
            SKILLY_GITHUB_COMMIT_SHA_METADATA_KEY
        )
        skill_skillsmp_id = skillsmp_id or metadata.get(SKILLY_SKILLSMP_ID_METADATA_KEY)

        resources: list[SkillResource] = []
        warnings: list[str] = []
        if path is not None:
            resources, warnings = load_resource_files(
                skill_path, file_system=file_system
            )

        return cls(
            name=required_string_field(parsed, "name"),
            description=required_string_field(parsed, "description"),
            path=skill_path,
            content=body,
            license=optional_string_field(parsed, "license"),
            compatibility=optional_string_field(parsed, "compatibility"),
            metadata=metadata,
            allowed_tools=optional_string_field(parsed, "allowed-tools"),
            resources=resources,
            resource_warnings=warnings,
            source=skill_source,
            package_name=skill_package_name,
            package_version=skill_package_version,
            github_url=skill_github_url,
            github_commit_sha=skill_github_commit_sha,
            skillsmp_id=skill_skillsmp_id,
        )

    @classmethod
    def from_file(
        cls,
        path: Path,
        *,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
        source: str | None = None,
        package_name: str | None = None,
        package_version: str | None = None,
        github_url: str | None = None,
        github_commit_sha: str | None = None,
        skillsmp_id: str | None = None,
    ) -> "Skill":
        """Build a skill from a SKILL.md file."""
        return cls.from_text(
            file_system.read_file(path),
            path=path,
            file_system=file_system,
            source=source,
            package_name=package_name,
            package_version=package_version,
            github_url=github_url,
            github_commit_sha=github_commit_sha,
            skillsmp_id=skillsmp_id,
        )

    @classmethod
    def from_dir(
        cls,
        path: Path,
        *,
        file_system: FileSystem = DEFAULT_FILE_SYSTEM,
        source: str | None = None,
        package_name: str | None = None,
        package_version: str | None = None,
        github_url: str | None = None,
        github_commit_sha: str | None = None,
        skillsmp_id: str | None = None,
    ) -> "Skill":
        """Build a skill from a directory containing SKILL.md."""
        skill_path = find_skill_markdown_path(path, file_system=file_system)
        return cls.from_file(
            skill_path,
            file_system=file_system,
            source=source,
            package_name=package_name,
            package_version=package_version,
            github_url=github_url,
            github_commit_sha=github_commit_sha,
            skillsmp_id=skillsmp_id,
        )

    @classmethod
    def from_github(
        cls,
        fetcher: GitHubSkillFetcher,
        github_url: str,
        *,
        source: str = SKILLY_SOURCE_GITHUB,
        skillsmp_id: str | None = None,
    ) -> "Skill":
        """Build a skill from a GitHub skill directory URL."""
        skills = discover_github_skills(
            fetcher,
            github_url,
            source=source,
            skillsmp_id=skillsmp_id,
        )
        if len(skills) != 1:
            raise ValueError(
                f"GitHub URL resolves to {len(skills)} skills; "
                "use a direct skill directory URL instead"
            )
        return skills[0]

    @classmethod
    def from_skillsmp(
        cls,
        fetcher: GitHubSkillFetcher,
        installable_skill: SkillsMpInstallableSkill,
    ) -> "Skill":
        """Build a skill from a SkillsMP search result."""
        return cls.from_github(
            fetcher,
            installable_skill.githubUrl,
            source=SKILLY_SOURCE_SKILLSMP,
            skillsmp_id=installable_skill.id,
        )


@dataclass(frozen=True)
class DistributionInfo:
    """Minimal package metadata from a dist-info directory."""

    name: str
    version: str | None


def optional_string_field(data: dict[str, object], key: str) -> str | None:
    """Return an optional string field from parsed frontmatter."""
    return normalize_string_field(data.get(key))


def required_string_field(data: dict[str, object], key: str) -> str:
    """Return a required string field from parsed frontmatter.

    Args:
        data: Parsed frontmatter mapping.
        key: Required key to read.

    Returns:
        The field value.

    Raises:
        ValueError: If the field is missing or not a string.
    """
    value = normalize_string_field(data.get(key))
    if value is not None:
        return value
    raise ValueError(f"{key} must be a string")


def infer_source(metadata: dict[str, str]) -> str:
    """Infer the skill source from stored metadata."""
    source = metadata.get(SKILLY_SOURCE_METADATA_KEY)
    if source in {
        SKILLY_SOURCE_DEPENDENCY,
        SKILLY_SOURCE_GITHUB,
        SKILLY_SOURCE_SKILLSMP,
    }:
        return source
    if metadata.get(SKILLY_SKILLSMP_ID_METADATA_KEY) is not None:
        return SKILLY_SOURCE_SKILLSMP
    if metadata.get(SKILLY_GITHUB_URL_METADATA_KEY) is not None:
        return SKILLY_SOURCE_GITHUB
    return SKILLY_UNKNOWN_SOURCE


def find_skill_markdown_path(path: Path, *, file_system: FileSystem) -> Path:
    """Return the SKILL.md file path inside a skill directory.

    Args:
        path: Candidate skill directory.
        file_system: File system abstraction used for file access.

    Returns:
        The resolved SKILL.md path.

    Raises:
        FileNotFoundError: If the directory or SKILL.md file does not exist.
    """
    directory = file_system.resolve(path)
    if not file_system.is_dir(directory):
        raise FileNotFoundError(directory)
    for child_name in sorted(file_system.list_files(directory)):
        child = directory / child_name
        if not file_system.is_dir(child) and child_name.lower() == "skill.md":
            return file_system.resolve(child)
    raise FileNotFoundError(directory / "SKILL.md")


def write_text_file(
    path: Path,
    content: str,
    *,
    overwrite: bool,
    file_system: FileSystem,
) -> None:
    """Write a text file to disk.

    Args:
        path: Destination file path.
        content: Text content to write.
        overwrite: Whether existing files may be overwritten.
        file_system: File system abstraction used for file access.

    Raises:
        FileExistsError: If the destination exists and overwrite is False.
    """
    destination = file_system.resolve(path)
    file_system.make_dir(destination.parent, parents=True, exist_ok=True)
    if file_system.exists(destination) and not overwrite:
        raise FileExistsError(f"Refusing to overwrite existing file: {destination}")
    file_system.write_file(destination, content)


def load_resource_files(
    skill_path: Path,
    *,
    file_system: FileSystem,
) -> tuple[list[SkillResource], list[str]]:
    """Load text resources from a skill directory.

    Args:
        skill_path: Path to the skill's SKILL.md file.
        file_system: File system abstraction used for file access.

    Returns:
        A tuple of loaded resources and non-fatal warnings.
    """
    root = skill_path.parent
    if not file_system.is_dir(root):
        return [], []

    resources: list[SkillResource] = []
    warnings: list[str] = []
    for child_name in sorted(file_system.list_files(root)):
        if child_name.lower() == "skill.md":
            continue
        resource_path = root / child_name
        if file_system.is_dir(resource_path):
            resources.extend(
                collect_directory_resource_files(
                    root,
                    resource_path,
                    file_system=file_system,
                    warnings=warnings,
                )
            )
            continue
        try:
            resources.append(
                SkillResource(
                    path=file_system.resolve(resource_path),
                    relative_path=PurePosixPath(child_name),
                    kind=classify_resource_kind(PurePosixPath(child_name)),
                    content=file_system.read_file(resource_path),
                )
            )
        except (OSError, UnicodeDecodeError) as exc:
            warnings.append(f"{resource_path}: could not read bundled resource ({exc})")

    resources.sort(key=lambda resource: resource.relative_path.as_posix())
    warnings.sort()
    return resources, warnings


def collect_directory_resource_files(
    root: Path,
    current_path: Path,
    *,
    file_system: FileSystem,
    warnings: list[str],
) -> list[SkillResource]:
    """Recursively collect text resources from a directory."""
    collected: list[SkillResource] = []
    try:
        child_names = sorted(file_system.list_files(current_path))
    except OSError as exc:
        warnings.append(f"{current_path}: could not list bundled resources ({exc})")
        return collected

    for child_name in child_names:
        child = current_path / child_name
        if file_system.is_dir(child):
            collected.extend(
                collect_directory_resource_files(
                    root,
                    child,
                    file_system=file_system,
                    warnings=warnings,
                )
            )
            continue
        relative_path = PurePosixPath(*child.relative_to(root).parts)
        try:
            collected.append(
                SkillResource(
                    path=file_system.resolve(child),
                    relative_path=relative_path,
                    kind=classify_resource_kind(relative_path),
                    content=file_system.read_file(child),
                )
            )
        except (OSError, UnicodeDecodeError) as exc:
            warnings.append(f"{child}: could not read bundled resource ({exc})")
    return collected


def classify_resource_kind(relative_path: PurePosixPath) -> ResourceKind:
    """Classify a resource path by its top-level directory."""
    if not relative_path.parts:
        return RESOURCE_KIND_OTHER
    return {
        "scripts": RESOURCE_KIND_SCRIPT,
        "references": RESOURCE_KIND_REFERENCE,
        "assets": RESOURCE_KIND_ASSET,
    }.get(relative_path.parts[0], RESOURCE_KIND_OTHER)


def to_relative_path(path: str | Path | PurePosixPath) -> PurePosixPath:
    """Normalize a resource path to a PurePosixPath."""
    if isinstance(path, PurePosixPath):
        return path
    if isinstance(path, Path):
        return PurePosixPath(*path.parts)
    return PurePosixPath(path.replace("\\", "/"))


def split_frontmatter(text: str) -> tuple[list[str], str]:
    """Split SKILL.md content into frontmatter lines and body text."""
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise ValueError("missing YAML frontmatter")

    for index, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            return lines[1:index], "\n".join(lines[index + 1 :])
    raise ValueError("unterminated YAML frontmatter")


def parse_frontmatter(lines: list[str]) -> dict[str, object]:
    """Parse the top-level YAML frontmatter used by SKILL.md."""
    try:
        parsed = load("\n".join(lines), Loader=BaseLoader)
    except YAMLError as exc:
        raise ValueError(f"invalid YAML frontmatter: {exc}") from exc

    if parsed is None:
        return {}
    if not isinstance(parsed, dict):
        raise ValueError("frontmatter must be a mapping")
    return parsed


def normalize_string_field(value: object) -> str | None:
    """Normalize YAML string fields while trimming block-scalar trailing newlines."""
    if not isinstance(value, str):
        return None
    return value.rstrip("\n")


def format_scalar(value: str) -> str:
    """Serialize a scalar value for SKILL.md frontmatter."""
    if (
        value == ""
        or value.strip() != value
        or any(marker in value for marker in (": ", "#", "\n", "\r", '"'))
    ):
        return json.dumps(value)
    return value


def find_site_packages_dir(
    venv_path: Path,
    *,
    file_system: FileSystem,
) -> Path | None:
    """Find the site-packages directory inside a virtual environment."""
    windows_path = venv_path / "Lib" / "site-packages"
    if file_system.is_dir(windows_path):
        return windows_path

    for lib_name in ("lib", "lib64"):
        lib_dir = venv_path / lib_name
        if not file_system.is_dir(lib_dir):
            continue
        try:
            child_names = sorted(file_system.list_files(lib_dir), reverse=True)
        except OSError:
            continue
        for child_name in child_names:
            child = lib_dir / child_name
            site_packages = child / "site-packages"
            if (
                child.name.startswith("python")
                and file_system.is_dir(child)
                and file_system.is_dir(site_packages)
            ):
                return file_system.resolve(site_packages)
    return None


def list_dist_info_dirs(
    site_packages: Path,
    *,
    file_system: FileSystem,
) -> list[Path]:
    """List dist-info directories within a site-packages directory."""
    try:
        child_names = sorted(file_system.list_files(site_packages))
    except OSError:
        return []
    return [
        site_packages / child_name
        for child_name in child_names
        if child_name.endswith(".dist-info")
        and file_system.is_dir(site_packages / child_name)
    ]


def read_distribution_info(
    dist_info: Path,
    *,
    file_system: FileSystem,
) -> DistributionInfo | None:
    """Read package metadata from a dist-info directory."""
    try:
        metadata_text = file_system.read_file(dist_info / "METADATA")
    except OSError:
        return None
    metadata = Parser().parsestr(metadata_text)
    name = metadata.get("Name")
    if not isinstance(name, str) or not name:
        return None
    version = metadata.get("Version")
    return DistributionInfo(
        name=name, version=version if isinstance(version, str) else None
    )


def is_skill_record(installed_path: str) -> bool:
    """Return whether a RECORD entry points at a skill file."""
    parts = to_relative_path(installed_path).parts
    for index, part in enumerate(parts):
        if part == ".agents" and len(parts) > index + 3:
            return parts[index + 1] == "skills" and parts[-1] == "SKILL.md"
    return False


def resolve_record_path(
    site_packages: Path,
    installed_path: str,
    *,
    file_system: FileSystem,
) -> Path:
    """Resolve a RECORD entry relative to a site-packages directory."""
    path = site_packages
    for part in to_relative_path(installed_path).parts:
        path /= part
    return file_system.resolve(path)


def collect_github_files(
    fetcher: GitHubSkillFetcher,
    location: GitHubSkillLocation,
    current_path: PurePosixPath,
) -> dict[PurePosixPath, GitHubFileBlob]:
    """Recursively fetch all files under a GitHub directory."""
    files: dict[PurePosixPath, GitHubFileBlob] = {}
    for entry in fetcher.fetch_github_directory(location, current_path):
        if entry.type == "dir":
            files.update(collect_github_files(fetcher, location, entry.path))
            continue
        if entry.type == "file":
            blob = fetcher.fetch_github_file(location, entry.path)
            files[blob.path] = blob
    return files


def find_github_skill_roots(
    fetcher: GitHubSkillFetcher,
    location: GitHubSkillLocation,
    current_path: PurePosixPath,
) -> list[PurePosixPath]:
    """Recursively discover skill root directories without fetching file blobs."""
    entries = fetcher.fetch_github_directory(location, current_path)
    if any(entry.type == "file" and entry.name == "SKILL.md" for entry in entries):
        return [current_path]

    skill_roots: list[PurePosixPath] = []
    for entry in entries:
        if entry.type == "dir":
            skill_roots.extend(find_github_skill_roots(fetcher, location, entry.path))
    return skill_roots


def build_github_skill_url(
    location: GitHubSkillLocation,
    path: PurePosixPath,
    *,
    ref: str | None = None,
) -> str | None:
    """Build a canonical GitHub web URL for a skill path when the ref is known."""
    base_url = f"https://github.com/{location.owner}/{location.repo}"
    resolved_ref = location.ref if ref is None else ref
    if str(path) in {"", "."}:
        return (
            f"{base_url}/tree/{resolved_ref}" if resolved_ref is not None else base_url
        )
    if resolved_ref is None:
        return None
    return f"{base_url}/tree/{resolved_ref}/{path.as_posix()}"


def find_github_skill_dirs(
    files: dict[PurePosixPath, GitHubFileBlob],
    *,
    root: PurePosixPath,
) -> list[PurePosixPath]:
    """Find skill directories by locating SKILL.md files under a fetched root."""
    return sorted(
        {
            path.parent
            for path in files
            if path.name == "SKILL.md"
            and (str(root) in {"", "."} or path.is_relative_to(root))
        },
        key=lambda path: path.as_posix(),
    )


def build_skill_from_github_files(
    files: dict[PurePosixPath, GitHubFileBlob],
    skill_dir: PurePosixPath,
    *,
    source: str,
    github_url: str | None,
    skillsmp_id: str | None,
) -> Skill:
    """Build a skill from already-fetched GitHub file blobs."""
    skill_blob = files.get(skill_dir / "SKILL.md")
    if skill_blob is None:
        raise FileNotFoundError(f"SKILL.md not found at {skill_dir.as_posix()}")
    github_commit_sha = skill_blob.commit_sha or next(
        (
            blob.commit_sha
            for path, blob in sorted(files.items())
            if path.is_relative_to(skill_dir) and blob.commit_sha is not None
        ),
        None,
    )

    skill = Skill.from_text(
        skill_blob.content,
        path=Path(skill_dir.as_posix()) / "SKILL.md",
        source=source,
        github_url=github_url,
        github_commit_sha=github_commit_sha,
        skillsmp_id=skillsmp_id,
    )
    resources = [
        SkillResource(
            path=Path(blob.path.as_posix()),
            relative_path=blob.path.relative_to(skill_dir),
            kind=classify_resource_kind(blob.path.relative_to(skill_dir)),
            content=blob.content,
        )
        for path, blob in sorted(files.items())
        if path != skill_dir / "SKILL.md" and path.is_relative_to(skill_dir)
    ]
    return replace(skill, resources=resources)


def github_versions_match(installed: Skill, available: Skill) -> bool:
    """Return whether two GitHub-backed skills resolve to the same commit."""
    return (
        installed.github_commit_sha is not None
        and installed.github_commit_sha == available.github_commit_sha
    )


def discover_github_skills(
    fetcher: GitHubSkillFetcher,
    github_url: str,
    *,
    source: str = SKILLY_SOURCE_GITHUB,
    skillsmp_id: str | None = None,
) -> list[Skill]:
    """Discover one or more skills from a GitHub repo, directory, or skill URL."""
    location = parse_github_skill_url(github_url)
    snapshot_fetcher = getattr(fetcher, "fetch_github_snapshot", None)
    if callable(snapshot_fetcher):
        snapshot = snapshot_fetcher(location)
        skill_dirs = find_github_skill_dirs(snapshot.files, root=location.path)
        if not skill_dirs:
            raise FileNotFoundError(f"No SKILL.md found at {github_url}")
        if skillsmp_id is not None and len(skill_dirs) != 1:
            raise ValueError("SkillsMP metadata can only be attached to a single skill")
        return [
            build_skill_from_github_files(
                snapshot.files,
                skill_dir,
                source=source,
                github_url=(
                    github_url
                    if len(skill_dirs) == 1 and skill_dir == location.path
                    else build_github_skill_url(location, skill_dir, ref=snapshot.ref)
                ),
                skillsmp_id=skillsmp_id if len(skill_dirs) == 1 else None,
            )
            for skill_dir in skill_dirs
        ]

    skill_dirs = find_github_skill_roots(fetcher, location, location.path)
    if not skill_dirs:
        raise FileNotFoundError(f"No SKILL.md found at {github_url}")
    if skillsmp_id is not None and len(skill_dirs) != 1:
        raise ValueError("SkillsMP metadata can only be attached to a single skill")

    return [
        build_skill_from_github_files(
            collect_github_files(fetcher, location, skill_dir),
            skill_dir,
            source=source,
            github_url=(
                github_url
                if len(skill_dirs) == 1 and skill_dir == location.path
                else build_github_skill_url(location, skill_dir)
            ),
            skillsmp_id=skillsmp_id if len(skill_dirs) == 1 else None,
        )
        for skill_dir in skill_dirs
    ]


def parse_github_skill_url(github_url: str) -> GitHubSkillLocation:
    """Parse a GitHub skill directory URL.

    Args:
        github_url: GitHub URL in repo or `/tree/<ref>[/<path>]` form.

    Returns:
        The parsed GitHub location.

    Raises:
        ValueError: If the URL does not point to a supported GitHub repository URL.
    """
    parsed = urlparse(github_url)
    if parsed.netloc != "github.com":
        raise ValueError(f"Unsupported GitHub URL host: {parsed.netloc}")

    parts = [unquote(part) for part in parsed.path.split("/") if part]
    if len(parts) < 2:
        raise ValueError(
            "GitHub skill URLs must look like "
            "https://github.com/<owner>/<repo> or "
            "https://github.com/<owner>/<repo>/tree/<ref>/<path>"
        )

    ref: str | None = None
    path = PurePosixPath(".")
    if len(parts) >= 3:
        if parts[2] != "tree":
            raise ValueError(
                "GitHub skill URLs must look like "
                "https://github.com/<owner>/<repo> or "
                "https://github.com/<owner>/<repo>/tree/<ref>/<path>"
            )
        if len(parts) < 4:
            raise ValueError(
                "GitHub tree URLs must include a ref like "
                "https://github.com/<owner>/<repo>/tree/<ref>"
            )
        ref = parts[3]
        if len(parts) > 4:
            path = PurePosixPath(*parts[4:])

    return GitHubSkillLocation(
        owner=parts[0],
        repo=parts[1],
        ref=ref,
        path=path,
        url=github_url,
    )


def discover_installed_skills(
    directory: Path = DEFAULT_SKILLS_PATH,
    *,
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> list[Skill]:
    """Discover installed skills from a skills directory.

    Args:
        directory: Root directory containing installed skills.
        file_system: File system abstraction used for file access.

    Returns:
        The installed skills found in the directory.
    """
    root = file_system.resolve(directory)
    if not file_system.exists(root):
        return []
    if not file_system.is_dir(root):
        raise NotADirectoryError(root)

    skills: list[Skill] = []
    for child_name in sorted(file_system.list_files(root)):
        child = file_system.resolve(root / child_name)
        if not file_system.is_dir(child):
            continue
        try:
            skills.append(Skill.from_dir(child, file_system=file_system))
        except FileNotFoundError:
            continue
    return skills


def discover_venv_skills(
    path: Path = Path(".venv"),
    *,
    file_system: FileSystem = DEFAULT_FILE_SYSTEM,
) -> list[Skill]:
    """Discover dependency skills from a virtual environment.

    Args:
        path: Virtual environment root directory.
        file_system: File system abstraction used for file access.

    Returns:
        All skills found in distribution RECORD files under site-packages.
    """
    site_packages = find_site_packages_dir(
        file_system.resolve(path), file_system=file_system
    )
    if site_packages is None or not file_system.is_dir(site_packages):
        return []

    skills: list[Skill] = []
    seen_directories: set[Path] = set()
    for dist_info in list_dist_info_dirs(site_packages, file_system=file_system):
        distribution = read_distribution_info(dist_info, file_system=file_system)
        if distribution is None:
            continue

        try:
            record_text = file_system.read_file(dist_info / "RECORD")
        except OSError:
            continue

        for row in csv.reader(record_text.splitlines()):
            if not row or not is_skill_record(row[0]):
                continue
            skill_path = resolve_record_path(
                site_packages,
                row[0],
                file_system=file_system,
            )
            if skill_path.parent in seen_directories:
                continue
            seen_directories.add(skill_path.parent)
            try:
                skills.append(
                    Skill.from_file(
                        skill_path,
                        file_system=file_system,
                        source=SKILLY_SOURCE_DEPENDENCY,
                        package_name=distribution.name,
                        package_version=distribution.version,
                    )
                )
            except (OSError, UnicodeDecodeError, ValueError):
                continue

    return sorted(
        skills,
        key=lambda skill: (
            skill.package_name or "",
            skill.package_version or "",
            skill.name,
        ),
    )
