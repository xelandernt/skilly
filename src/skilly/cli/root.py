from __future__ import annotations

from pathlib import Path

import questionary
from cyclopts import App

from skilly.cli.choices import UPDATE_CHOICE, EXIT_CHOICE, BACK_CHOICE, REMOVE_CHOICE
from skilly.cli.skillsmp import skillsmp_cli
from skilly.cli.util import util_cli
from skilly.constants import DEFAULT_SKILLS_PATH
from skilly.skills import DiscoveredSkill, InstalledSkill, ManagedSkills
from skilly.skillsmp import SkillsMp
from skilly.util import get_project_discovered_skills


cli = App(help="Manage agent skills.")

cli.command(util_cli)
cli.command(skillsmp_cli)


@cli.command()
def scan(directory: Path = DEFAULT_SKILLS_PATH, dev: bool = False) -> None:
    """Scan dependencies for new skills."""
    managed_skills = ManagedSkills()
    discovered_skills = get_project_discovered_skills(include_dev=dev)
    if not discovered_skills:
        print("No dependency skills found in pyproject.toml and .venv")
        return

    installable_by_label: dict[str, DiscoveredSkill] = {}
    for discovered_skill in discovered_skills:
        installed_skill = managed_skills.find_installed_skill(
            discovered_skill.skill.name,
            directory=directory,
        )
        print(
            f"{discovered_skill.skill.name}[{discovered_skill.package_name}=="
            f"{discovered_skill.package_version}]: {discovered_skill.skill.description} "
            f"[{_scan_status(discovered_skill, installed_skill)}]"
        )
        if installed_skill is None:
            installable_by_label[_scan_choice_label(discovered_skill)] = (
                discovered_skill
            )

    if not installable_by_label:
        print("All discovered dependency skills are already installed")
        return

    while installable_by_label:
        default_selection = next(iter(installable_by_label))
        selection = questionary.select(
            "Select dependency skill to install",
            default=default_selection,
            choices=[*installable_by_label, EXIT_CHOICE],
        ).ask()
        if selection in {None, EXIT_CHOICE}:
            return

        installed_skill = managed_skills.install_discovered_skill(
            installable_by_label.pop(selection),
            directory=directory,
        )
        print(
            f"Installed {installed_skill.directory_name} to {installed_skill.directory}"
        )


@cli.command()
def list(directory: Path = DEFAULT_SKILLS_PATH) -> None:
    """List installed skills."""
    managed_skills = ManagedSkills()
    github_client = SkillsMp()

    while True:
        installed_skills = managed_skills.list_installed_skills(directory)
        if not installed_skills:
            print(f"No installed skills found in {directory.resolve()}")
            return

        skill_by_label = {
            _installed_skill_label(installed_skill): installed_skill
            for installed_skill in installed_skills
        }
        selection = questionary.select(
            "Select an installed skill",
            default=EXIT_CHOICE,
            choices=[*skill_by_label, EXIT_CHOICE],
        ).ask()
        if selection in {None, EXIT_CHOICE}:
            return

        installed_skill = skill_by_label[selection]
        _print_installed_skill(installed_skill)
        action = questionary.select(
            "Choose an action",
            default=BACK_CHOICE,
            choices=_installed_skill_actions(installed_skill),
        ).ask()
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            return
        if action == UPDATE_CHOICE:
            _update_listed_skill(
                managed_skills,
                github_client,
                installed_skill,
                directory=directory,
            )
            continue

        removed_skill = managed_skills.remove_installed_skill(
            installed_skill.directory_name,
            directory=directory,
        )
        print(f"Removed {removed_skill.directory_name}")


@cli.command()
def update(directory: Path = DEFAULT_SKILLS_PATH, force: bool = False) -> None:
    """Preview or apply dependency skill updates."""
    managed_skills = ManagedSkills()
    discovered_skills = get_project_discovered_skills()
    dependency_updates = managed_skills.list_dependency_skill_updates(
        discovered_skills,
        directory=directory,
    )
    if not dependency_updates:
        print("No dependency skill updates available")
        return

    for dependency_update in dependency_updates:
        print(
            f"{dependency_update.installed_skill.directory_name}: "
            f"{dependency_update.package_name} "
            f"{dependency_update.installed_version} -> "
            f"{dependency_update.available_version}"
        )

    if not force:
        print("Run with --force to apply these updates")
        return

    for updated_skill in managed_skills.update_dependency_skills(
        discovered_skills,
        directory=directory,
    ):
        print(
            f"Updated {updated_skill.directory_name} to "
            f"{updated_skill.dependency_package_version}"
        )


def _scan_choice_label(discovered_skill: DiscoveredSkill) -> str:
    return (
        f"{discovered_skill.skill.name} "
        f"[{discovered_skill.package_name}=={discovered_skill.package_version}]"
    )


def _installed_skill_label(installed_skill: InstalledSkill) -> str:
    details: list[str] = []
    if installed_skill.source == "dependency":
        details.append(
            f"{installed_skill.dependency_package_name}=="
            f"{installed_skill.dependency_package_version}"
        )
    elif (
        installed_skill.source == "skillsmp" and installed_skill.skillsmp_id is not None
    ):
        details.append(f"id={installed_skill.skillsmp_id}")
    detail_suffix = f" ({', '.join(details)})" if details else ""
    return (
        f"{installed_skill.directory_name}: {installed_skill.skill.name} "
        f"[{installed_skill.source}]{detail_suffix}"
    )


def _installed_skill_actions(installed_skill: InstalledSkill) -> list[str]:
    if installed_skill.source == "unknown":
        return [REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
    return [UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]


def _print_installed_skill(installed_skill: InstalledSkill) -> None:
    print(f"Name: {installed_skill.skill.name}")
    print(f"Directory: {installed_skill.directory}")
    print(f"Source: {installed_skill.source}")
    if installed_skill.source == "dependency":
        print(
            "Dependency: "
            f"{installed_skill.dependency_package_name}=="
            f"{installed_skill.dependency_package_version}"
        )
    if installed_skill.github_url is not None:
        print(f"GitHub Url: {installed_skill.github_url}")
    if installed_skill.skillsmp_id is not None:
        print(f"SkillsMP Id: {installed_skill.skillsmp_id}")


def _update_listed_skill(
    managed_skills: ManagedSkills,
    github_client: SkillsMp,
    installed_skill: InstalledSkill,
    *,
    directory: Path,
) -> None:
    if installed_skill.source == "dependency":
        dependency_skill = _find_dependency_skill(installed_skill)
        if dependency_skill is None:
            print(f"No dependency source found for {installed_skill.directory_name}")
            return
        if (
            dependency_skill.package_version
            == installed_skill.dependency_package_version
        ):
            print(
                f"{installed_skill.directory_name} is already up to date "
                f"({dependency_skill.package_version})"
            )
            return
        managed_skills.remove_installed_skill(
            installed_skill.directory_name,
            directory=directory,
        )
        updated_skill = managed_skills.install_discovered_skill(
            dependency_skill,
            directory=directory,
            skill_name=installed_skill.directory_name,
        )
        print(
            f"Updated {updated_skill.directory_name} to "
            f"{updated_skill.dependency_package_version}"
        )
        return
    if installed_skill.source == "skillsmp":
        downloaded = managed_skills.update_installed_skill(
            github_client,
            installed_skill.directory_name,
            directory=directory,
        )
        print(
            f"Updated {installed_skill.directory_name} with "
            f"{len(downloaded.files)} files"
        )
        return
    print(f"Cannot update {installed_skill.directory_name}: unknown source")


def _find_dependency_skill(installed_skill: InstalledSkill) -> DiscoveredSkill | None:
    package_name = installed_skill.dependency_package_name
    if package_name is None:
        return None
    for discovered_skill in get_project_discovered_skills():
        if (
            discovered_skill.package_name == package_name
            and discovered_skill.skill.name == installed_skill.skill.name
        ):
            return discovered_skill
    return None


def _scan_status(
    discovered_skill: DiscoveredSkill,
    installed_skill: InstalledSkill | None,
) -> str:
    if installed_skill is None:
        return "not installed"
    if installed_skill.source == "dependency":
        installed_version = installed_skill.dependency_package_version or "unknown"
        if installed_version == discovered_skill.package_version:
            return f"installed from dependency {installed_version}"
        return (
            f"installed from dependency {installed_version}, "
            f"available {discovered_skill.package_version}"
        )
    return f"installed from {installed_skill.source}"


@cli.command()
def remove(name: str, directory: Path = DEFAULT_SKILLS_PATH) -> None:
    """Remove a skill."""
    removed_skill = ManagedSkills().remove_installed_skill(name, directory=directory)
    print(f"Removed {removed_skill.directory_name}")
