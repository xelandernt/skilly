# AGENTS.md

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- Identify the affected contract first: Rust core, CLI, Python API, custom
  filesystem protocol, or external SkillsMP/GitHub boundary.
- Treat the [Agent Skills specification](https://agentskills.io/specification)
  as authoritative for `SKILL.md` fields, names, resources, and validation
  rules.
- State whether a public API change is breaking. Preserve compatibility unless
  the user explicitly allows breaking changes; when breaking changes are
  allowed, remove the old surface instead of maintaining parallel APIs.
- Confirm whether a CLI path must support both interactive terminals and
  non-interactive automation. Never assume a TTY is available.
- For filesystem work, reason about native and custom filesystems, path
  traversal, partial writes, stale resources, and rollback before coding.
- If behavior could live in Rust or Python, default to Rust core logic with a
  thin Python binding. Explain any exception.
- If multiple interpretations remain, present them and ask rather than choosing
  silently.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- Implement domain behavior once in `src/core.rs`. Native filesystem entry
  points and Python/custom-filesystem entry points should delegate to the same
  generic implementation.
- Keep `src/lib.rs` as a binding layer, `src/skilly/_bridge.py` as a typed
  adapter, and `src/skilly/repository.py` as orchestration. Do not duplicate
  parsing, matching, scanning, installation, or update logic across layers.
- Prefer one obvious public interface. Do not add response wrappers, aliases,
  convenience utilities, or configuration paths that duplicate an existing
  capability.
- Use `SkillRepository` for stateful workflows and focused discovery functions
  for stateless reads.
- Keep CLI flows small: shared destination resolution, shared core operations,
  explicit TTY/non-TTY branches, and no terminal UI when plain output is enough.
- Do not add abstractions for a single use or options without a demonstrated
  caller.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Follow the existing layer boundaries. A core behavior change may require the
  corresponding Rust binding, Python stub, bridge, wrapper, and tests; that is
  one coherent change, not unrelated cleanup.
- Update public documentation and type stubs in the same change as a public CLI
  or Python API change.
- When extending `FileSystem`, update every implementation and test double.
- When changing install/update behavior, preserve these invariants:
  - validate skill names and resource paths before writes;
  - resources cannot escape the skill directory;
  - replacement removes stale files and preserves the old tree on failure;
  - malformed installed skills produce actionable diagnostics.
- Do not edit generated extension binaries or cache artifacts.
- Do not refactor adjacent code unless it is necessary to remove duplication
  introduced or exposed by the requested change.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Remove superseded public surfaces when the user explicitly approved a
  breaking cleanup.
- Don't remove unrelated pre-existing dead code; mention it instead.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Translate work into repository-specific observable outcomes:
- Validation change -> invalid input is rejected before any filesystem write.
- Install/update change -> native and custom filesystem tests prove the same
  behavior, including stale-file removal and failure safety.
- CLI change -> help text, interactive behavior, and non-TTY behavior are
  covered as applicable.
- Scan/update change -> dependency selection and matching behave identically
  through CLI, Rust, and Python entry points.
- Python API change -> runtime behavior, `_core.pyi`, exports, README examples,
  and type checking agree.
- Refactor -> tests pass before and after, and duplicated behavior is actually
  removed rather than wrapped.

For multi-step tasks, state a brief plan:
```
1. Define the contract and invariants → verify with focused tests
2. Implement in the lowest shared layer → verify all callers use it
3. Update interfaces and docs → verify help, stubs, and examples agree
4. Run repository quality gates → verify lint, tests, and types pass
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.


## 5. Verification

- Before handing work back, `just lint` must pass.
- Before handing work back, `just test` must pass.
- Before handing work back, `just typecheck` must pass.
- Run focused tests while iterating, then run the full gates.
- If a change affects the Rust/Python boundary, ensure `maturin develop` is run
  before Python behavior is considered verified; `just test` does this.
- If a change affects CLI behavior, verify relevant `uvx skilly <command>
  --help` output or CLI tests.

## 6. Python Typing

- Do not use `typing.cast()`.
- Do not use `# type: ignore` or similar suppression comments.
- If typing is awkward, fix the design or the annotations instead of hiding the problem.
- Prefer explicit type definitions, Protocols, generics, and overloads when they express the shape clearly.

## 7. Repository Shape

- `src/core.rs`: domain models, parsing, validation, filesystem-independent
  discovery, installation, scanning, and update logic.
- `src/cli.rs`: CLI parsing and user interaction, including TUI and plain
  non-TTY output.
- `src/client.rs`: blocking SkillsMP and GitHub transport with bounded requests.
- `src/lib.rs`: PyO3 bindings; keep domain decisions out of this layer.
- `src/skilly/_bridge.py` and `src/skilly/_core.pyi`: typed Python/Rust boundary.
- `src/skilly/repository.py`: stateful Python orchestration.
- `src/skilly/skillsmp/`: Pythonic SkillsMP client and typed public results.
- `tests/`: Python API, CLI, parser, repository, and custom filesystem coverage.
- `docs/`: detailed design contracts that are too specific for the README.

## 8. Handoff Standard

- Leave the tree in a working state.
- Summarize what changed and what was verified.
- Call out any remaining risks or unfinished follow-up work explicitly.
