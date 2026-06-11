from enum import Enum
from pathlib import Path
from typing import Final


class _StrEnum(str, Enum):
    def __str__(self) -> str:
        return str(self.value)


DEFAULT_SKILLS_PATH: Final[Path] = Path(".agents/skills")
DEFAULT_VENV_PATH: Final[Path] = Path(".venv")
DEFAULT_PYPROJECT_PATH: Final[Path] = Path("pyproject.toml")


class SkillInstallStatus(_StrEnum):
    INSTALLED = "installed"
    INSTALLABLE = "installable"
    UPDATABLE = "updatable"
