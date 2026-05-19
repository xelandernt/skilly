from enum import Enum
from pathlib import Path
from typing import Final
from typing_extensions import deprecated


@deprecated("Remove in python 3.11")
class _StrEnum(str, Enum):
    def __str__(self) -> str:
        return str(self.value)


DEFAULT_SKILLS_PATH: Final[Path] = Path(".agents/skills")


class SkillInstallStatus(_StrEnum):
    INSTALLED = "installed"
    INSTALLABLE = "installable"
    UPDATABLE = "updatable"
