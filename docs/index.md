# skilly

Manage [Agent Skills](https://agentskills.io/specification) from the command line
or Python.

skilly creates specification-compliant skills, installs skills from GitHub,
Bitbucket Cloud, Bitbucket Data Center, and Python, Node, or Maven
dependencies, and keeps managed skills up to date.

## Supported interfaces

-   **CLI** — run `skilly` via `uvx`, `npx`, or Homebrew. Full set of commands
    for creating, installing, scanning, updating, and removing skills. Works in
    both interactive terminals and non-interactive automation.
-   **Python API** — import `skilly` for programmatic control. `SkillRepository`
    for stateful workflows, focused discovery functions for one-shot reads.

## Primary workflows

-   [Create a skill](cli/creating-skills.md) — define a new skill with metadata,
    instructions, and optional scripts or references.
-   [Scan dependencies](cli/dependency-scanning.md) — discover skills shipped by
    your project's Python, Node, and Maven dependencies.
-   [Install from a repository](cli/installing-and-managing.md) — download
    skills from GitHub, Bitbucket Cloud, or Bitbucket Data Center.
-   [Search SkillsMP](cli/installing-and-managing.md) — find and install skills
    from the SkillsMP registry.
-   [Update skills](cli/installing-and-managing.md) — check for updates to
    installed skills and apply them.
-   [Configure skilly](cli/destinations-and-configuration.md) — manage agent
    directories and repository credentials.

## Related resources

-   [Agent Skills specification](https://agentskills.io/specification) — the
    specification that installed skills must conform to.
-   [GitHub repository](https://github.com/xelandernt/skilly) — source code,
    issues, and contributions.
