from __future__ import annotations

from skilly.repository import SkillMatch
from skilly.skills import Skill
from skilly.skillsmp import SkillsMpSkill


def scan_match_preview_lines(match: SkillMatch) -> tuple[str, ...]:
    extra_lines = [f"Status: {match.status.value}"]
    if match.installed is not None:
        extra_lines.append(f"Installed Directory: {match.installed.directory_name}")
    return skill_preview_lines(match.available, extra_lines=extra_lines)


def installed_skill_preview_lines(skill: Skill) -> tuple[str, ...]:
    return skill_preview_lines(skill)


def skillsmp_search_preview_lines(skill: SkillsMpSkill) -> tuple[str, ...]:
    lines = [
        f"Name: {skill.name}",
        f"Description: {skill.description}",
        f"Author: {skill.author}",
        f"SkillsMP Url: {skill.skillUrl}",
        f"GitHub Url: {skill.githubUrl}",
        f"SkillsMP Id: {skill.id}",
    ]
    if skill.stars is not None:
        lines.append(f"Stars: {skill.stars}")
    if skill.updatedAt is not None:
        lines.append(f"Updated At: {skill.updatedAt}")
    return tuple(lines)


def skillsmp_installable_preview_lines(
    result: SkillsMpSkill, installable: Skill
) -> tuple[str, ...]:
    return (
        skillsmp_search_preview_lines(result) + ("",) + bundled_file_lines(installable)
    )


def skill_preview_lines(
    skill: Skill,
    *,
    extra_lines: list[str] | None = None,
) -> tuple[str, ...]:
    lines = [
        f"Name: {skill.name}",
        f"Description: {skill.description}",
        f"Source: {skill.source}",
        f"Installed: {skill.is_installed()}",
    ]
    package_reference = skill.package_reference()
    if package_reference is not None:
        lines.append(f"Package: {package_reference}")
    if skill.github_url is not None:
        lines.append(f"GitHub Url: {skill.github_url}")
    if skill.github_commit_sha is not None:
        lines.append(f"GitHub Commit: {skill.github_commit_sha}")
    if skill.skillsmp_id is not None:
        lines.append(f"SkillsMP Id: {skill.skillsmp_id}")
    if extra_lines:
        lines.extend(["", *extra_lines])
    lines.extend(["", *bundled_file_lines(skill)])
    return tuple(lines)


def bundled_file_lines(skill: Skill) -> tuple[str, ...]:
    return ("Files:", "  SKILL.md") + tuple(
        f"  {resource.relative_path.as_posix()}" for resource in skill.resources
    )
