from pathlib import Path
from typing import Final, Literal, TypeAlias
from enum import Enum

try:
    from enum import StrEnum
except ImportError:

    class StrEnum(str, Enum):
        def __str__(self) -> str:
            return str(self.value)


DEFAULT_SKILLS_PATH: Final[Path] = Path(".agents/skills")
ResourceKind: TypeAlias = Literal["script", "reference", "asset", "other"]
RESOURCE_KIND_SCRIPT: Final[ResourceKind] = "script"
RESOURCE_KIND_REFERENCE: Final[ResourceKind] = "reference"
RESOURCE_KIND_ASSET: Final[ResourceKind] = "asset"
RESOURCE_KIND_OTHER: Final[ResourceKind] = "other"


class SkillInstallStatus(StrEnum):
    INSTALLED = "installed"
    INSTALLABLE = "installable"
    UPDATABLE = "updatable"


SKILLY_MANAGED_METADATA_KEY = "skilly-managed-by"
SKILLY_MANAGED_METADATA_VALUE = "skilly"
SKILLY_SOURCE_METADATA_KEY = "skilly-source"
SKILLY_SOURCE_DEPENDENCY = "dependency"
SKILLY_SOURCE_GITHUB = "github"
SKILLY_SOURCE_SKILLSMP = "skillsmp"
SKILLY_UNKNOWN_SOURCE = "unknown"
SKILLY_GITHUB_URL_METADATA_KEY = "skilly-github-url"
SKILLY_SKILLSMP_ID_METADATA_KEY = "skilly-skillsmp-id"
SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY = "skilly-package-name"
SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY = "skilly-package-version"
