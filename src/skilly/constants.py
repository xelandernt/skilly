from pathlib import Path
from typing import Final

DEFAULT_SKILLS_PATH: Final[Path] = Path(".agents/skills")
SKILLY_MANAGED_METADATA_KEY = "skilly-managed-by"
SKILLY_MANAGED_METADATA_VALUE = "skilly"
SKILLY_SOURCE_METADATA_KEY = "skilly-source"
SKILLY_SOURCE_DEPENDENCY = "dependency"
SKILLY_SOURCE_SKILLSMP = "skillsmp"
SKILLY_UNKNOWN_SOURCE = "unknown"
SKILLY_GITHUB_URL_METADATA_KEY = "skilly-github-url"
SKILLY_SKILLSMP_ID_METADATA_KEY = "skilly-skillsmp-id"
SKILLY_DEPENDENCY_PACKAGE_NAME_METADATA_KEY = "skilly-package-name"
SKILLY_DEPENDENCY_PACKAGE_VERSION_METADATA_KEY = "skilly-package-version"
