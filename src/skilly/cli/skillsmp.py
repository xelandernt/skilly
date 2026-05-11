from __future__ import annotations

from pathlib import Path

from cyclopts import App

from skilly.cli.choices import (
    BACK_CHOICE,
    DELETE_CHOICE,
    EXIT_CHOICE,
    INSTALL_CHOICE,
    UPDATE_CHOICE,
)
from skilly.cli.previews import (
    installed_skill_preview_lines,
    skillsmp_installable_preview_lines,
    skillsmp_search_preview_lines,
)
from skilly.cli.ui import Menu, MenuItem, cli_ui
from skilly.constants import DEFAULT_SKILLS_PATH
from skilly.repository import SkillRepository
from skilly.skills import Skill
from skilly.skillsmp import SkillsMp, SkillsMpSkill


skillsmp_cli = App("skillsmp", help="Manage skills with skillsmp.")


def search_skill_label(skill: SkillsMpSkill) -> str:
    return f"{skill.name} [{skill.author}] ({skill.id})"


def installed_skill_label(skill: Skill) -> str:
    return f"{skill.directory_name}: {skill.name}"


@skillsmp_cli.command()
async def search(
    query: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    overwrite: bool = False,
) -> None:
    """Search the skillsmp database for skills."""
    skillsmp_client = SkillsMp()
    repository = SkillRepository()
    response = skillsmp_client.search(query)
    data = response.parsed_data
    if not data.data.skills:
        print(f"No SkillsMP skills found for {query}")
        return

    installable_skills: dict[str, Skill] = {}
    messages: list[str] = []
    status_message: str | None = None
    while True:
        skill = await cli_ui.select_async(
            Menu(
                title=f'Select a skill for "{query}"',
                items=tuple(
                    [
                        *search_skill_menu_items(data.data.skills),
                        exit_menu_item("Exit search"),
                    ]
                ),
                default=data.data.skills[0],
                preview_title="SkillsMP result",
                status=status_message,
            )
        )
        if skill is None or skill == EXIT_CHOICE:
            break

        installable = installable_skills.get(skill.id)
        if installable is None:
            installable = Skill.from_skillsmp(skillsmp_client, skill)
            installable_skills[skill.id] = installable

        action = await cli_ui.select_async(
            Menu(
                title=f"Choose an action for {skill.name}",
                items=tuple(
                    MenuItem(
                        value=item,
                        label=item,
                        preview_lines=skillsmp_installable_preview_lines(
                            skill, installable
                        ),
                    )
                    for item in [INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE]
                ),
                default=INSTALL_CHOICE,
                preview_title=skill.name,
                status=status_message,
            )
        )
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            break

        installed = repository.install(
            installable,
            directory=directory,
            overwrite=overwrite,
        )
        status_message = f"Installed {installed.name} to {installed.directory}"
        messages.append(status_message)

    if messages:
        print("\n".join(messages))


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
    messages: list[str] = []
    status_message: str | None = None

    while True:
        installed_skills = [
            skill for skill in repository.list(directory) if skill.is_skillsmp()
        ]
        if not installed_skills:
            if messages:
                break
            print(f"No SkillsMP-installed skills found in {directory.resolve()}")
            return

        skill = cli_ui.select(
            Menu(
                title="Select an installed SkillsMP skill",
                items=tuple(
                    [
                        *installed_skill_menu_items(installed_skills),
                        exit_menu_item("Exit list"),
                    ]
                ),
                default=installed_skills[0],
                preview_title="Installed skill",
                status=status_message,
            )
        )
        if skill is None or skill == EXIT_CHOICE:
            break

        action = cli_ui.select(
            Menu(
                title=f"Choose an action for {skill.directory_name}",
                items=tuple(
                    MenuItem(
                        value=item,
                        label=item,
                        preview_lines=installed_skill_preview_lines(skill),
                    )
                    for item in [UPDATE_CHOICE, DELETE_CHOICE, BACK_CHOICE, EXIT_CHOICE]
                ),
                default=UPDATE_CHOICE,
                preview_title=skill.directory_name,
                status=status_message,
            )
        )
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            break
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
            status_message = f"Updated {updated.directory_name} with {len(updated.resources) + 1} files"
            messages.append(status_message)
            continue

        removed = repository.remove(skill.directory_name, directory=directory)
        status_message = f"Removed {removed.directory_name}"
        messages.append(status_message)

    if messages:
        print("\n".join(messages))


def search_skill_menu_items(
    skills: list[SkillsMpSkill],
) -> list[MenuItem[SkillsMpSkill | str]]:
    return [
        MenuItem(
            value=skill,
            label=search_skill_label(skill),
            preview_lines=skillsmp_search_preview_lines(skill),
        )
        for skill in skills
    ]


def installed_skill_menu_items(skills: list[Skill]) -> list[MenuItem[Skill | str]]:
    return [
        MenuItem(
            value=skill,
            label=installed_skill_label(skill),
            preview_lines=installed_skill_preview_lines(skill),
        )
        for skill in skills
    ]


def exit_menu_item(preview_label: str) -> MenuItem[str]:
    return MenuItem(
        value=EXIT_CHOICE,
        label=EXIT_CHOICE,
        preview_lines=(preview_label,),
    )
