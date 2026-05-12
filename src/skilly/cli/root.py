from __future__ import annotations

from pathlib import Path

from cyclopts import App

from skilly.cli.choices import (
    BACK_CHOICE,
    EXIT_CHOICE,
    INSTALL_CHOICE,
    UPDATE_CHOICE,
)
from skilly.cli.previews import installed_skill_preview_lines, scan_match_preview_lines
from skilly.cli.shared import (
    exit_menu_item,
    installed_skill_actions,
    installed_skill_menu_items,
    update_skill,
)
from skilly.cli.skillsmp import download as skillsmp_download
from skilly.cli.skillsmp import skillsmp_cli
from skilly.cli.ui import Menu, MenuItem, cli_ui
from skilly.cli.util import util_cli
from skilly.constants import DEFAULT_SKILLS_PATH, SkillInstallStatus
from skilly.repository import SkillMatch, SkillRepository
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

    actionable = [
        match for match in matches if match.status is not SkillInstallStatus.INSTALLED
    ]
    if not actionable:
        print("All discovered dependency skills are already installed")
        return

    messages: list[str] = []
    status_message: str | None = None
    while actionable:
        match = cli_ui.select(
            Menu(
                title="Select dependency skill to install",
                items=tuple(
                    [*scan_menu_items(actionable), exit_menu_item("Exit scan")]
                ),
                default=actionable[0],
                preview_title="Dependency skill",
                status=status_message,
            )
        )
        if match is None or match == EXIT_CHOICE:
            break

        action = cli_ui.select(
            Menu(
                title=f"Choose an action for {match.available.name}",
                items=tuple(
                    MenuItem(
                        value=item,
                        label=item,
                        preview_lines=scan_match_preview_lines(match),
                    )
                    for item in scan_skill_actions(match)
                ),
                default=scan_primary_action(match),
                preview_title=match.available.name,
                status=status_message,
            )
        )
        if action in {None, BACK_CHOICE}:
            continue
        if action == EXIT_CHOICE:
            break

        installed = repository.install(
            match.available,
            directory=directory,
            skill_name=match.installed.directory_name if match.installed else None,
            replace=match.installed is not None,
        )
        actionable = [item for item in actionable if item != match]
        if match.installed is None:
            status_message = (
                f"Installed {installed.directory_name} to {installed.directory}"
            )
        else:
            status_message = (
                f"Updated {installed.directory_name} to "
                f"{installed.package_version or 'unknown'}"
            )
        messages.append(status_message)

    if messages:
        print("\n".join(messages))


@cli.command()
def download(
    github_url: str,
    directory: Path = DEFAULT_SKILLS_PATH,
    skill_name: str | None = None,
    all: bool = False,
    overwrite: bool = False,
    github_token: str | None = None,
) -> None:
    """Download a skill using its github url."""
    skillsmp_download(
        github_url,
        directory=directory,
        skill_name=skill_name,
        all=all,
        overwrite=overwrite,
        github_token=github_token,
    )


@cli.command()
def list(
    directory: Path = DEFAULT_SKILLS_PATH, github_token: str | None = None
) -> None:
    """List installed skills."""
    repository = SkillRepository()
    github_client = SkillsMp(github_token=github_token)
    messages: list[str] = []
    status_message: str | None = None

    while True:
        skills = repository.list(directory)
        if not skills:
            if messages:
                break
            print(f"No installed skills found in {directory.resolve()}")
            return

        skill = cli_ui.select(
            Menu(
                title="Select an installed skill",
                items=tuple(
                    [*installed_skill_menu_items(skills), exit_menu_item("Exit list")]
                ),
                default=skills[0],
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
                    for item in installed_skill_actions(skill)
                ),
                default=installed_skill_actions(skill)[0],
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
                github_client,
                directory=directory,
                skill=skill,
            )
            messages.append(status_message)
            continue
        removed = repository.remove(skill.directory_name, directory=directory)
        status_message = f"Removed {removed.directory_name}"
        messages.append(status_message)

    if messages:
        print("\n".join(messages))


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


def scan_menu_items(matches: list[SkillMatch]) -> list[MenuItem[SkillMatch | str]]:
    return [
        MenuItem(
            value=match,
            label=scan_choice_label(match),
            preview_lines=scan_match_preview_lines(match),
        )
        for match in matches
    ]


def scan_primary_action(match: SkillMatch) -> str:
    return UPDATE_CHOICE if match.installed is not None else INSTALL_CHOICE


def scan_skill_actions(match: SkillMatch) -> list[str]:
    return [scan_primary_action(match), BACK_CHOICE, EXIT_CHOICE]
