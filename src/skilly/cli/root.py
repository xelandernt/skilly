from __future__ import annotations

from pathlib import Path

import questionary
from cyclopts import App

from skilly.cli.choices import BACK_CHOICE, EXIT_CHOICE, REMOVE_CHOICE, UPDATE_CHOICE
from skilly.cli.skillsmp import skillsmp_cli
from skilly.cli.util import util_cli
from skilly.constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from skilly.repository import SkillMatch, SkillRepository
from skilly.skills import Skill
from skilly.skillsmp import SkillsMp


cli = App(help="Manage agent skills.")

cli.command(util_cli)
cli.command(skillsmp_cli)


@cli.command()
def scan(directory: Path = DEFAULT_SKILLS_PATH, dev: bool = False) -> None:
    """Scan dependencies for new or updated skills."""
    repository = SkillRepository()
    matches = repository.scan_project(directory=directory, include_dev=dev)
    if not matches:
        print("No dependency skills found in pyproject.toml and .venv")
        return

    actionable: dict[str, SkillMatch] = {}
    for match in matches:
        package_reference = match.available.package_reference() or "unknown"
        print(
            f"{match.available.name}[{package_reference}]: "
            f"{match.available.description} [{match.status.value}]"
        )
        if match.status is not SkillInstallStatus.INSTALLED:
            actionable[scan_choice_label(match)] = match

    if not actionable:
        print("All discovered dependency skills are already installed")
        return

    while actionable:
        selection = questionary.select(
            "Select dependency skill to install",
            default=next(iter(actionable)),
            choices=[*actionable, EXIT_CHOICE],
        ).ask()
        if selection in {None, EXIT_CHOICE}:
            return

        match = actionable.pop(selection)
        installed = repository.install(
            match.available,
            directory=directory,
            skill_name=match.installed.directory_name if match.installed else None,
            replace=match.installed is not None,
        )
        if match.installed is None:
            print(f"Installed {installed.directory_name} to {installed.directory}")
        else:
            print(
                f"Updated {installed.directory_name} to "
                f"{installed.package_version or 'unknown'}"
            )


@cli.command()
def list(directory: Path = DEFAULT_SKILLS_PATH) -> None:
    """List installed skills."""
    repository = SkillRepository()
    github_client = SkillsMp()

    while True:
        skills = repository.list(directory)
        if not skills:
            print(f"No installed skills found in {directory.resolve()}")
            return

        skills_by_label = {installed_skill_label(skill): skill for skill in skills}
        selection = questionary.select(
            "Select an installed skill",
            default=EXIT_CHOICE,
            choices=[*skills_by_label, EXIT_CHOICE],
        ).ask()
        if selection in {None, EXIT_CHOICE}:
            return

        skill = skills_by_label[selection]
        print_skill(skill)
        action = questionary.select(
            "Choose an action",
            default=BACK_CHOICE,
            choices=installed_skill_actions(skill),
        ).ask()
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            return
        if action == UPDATE_CHOICE:
            update_skill(repository, github_client, skill, directory=directory)
            continue

        removed = repository.remove(skill.directory_name, directory=directory)
        print(f"Removed {removed.directory_name}")


@cli.command()
def update(directory: Path = DEFAULT_SKILLS_PATH, force: bool = False) -> None:
    """Preview or apply dependency skill updates."""
    repository = SkillRepository()
    matches = repository.dependency_updates(directory=directory)
    if not matches:
        print("No dependency skill updates available")
        return

    for match in matches:
        print(
            f"{match.installed.directory_name}: {match.available.package_name} "
            f"{match.installed.package_version or 'unknown'} -> "
            f"{match.available.package_version or 'unknown'}"
        )

    if not force:
        print("Run with --force to apply these updates")
        return

    for match in matches:
        updated = repository.install(
            match.available,
            directory=directory,
            skill_name=match.installed.directory_name,
            replace=True,
        )
        print(
            f"Updated {updated.directory_name} to {updated.package_version or 'unknown'}"
        )


@cli.command()
def remove(name: str, directory: Path = DEFAULT_SKILLS_PATH) -> None:
    """Remove a skill."""
    removed = SkillRepository().remove(name, directory=directory)
    print(f"Removed {removed.directory_name}")


def scan_choice_label(match: SkillMatch) -> str:
    package_reference = match.available.package_reference() or "unknown"
    return f"{match.available.name} [{package_reference}] [{match.status.value}]"


def installed_skill_label(skill: Skill) -> str:
    details: list[str] = []
    package_reference = skill.package_reference()
    if package_reference is not None:
        details.append(package_reference)
    if skill.skillsmp_id is not None:
        details.append(f"id={skill.skillsmp_id}")
    detail_suffix = f" ({', '.join(details)})" if details else ""
    return f"{skill.directory_name}: {skill.name} [{skill.source}]{detail_suffix}"


def installed_skill_actions(skill: Skill) -> list[str]:
    actions = [REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
    if skill.can_update():
        actions.insert(0, UPDATE_CHOICE)
    return actions


def print_skill(skill: Skill) -> None:
    print(f"Name: {skill.name}")
    print(f"Directory: {skill.directory}")
    print(f"Source: {skill.source}")
    print(f"Installed: {skill.is_installed()}")
    package_reference = skill.package_reference()
    if package_reference is not None:
        print(f"Package: {package_reference}")
    if skill.github_url is not None:
        print(f"GitHub Url: {skill.github_url}")
    if skill.skillsmp_id is not None:
        print(f"SkillsMP Id: {skill.skillsmp_id}")


def update_skill(
    repository: SkillRepository,
    github_client: SkillsMp,
    skill: Skill,
    *,
    directory: Path,
) -> None:
    if skill.is_dependency():
        available = repository.available_dependency_skill(skill)
        if available is None:
            print(f"No dependency source found for {skill.directory_name}")
            return
        if available.package_version == skill.package_version:
            print(
                f"{skill.directory_name} is already up to date "
                f"({available.package_version or 'unknown'})"
            )
            return
        updated = repository.install(
            available,
            directory=directory,
            skill_name=skill.directory_name,
            replace=True,
        )
        print(
            f"Updated {updated.directory_name} to {updated.package_version or 'unknown'}"
        )
        return
    if skill.github_url is not None:
        refreshed = Skill.from_github(
            github_client,
            skill.github_url,
            source=skill.source,
            skillsmp_id=skill.skillsmp_id,
        )
        updated = repository.install(
            refreshed,
            directory=directory,
            skill_name=skill.directory_name,
            replace=True,
        )
        print(
            f"Updated {updated.directory_name} with {len(updated.resources) + 1} files"
        )
        return
    print(f"Cannot update {skill.directory_name}: unknown source")
