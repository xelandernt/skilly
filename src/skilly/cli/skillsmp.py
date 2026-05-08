from pathlib import Path

import questionary
from cyclopts import App

from skilly.cli.choices import (
    BACK_CHOICE,
    DELETE_CHOICE,
    EXIT_CHOICE,
    INSTALL_CHOICE,
    UPDATE_CHOICE,
)
from skilly.constants import DEFAULT_SKILLS_PATH
from skilly.skills import InstalledSkill, ManagedSkills
from skilly.skillsmp import AsyncSkillsMp, SkillsMp, SkillsMpSkill

skillsmp_cli = App("skillsmp", help="Manage skills with skillsmp.")


def _search_skill_label(skill: SkillsMpSkill) -> str:
    return f"{skill.name} [{skill.author}] ({skill.id})"


def _installed_skill_label(installed_skill: InstalledSkill) -> str:
    return f"{installed_skill.directory_name}: {installed_skill.skill.name}"


def _print_search_skill(skill: SkillsMpSkill) -> None:
    print(f"Name: {skill.name}")
    print(f"Description: {skill.description}")
    print(f"Url: {skill.skillUrl}")
    print(f"GitHub Url: {skill.githubUrl}")
    print(f"Author: {skill.author}")
    print(f"Id: {skill.id}")


def _print_installed_skill(installed_skill: InstalledSkill) -> None:
    print(f"Name: {installed_skill.skill.name}")
    print(f"Directory: {installed_skill.directory}")
    print(f"Managed by Skilly: {installed_skill.managed_by_skilly}")
    if installed_skill.github_url is not None:
        print(f"GitHub Url: {installed_skill.github_url}")
    if installed_skill.skillsmp_id is not None:
        print(f"SkillsMP Id: {installed_skill.skillsmp_id}")


@skillsmp_cli.command()
async def search(
    query: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    overwrite: bool = False,
) -> None:
    """Search the skillsmp database for skills."""
    search_client = AsyncSkillsMp()
    install_client = SkillsMp()
    managed_skills = ManagedSkills()
    response = await search_client.search(query)
    data = await response.parsed_data
    skill_by_label = {_search_skill_label(skill): skill for skill in data.data.skills}

    while True:
        selection = await questionary.select(
            "Select a skill",
            default=EXIT_CHOICE,
            choices=[*skill_by_label, EXIT_CHOICE],
        ).ask_async()
        if selection in {None, EXIT_CHOICE}:
            return

        skill = skill_by_label[selection]
        _print_search_skill(skill)
        action = await questionary.select(
            "Choose an action",
            default=BACK_CHOICE,
            choices=[INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        ).ask_async()
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            return

        downloaded = managed_skills.install_skill(
            install_client,
            skill,
            directory=directory,
            overwrite=overwrite,
        )
        print(f"Installed {skill.name} to {downloaded.destination}")


@skillsmp_cli.command()
def download(
    github_url: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    skill_name: str | None = None,
    overwrite: bool = False,
) -> None:
    """Download a skill usings its github url."""
    client = SkillsMp()
    managed_skills = ManagedSkills()
    downloaded = managed_skills.download_skill(
        client,
        github_url,
        directory=directory,
        skill_name=skill_name,
        overwrite=overwrite,
    )
    print(f"Downloaded {len(downloaded.files)} files to {downloaded.destination}")


@skillsmp_cli.command()
def list(directory: Path = Path(".agents/skills")) -> None:
    """List installed skills with skillsmp."""
    client = SkillsMp()
    managed_skills = ManagedSkills()

    while True:
        installed_skills = [
            installed_skill
            for installed_skill in managed_skills.list_installed_skills(directory)
            if installed_skill.source == "skillsmp"
        ]
        if not installed_skills:
            print(f"No SkillsMP-installed skills found in {directory.resolve()}")
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
            choices=[UPDATE_CHOICE, DELETE_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        ).ask()
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            return
        if action == UPDATE_CHOICE:
            downloaded = managed_skills.update_installed_skill(
                client,
                installed_skill.directory_name,
                directory=directory,
            )
            print(
                f"Updated {installed_skill.directory_name} with "
                f"{len(downloaded.files)} files"
            )
            continue

        removed_skill = managed_skills.remove_installed_skill(
            installed_skill.directory_name,
            directory=directory,
        )
        print(f"Removed {removed_skill.directory_name}")
