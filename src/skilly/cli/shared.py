from __future__ import annotations

from pathlib import Path

from skilly.cli.choices import BACK_CHOICE, EXIT_CHOICE, REMOVE_CHOICE, UPDATE_CHOICE
from skilly.cli.previews import installed_skill_preview_lines
from skilly.cli.ui import MenuItem
from skilly.repository import SkillRepository
from skilly.skills import Skill, github_versions_match
from skilly.skillsmp import SkillsMp


def installed_skill_label(skill: Skill) -> str:
    details: list[str] = []
    package_reference = skill.package_reference()
    if package_reference is not None:
        details.append(package_reference)
    if skill.skillsmp_id is not None:
        details.append(f"id={skill.skillsmp_id}")
    detail_suffix = f" ({', '.join(details)})" if details else ""
    return f"{skill.directory_name}: {skill.name} [{skill.source}]{detail_suffix}"


def installed_skill_menu_items(skills: list[Skill]) -> list[MenuItem[Skill | str]]:
    return [
        MenuItem(
            value=skill,
            label=installed_skill_label(skill),
            preview_lines=installed_skill_preview_lines(skill),
        )
        for skill in skills
    ]


def installed_skill_actions(
    skill: Skill, *, remove_choice: str = REMOVE_CHOICE
) -> list[str]:
    actions = [remove_choice, BACK_CHOICE, EXIT_CHOICE]
    if skill.can_update():
        actions.insert(0, UPDATE_CHOICE)
    return actions


def exit_menu_item(preview_label: str) -> MenuItem[str]:
    return MenuItem(
        value=EXIT_CHOICE,
        label=EXIT_CHOICE,
        preview_lines=(preview_label,),
    )


def update_skill(
    repository: SkillRepository,
    github_client: SkillsMp,
    skill: Skill,
    *,
    directory: Path,
) -> str:
    if skill.is_dependency():
        available = repository.available_dependency_skill(skill)
        if available is None:
            return f"No dependency source found for {skill.directory_name}"
        if available.package_version == skill.package_version:
            return (
                f"{skill.directory_name} is already up to date "
                f"({available.package_version or 'unknown'})"
            )
        updated = repository.install(
            available,
            directory=directory,
            skill_name=skill.directory_name,
            replace=True,
        )
        return f"Updated {updated.directory_name} to {updated.package_version or 'unknown'}"
    if skill.github_url is not None:
        refreshed = Skill.from_github(
            github_client,
            skill.github_url,
            source=skill.source,
            skillsmp_id=skill.skillsmp_id,
        )
        if github_versions_match(skill, refreshed):
            return (
                f"{skill.directory_name} is already up to date "
                f"({skill.github_commit_sha})"
            )
        updated = repository.install(
            refreshed,
            directory=directory,
            skill_name=skill.directory_name,
            replace=True,
        )
        return (
            f"Updated {updated.directory_name} with {len(updated.resources) + 1} files"
        )
    return f"Cannot update {skill.directory_name}: unknown source"
