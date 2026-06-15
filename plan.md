# Implementation Plan: Zensical Documentation Site

Issue: <https://github.com/xelandernt/skilly/issues/3>

## Overview

Create a version-controlled documentation site with
[Zensical](https://zensical.org/) that gives users a clear path from
installation to CLI and Python API usage, and publishes automatically to GitHub
Pages.

The current `README.md` already contains the raw material for installation,
CLI workflows, dependency scanning, the Python API, and development. The site
should reorganize and expand that material rather than invent a second,
conflicting contract. After this work, the README remains the concise package
and repository landing page, while `docs/` becomes the authoritative location
for detailed user and API documentation.

## Affected Contract

- **New public interface:** the documentation site's navigation, URLs, examples,
  and descriptions become observable user-facing behavior.
- **Existing public interfaces documented:** the native CLI, Python API,
  destination/configuration behavior, dependency source behavior, SkillsMP, and
  GitHub integration.
- **External boundary:** GitHub Pages hosts the built static site; Zensical is a
  development/build dependency only.
- **Runtime behavior:** unchanged. The site build must not require importing the
  compiled PyO3 extension or executing project code.
- **Compatibility:** additive and non-breaking. Existing README anchors may be
  depended on, so retain the concise installation and quick-start sections while
  adding links to the detailed site instead of deleting the README wholesale.

## Success Criteria

- A contributor can run one documented command to preview the site and one
  command to perform a strict production build.
- The site has a stable, explicit navigation structure covering installation,
  core concepts, CLI workflows, Python API, and contributing.
- CLI documentation agrees with `skilly <command> --help`, including
  interactive and non-interactive behavior.
- Python documentation covers the supported exports from `skilly.__all__` and
  `skilly.skillsmp`, without presenting `_core`, `_bridge`, or Rust internals as
  public API.
- Pull requests fail when the Zensical site does not build, and pushes to
  `main` publish the site to GitHub Pages.
- `just lint`, `just test`, and `just typecheck` pass after the documentation
  work.

## Architecture Decisions

### Documentation Source and Build

- Add `zensical.toml` at the repository root and keep authored Markdown under
  `docs/`.
- Add Zensical to a dedicated `docs` dependency group in `pyproject.toml`, with
  the resolved version committed in `uv.lock`. Zensical is pre-1.0, so use a
  compatible bounded range rather than an unbounded minimum.
- Add `just docs-serve` and `just docs-build` recipes so local and CI builds use
  the same commands.
- Build into `site/` and ignore that generated directory. Do not commit built
  HTML.

### Information Architecture

Use task-oriented guides for workflows and manually curated reference pages for
public interfaces:

```text
Home
Getting started
  Installation
  Core concepts
CLI
  Command reference and automation
  Creating skills
  Installing and managing skills
  Dependency scanning
  Destinations and configuration
Python API
  Overview
  Skills and resources
  Repository and project sources
  SkillsMP client
  Custom filesystems
Contributing
```

Keep URLs and page titles explicit in `zensical.toml` navigation. Avoid
auto-generated API pages in the first version: most public objects do not yet
have complete docstrings, and importing `skilly.Skill` during a docs build
would couple documentation deployment to building the Rust extension.

### Sources of Truth

- CLI names, options, defaults, and exit behavior: `src/cli/args.rs` and
  verified `skilly ... --help` output.
- Python public surface: `src/skilly/__init__.py`,
  `src/skilly/skillsmp/__init__.py`, and their typed signatures.
- Behavioral details and examples: tests plus the existing README.
- Agent Skill format and validation rules: link to the Agent Skills
  specification instead of duplicating it.

When documentation work exposes an existing disagreement, update the smallest
public contract needed to make behavior, help text, and docs agree. For example,
the current `scan` help mentions Python and Node even though the checked-out
implementation also has Maven support.

### Publishing

- Add one GitHub Actions workflow that builds docs for pull requests and pushes
  to `main`.
- Give the build job read-only repository permissions.
- Run the Pages deployment job only for pushes to `main`, with the minimum
  `pages: write` and `id-token: write` permissions and the `github-pages`
  environment.
- Set the canonical site URL to `https://xelandernt.github.io/skilly/` and the
  repository link to `https://github.com/xelandernt/skilly`.

## Dependency Graph

```text
Zensical dependency + configuration + just recipes
    |
    +-- Site navigation and shared conventions
    |       |
    |       +-- Getting-started and concept pages
    |       +-- CLI workflow/reference pages
    |       +-- Python API pages
    |
    +-- Strict local documentation build
            |
            +-- Pull-request validation
            +-- GitHub Pages deployment
            +-- README links to published documentation
```

## Task List

### Task 1: Establish the Zensical build contract

**Description:** Add the smallest reproducible Zensical toolchain and site
configuration. Define metadata, canonical URLs, explicit navigation, Markdown
features, local preview, production build, and generated-output handling before
writing content.

**Acceptance criteria:**

- [ ] `zensical.toml` defines the site name, description, canonical site URL,
      repository link, `docs/` source directory, `site/` output directory, and
      the complete initial navigation tree.
- [ ] Zensical is isolated in a `docs` dependency group and locked in
      `uv.lock`; runtime Python, Rust, and npm packages do not gain a docs
      dependency.
- [ ] `just docs-serve` previews the authored site and `just docs-build`
      performs a clean, strict production build.
- [ ] Generated `site/` content is ignored and never treated as source.

**Verification:**

- [ ] Run `just docs-build`.
- [ ] Run `git status --short` and confirm no generated site files are tracked.
- [ ] Start `just docs-serve` and confirm the home page and navigation load.

**Dependencies:** None

**Files likely touched:**

- `zensical.toml`
- `pyproject.toml`
- `uv.lock`
- `justfile`
- `.gitignore`

**Estimated scope:** Medium, 5 files

### Task 2: Create the onboarding documentation slice

**Description:** Build the first complete user journey: understand what skilly
does, choose an installation method, install it, create or download a skill,
and understand where managed skills are stored. Keep the home page concise and
move detailed explanations out of the README.

**Acceptance criteria:**

- [ ] The home page states skilly's purpose, supported interfaces, and primary
      workflows, with links to the Agent Skills specification.
- [ ] Installation documents the capability differences between `uvx`/PyPI,
      `npx`, and Homebrew, including supported platforms.
- [ ] Getting started contains a tested end-to-end CLI path and explicitly
      distinguishes interactive terminal behavior from automation.
- [ ] Core concepts explain a skill, resources, managed metadata, origins,
      statuses, and destinations using current skilly terminology.

**Verification:**

- [ ] Run every onboarding command example against a temporary destination.
- [ ] Run `just docs-build`.
- [ ] Manually check that all onboarding pages are reachable from the primary
      navigation and have no broken internal links.

**Dependencies:** Task 1

**Files likely touched:**

- `docs/index.md`
- `docs/getting-started.md`
- `docs/installation.md`
- `docs/core-concepts.md`

**Estimated scope:** Medium, 4 files

### Checkpoint: Site foundation and onboarding

- [ ] A fresh checkout can install the docs group and run `just docs-build`.
- [ ] A new user can follow the onboarding path without relying on unstated TTY,
      Python import, or filesystem assumptions.
- [ ] Navigation names and URLs are suitable to keep stable.

### Task 3: Document the complete CLI workflow

**Description:** Turn the CLI's supported commands into task-oriented guidance
and a compact command reference. Document plain non-TTY usage as the baseline,
then explain TUI behavior where applicable. Correct only the stale public help
or tests discovered while verifying the documentation.

**Acceptance criteria:**

- [ ] Every top-level command and nested SkillsMP/utility command is listed with
      its purpose, important options, interactive behavior, automation behavior,
      and a link to the relevant workflow guide.
- [ ] Skill creation, GitHub/SkillsMP installation, listing, updating, removing,
      dependency scanning, destinations, and configuration each have copyable
      examples.
- [ ] Dependency scanning documents Python, Node, and Maven sources, defaults,
      selection controls, limitations, and source/status output.
- [ ] CLI docs, `--help` output, and CLI tests agree; stale descriptions exposed
      by this work are corrected without changing command behavior.

**Verification:**

- [ ] Run `uvx skilly --help` and relevant
      `uvx skilly <command> --help` checks for every documented command group.
- [ ] Run focused CLI tests: `uv run pytest tests/test_cli.py`.
- [ ] Run `just docs-build`.

**Dependencies:** Tasks 1 and 2

**Files likely touched:**

- `docs/cli/index.md`
- `docs/cli/workflows.md`
- `docs/cli/dependency-scanning.md`
- `docs/cli/destinations-and-configuration.md`
- `src/cli/args.rs` only if contract discrepancies are confirmed; keep any
  focused help-text test beside the existing Rust CLI tests in that file

**Estimated scope:** Medium, at most 5 files

### Task 4: Document the supported Python API

**Description:** Define the Python API contract around the public package
exports. Use hand-authored signatures and examples so the site build remains
independent of the compiled extension, while clearly distinguishing stateful
`SkillRepository` workflows from focused stateless discovery functions.

**Acceptance criteria:**

- [ ] The API overview states compatibility expectations and identifies the
      supported `skilly` and `skilly.skillsmp` import surfaces.
- [ ] Skills/resources documentation covers `Skill`, `SkillOrigin`,
      `SkillResource`, resource kinds, parsing/installation, and discovery
      functions without exposing `_core` or `_bridge`.
- [ ] Repository documentation covers `SkillRepository`, `ProjectSettings`,
      `PythonSource`, `NodeSource`, `MavenSource`, matches, updates, and
      filesystem-independent operation.
- [ ] SkillsMP and custom filesystem pages document typed contracts, boundary
      validation expectations, error behavior, and complete minimal examples.
- [ ] All documented symbols and signatures agree with exports and type stubs.

**Verification:**

- [ ] Compare documented symbols against `skilly.__all__` and
      `skilly.skillsmp.__all__`.
- [ ] Run Python examples as focused tests or executable snippets after
      `maturin develop`.
- [ ] Run `just docs-build` without first building or importing the extension.
- [ ] Run `just typecheck`.

**Dependencies:** Tasks 1 and 2

**Files likely touched:**

- `docs/python/index.md`
- `docs/python/skills-and-resources.md`
- `docs/python/repository-and-sources.md`
- `docs/python/skillsmp.md`
- `docs/python/custom-filesystems.md`

**Estimated scope:** Medium, 5 files

### Checkpoint: User and API contract coverage

- [ ] Every documented command and Python symbol maps to an existing public
      interface.
- [ ] Examples use supported import paths and current CLI options.
- [ ] The site does not expose internal Rust, PyO3, bridge, or TUI
      implementation details as user contracts.
- [ ] `just docs-build`, focused CLI tests, and `just typecheck` pass.

### Task 5: Document contribution and documentation maintenance

**Description:** Give contributors a concise operating guide for the repository
and for keeping docs synchronized with public interface changes. This page
should point to existing commands and boundaries rather than duplicate
`AGENTS.md`.

**Acceptance criteria:**

- [ ] Contributors can install dependencies, run the project, preview/build
      docs, and execute all quality gates from the contribution page.
- [ ] The page explains that public CLI/Python changes require corresponding
      docs, help/type-stub updates, and examples in the same change.
- [ ] Documentation authoring conventions identify sources of truth, stable
      link expectations, and the rule against committing generated `site/`
      output.

**Verification:**

- [ ] Follow the documented setup and docs commands from a clean checkout or
      equivalent clean environment.
- [ ] Run `just docs-build`.

**Dependencies:** Tasks 1, 3, and 4

**Files likely touched:**

- `docs/contributing.md`
- `docs/documentation.md`

**Estimated scope:** Small, 2 files

### Task 6: Validate and publish the documentation site

**Description:** Add continuous documentation validation and GitHub Pages
deployment, then reduce the README to a durable project landing page that
points readers to the site without breaking useful existing entry points.

**Acceptance criteria:**

- [ ] A pull request runs a clean Zensical build using the locked docs
      dependency set.
- [ ] A push to `main` deploys only a successfully built `site/` artifact to the
      `github-pages` environment with minimum required permissions.
- [ ] The README retains installation, a short quick start, package badges, and
      development commands, while linking to the published guides and API
      reference for details.
- [ ] README, Zensical metadata, package metadata used by the docs, and workflow
      URLs consistently identify `xelandernt/skilly` as the canonical
      repository.

**Verification:**

- [ ] Run the workflow's build commands locally.
- [ ] Validate the workflow syntax and inspect job-level permissions/triggers.
- [ ] Open the deployed site and confirm the canonical URL, repository link,
      navigation, and representative CLI/Python pages.
- [ ] Run all repository quality gates: `just lint`, `just test`, and
      `just typecheck`.

**Dependencies:** Tasks 1 through 5

**Files likely touched:**

- `.github/workflows/docs.yml`
- `README.md`
- `zensical.toml`

**Estimated scope:** Medium, 3 files

### Checkpoint: Complete

- [ ] All success criteria are met.
- [ ] The production site builds from a fresh checkout using locked
      dependencies.
- [ ] Pull-request validation and `main` deployment are both proven.
- [ ] `just lint`, `just test`, and `just typecheck` pass.
- [ ] Generated documentation output is absent from the git diff.
- [ ] The site is ready for review and issue #3 can be closed.

## Parallelization Opportunities

- After Task 1 fixes the navigation and writing conventions, Tasks 3 and 4 can
  proceed in parallel because the CLI and Python contracts are independent.
- Task 5 can begin after the local docs commands are stable, but its final
  contract-maintenance section should be reviewed after Tasks 3 and 4.
- Task 6 must remain last because its README links, workflow, and deployment
  depend on the complete site structure and a passing production build.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Documentation drifts from CLI help or Python exports | High | Treat `src/cli/args.rs`, public exports, stubs, and focused tests as sources of truth; verify them while authoring |
| Docs build depends on a compiled native extension | High | Use hand-authored API reference pages and verify `just docs-build` without `maturin develop` |
| README and site duplicate detailed content | Medium | Keep README as a concise landing page and make detailed guides authoritative in `docs/` |
| Pre-1.0 Zensical changes break builds | Medium | Use a bounded docs dependency and commit its resolved version in `uv.lock` |
| GitHub Pages workflow gains excessive permissions | Medium | Separate build/deploy jobs and grant Pages permissions only to the conditional deploy job |
| Existing public descriptions disagree with current Maven behavior | Medium | Correct confirmed stale help/docs in the smallest focused change and cover it with CLI tests |
| Stable documentation links break during later reorganization | Low | Declare navigation explicitly and avoid renaming published paths without redirects or a migration decision |

## Resolved Scope Decisions

- The first release is an English-only static site.
- No generated API reference, versioned docs, search service, analytics,
  custom domain, blog, or release-notes system is included.
- Rust internals and private Python bridge APIs are not documented as supported
  public interfaces.
- Documentation examples target non-interactive automation first and describe
  TUI enhancements separately.
- Site publication targets GitHub Pages for
  `https://github.com/xelandernt/skilly`.

## References

- Issue #3: <https://github.com/xelandernt/skilly/issues/3>
- Zensical repository: <https://github.com/zensical/zensical>
- Zensical documentation: <https://zensical.org/docs/get-started/>
- Agent Skills specification: <https://agentskills.io/specification>
