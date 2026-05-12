from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from cyclopts import App

from skilly.cli.choices import (
    BACK_CHOICE,
    DELETE_CHOICE,
    EXIT_CHOICE,
    INSTALL_CHOICE,
    REMOVE_CHOICE,
    UPDATE_CHOICE,
)
from skilly.cli.previews import (
    installed_skill_preview_lines,
    skill_preview_lines,
    skillsmp_installable_preview_lines,
    skillsmp_search_preview_lines,
)
from skilly.cli.shared import (
    exit_menu_item,
    installed_skill_actions,
    installed_skill_menu_items,
    update_skill,
)
from skilly.cli.ui import Menu, MenuItem, cli_ui
from skilly.constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from skilly.repository import SkillRepository
from skilly.skills import Skill, discover_github_skills, github_versions_match
from skilly.skillsmp import SkillsMp, SkillsMpSkill


skillsmp_cli = App("skillsmp", help="Manage skills with skillsmp.")


@dataclass(frozen=True)
class DownloadableSkillMatch:
    available: Skill
    installed: Skill | None = None

    @property
    def status(self) -> SkillInstallStatus:
        if self.installed is None:
            return SkillInstallStatus.INSTALLABLE
        if github_versions_match(self.installed, self.available):
            return SkillInstallStatus.INSTALLED
        return SkillInstallStatus.UPDATABLE


def search_skill_label(skill: SkillsMpSkill) -> str:
    return f"{skill.name} [{skill.author}] ({skill.id})"


@skillsmp_cli.command()
async def search(
    query: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    overwrite: bool = False,
    github_token: str | None = None,
) -> None:
    """Search the skillsmp database for skills."""
    skillsmp_client = SkillsMp(github_token=github_token)
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


def download(
    github_url: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    skill_name: str | None = None,
    all: bool = False,
    overwrite: bool = False,
    github_token: str | None = None,
) -> None:
    """Download a skill using its github url."""
    client = SkillsMp(github_token=github_token)
    repository = SkillRepository()
    skills = discover_github_skills(client, github_url)
    if all and skill_name is not None and len(skills) != 1:
        raise ValueError(
            "Use either --skill-name or --all when downloading multiple skills"
        )
    if len(skills) > 1 and not all and skill_name is None:
        download_selected_skills(
            skills, client, repository, directory=directory, overwrite=overwrite
        )
        return
    if skill_name is not None and len(skills) != 1 and not all:
        skills = [select_download_skill(skills, skill_name)]
    elif skill_name is not None and len(skills) != 1:
        raise ValueError(
            "Custom skill names can only be used when downloading a single skill"
        )

    installed_skills = [
        repository.install(
            skill,
            directory=directory,
            skill_name=skill_name if len(skills) == 1 else None,
            overwrite=overwrite,
        )
        for skill in skills
    ]
    if len(installed_skills) == 1:
        installed = installed_skills[0]
        print(
            f"Downloaded {len(installed.resources) + 1} files to {installed.directory}"
        )
        return
    for installed in installed_skills:
        print(
            f"Downloaded {installed.directory_name} "
            f"with {len(installed.resources) + 1} files to {installed.directory}"
        )


def download_selected_skills(
    skills: list[Skill],
    client: SkillsMp,
    repository: SkillRepository,
    *,
    directory: Path,
    overwrite: bool,
) -> None:
    """Interactively choose one or more skills to install from a multi-skill source."""
    messages: list[str] = []
    status_message: str | None = None
    while True:
        matches = downloadable_skill_matches(skills, repository.list(directory))
        selected = cli_ui.select(
            Menu(
                title="Select a skill to download",
                items=tuple(
                    [
                        *downloadable_skill_menu_items(matches),
                        exit_menu_item("Exit download"),
                    ]
                ),
                default=matches[0],
                preview_title="Downloadable skill",
                status=status_message,
            )
        )
        if selected is None or selected == EXIT_CHOICE:
            break

        actions = downloadable_skill_actions(selected)
        action = cli_ui.select(
            Menu(
                title=f"Choose an action for {selected.available.directory_name}",
                items=tuple(
                    MenuItem(
                        value=item,
                        label=item,
                        preview_lines=downloadable_skill_preview_lines(selected),
                    )
                    for item in actions
                ),
                default=actions[0],
                preview_title=selected.available.directory_name,
                status=status_message,
            )
        )
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            break

        if action == INSTALL_CHOICE:
            installed = repository.install(
                selected.available,
                directory=directory,
                overwrite=overwrite,
            )
            status_message = (
                f"Downloaded {installed.directory_name} "
                f"with {len(installed.resources) + 1} files to {installed.directory}"
            )
        elif action == UPDATE_CHOICE:
            if selected.installed is None:
                raise ValueError("Only installed skills can be updated")
            status_message = update_skill(
                repository, client, selected.installed, directory=directory
            )
        else:
            if selected.installed is None:
                raise ValueError("Only installed skills can be removed")
            removed = repository.remove(
                selected.installed.directory_name, directory=directory
            )
            status_message = f"Removed {removed.directory_name}"
        messages.append(status_message)

    if messages:
        print("\n".join(messages))


def select_download_skill(skills: list[Skill], skill_name: str) -> Skill:
    """Select a single downloadable skill by directory name or unique skill name."""
    for skill in skills:
        if skill.directory_name == skill_name:
            return skill

    matches = [skill for skill in skills if skill.name == skill_name]
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        raise ValueError(f"Multiple downloadable skills match name: {skill_name}")

    available = ", ".join(sorted(skill.directory_name for skill in skills))
    raise FileNotFoundError(
        f"Downloadable skill not found: {skill_name}. Available: {available}"
    )


@skillsmp_cli.command()
def list(
    directory: Path = DEFAULT_SKILLS_PATH, github_token: str | None = None
) -> None:
    """List installed skills with skillsmp."""
    client = SkillsMp(github_token=github_token)
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
                    for item in installed_skill_actions(
                        skill, remove_choice=DELETE_CHOICE
                    )
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
            status_message = update_skill(
                repository,
                client,
                skill,
                directory=directory,
            )
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


def downloadable_skill_label(match: DownloadableSkillMatch) -> str:
    return (
        f"{match.available.directory_name}: {match.available.name} "
        f"[{match.status.value}]"
    )


def downloadable_skill_preview_lines(match: DownloadableSkillMatch) -> tuple[str, ...]:
    extra_lines = [f"Status: {match.status.value}"]
    if match.installed is not None:
        extra_lines.append(f"Installed Directory: {match.installed.directory_name}")
    return skill_preview_lines(match.available, extra_lines=extra_lines)


def downloadable_skill_menu_items(
    matches: list[DownloadableSkillMatch],
) -> list[MenuItem[DownloadableSkillMatch | str]]:
    return [
        MenuItem(
            value=match,
            label=downloadable_skill_label(match),
            preview_lines=downloadable_skill_preview_lines(match),
            style=downloadable_skill_style(match),
        )
        for match in matches
    ]


def downloadable_skill_style(match: DownloadableSkillMatch) -> str:
    if match.status is SkillInstallStatus.INSTALLED:
        return "class:menu-item-installed"
    return ""


def downloadable_skill_actions(match: DownloadableSkillMatch) -> list[str]:
    if match.status is SkillInstallStatus.INSTALLABLE:
        return [INSTALL_CHOICE, BACK_CHOICE, EXIT_CHOICE]
    return [UPDATE_CHOICE, REMOVE_CHOICE, BACK_CHOICE, EXIT_CHOICE]


def downloadable_skill_matches(
    skills: list[Skill], installed_skills: list[Skill]
) -> list[DownloadableSkillMatch]:
    repository = SkillRepository()
    return [
        DownloadableSkillMatch(
            available=skill,
            installed=repository.match_installed(
                installed_skills,
                skill,
                candidate_filter=is_download_source_skill,
            ),
        )
        for skill in skills
    ]


def is_download_source_skill(skill: Skill) -> bool:
    return skill.is_github() or skill.is_skillsmp()
