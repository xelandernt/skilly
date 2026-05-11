import csv
from dataclasses import dataclass, field
from email.parser import Parser
from pathlib import Path, PurePosixPath
from typing import Sequence

from packaging.requirements import Requirement
from yaml import YAMLError, safe_load

from .filesystem import DEFAULT_FILE_SYSTEM, FileSystem


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
        name = _string_frontmatter_value(raw_metadata.get("name"))
        description = _string_frontmatter_value(raw_metadata.get("description"))
        license_value = _string_frontmatter_value(raw_metadata.get("license"))
        compatibility = _string_frontmatter_value(raw_metadata.get("compatibility"))
        allowed_tools = _string_frontmatter_value(raw_metadata.get("allowed-tools"))

        raw_skill_metadata = raw_metadata.get("metadata")
        if isinstance(raw_skill_metadata, dict):
            skill_metadata = {
                str(key): _string_frontmatter_value(value)
                for key, value in raw_skill_metadata.items()
                if _string_frontmatter_value(value) is not None
            }
        else:
            skill_metadata = {}

        return cls(
            name=name or "",
            description=description or "",
            path=skill_path,
            content=body,
            license=license_value,
            compatibility=compatibility,
            metadata=skill_metadata,
            allowed_tools=allowed_tools,
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
class _DistributionInfo:
    name: str
    version: str


@dataclass(frozen=True)
class _ScanResult:
    skills: list[DiscoveredSkill] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


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


def _split_frontmatter(text: str) -> tuple[list[str], str]:
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise ValueError("missing YAML frontmatter")

    for index, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            return lines[1:index], "\n".join(lines[index + 1 :])

    raise ValueError("unterminated YAML frontmatter")


def _parse_frontmatter(lines: list[str]) -> dict[str, object]:
    try:
        parsed = safe_load("\n".join(lines))
    except YAMLError as exc:
        raise ValueError(f"invalid YAML frontmatter: {exc}") from exc

    if parsed is None:
        return {}
    if not isinstance(parsed, dict):
        raise ValueError("frontmatter must be a mapping")
    return parsed


def _string_frontmatter_value(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    return value.rstrip("\n")


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
