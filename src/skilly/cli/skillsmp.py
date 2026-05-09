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
from skilly.repository import SkillRepository
from skilly.skills import Skill
from skilly.skillsmp import AsyncSkillsMp, SkillsMp, SkillsMpSkill


skillsmp_cli = App("skillsmp", help="Manage skills with skillsmp.")


def search_skill_label(skill: SkillsMpSkill) -> str:
    return f"{skill.name} [{skill.author}] ({skill.id})"


def installed_skill_label(skill: Skill) -> str:
    return f"{skill.directory_name}: {skill.name}"


def print_search_skill(skill: SkillsMpSkill) -> None:
    print(f"Name: {skill.name}")
    print(f"Description: {skill.description}")
    print(f"Url: {skill.skillUrl}")
    print(f"GitHub Url: {skill.githubUrl}")
    print(f"Author: {skill.author}")
    print(f"Id: {skill.id}")


def print_installed_skill(skill: Skill) -> None:
    print(f"Name: {skill.name}")
    print(f"Directory: {skill.directory}")
    print(f"Installed: {skill.is_installed()}")
    if skill.github_url is not None:
        print(f"GitHub Url: {skill.github_url}")
    if skill.skillsmp_id is not None:
        print(f"SkillsMP Id: {skill.skillsmp_id}")


@skillsmp_cli.command()
async def search(
    query: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    overwrite: bool = False,
) -> None:
    """Search the skillsmp database for skills."""
    search_client = AsyncSkillsMp()
    install_client = SkillsMp()
    repository = SkillRepository()
    response = await search_client.search(query)
    data = await response.parsed_data
    skills_by_label = {search_skill_label(skill): skill for skill in data.data.skills}

    while True:
        selection = await questionary.select(
            "Select a skill",
            default=EXIT_CHOICE,
            choices=[*skills_by_label, EXIT_CHOICE],
        ).ask_async()
        if selection in {None, EXIT_CHOICE}:
            return

        skill = skills_by_label[selection]
        print_search_skill(skill)
        action = await questionary.select(
            "Choose an action",
            default=BACK_CHOICE,
            choices=[INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE],
        ).ask_async()
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            return

        installed = repository.install(
            Skill.from_skillsmp(install_client, skill),
            directory=directory,
            overwrite=overwrite,
        )
        print(f"Installed {installed.name} to {installed.directory}")


@skillsmp_cli.command()
def download(
    github_url: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    skill_name: str | None = None,
    overwrite: bool = False,
) -> None:
    """Download a skill using its github url."""
    client = SkillsMp()
    repository = SkillRepository()
    installed = repository.install(
        Skill.from_github(client, github_url),
        directory=directory,
        skill_name=skill_name,
        overwrite=overwrite,
    )
    print(f"Downloaded {len(installed.resources) + 1} files to {installed.directory}")


@skillsmp_cli.command()
def list(directory: Path = DEFAULT_SKILLS_PATH) -> None:
    """List installed skills with skillsmp."""
    client = SkillsMp()
    repository = SkillRepository()

    while True:
        installed_skills = [
            skill for skill in repository.list(directory) if skill.is_skillsmp()
        ]
        if not installed_skills:
            print(f"No SkillsMP-installed skills found in {directory.resolve()}")
            return

        skills_by_label = {
            installed_skill_label(installed_skill): installed_skill
            for installed_skill in installed_skills
        }
        selection = questionary.select(
            "Select an installed skill",
            default=EXIT_CHOICE,
            choices=[*skills_by_label, EXIT_CHOICE],
        ).ask()
        if selection in {None, EXIT_CHOICE}:
            return

        skill = skills_by_label[selection]
        print_installed_skill(skill)
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
            updated = repository.install(
                Skill.from_github(
                    client,
                    skill.github_url,
                    source=skill.source,
                    skillsmp_id=skill.skillsmp_id,
                ),
                directory=directory,
                skill_name=skill.directory_name,
                replace=True,
            )
            print(
                f"Updated {updated.directory_name} with {len(updated.resources) + 1} files"
            )
            continue

        removed = repository.remove(skill.directory_name, directory=directory)
        print(f"Removed {removed.directory_name}")
