# AGENTS.md

## Working Rules

- Make the smallest useful change that solves the task.
- Keep changes incremental. Finish one logical step, verify it, then move on.
- If a cleanup seems useful but is outside the requested work, ask the user first.
- Prefer the repo's existing patterns and naming over introducing new abstractions.

## Verification

- Before handing work back, `just lint` must pass.
- Before handing work back, `just test` must pass.
- If a change affects types, build behavior, or public interfaces, run any additional checks that are clearly relevant before handoff.

## Python Typing

- Use clean typing only.
- Do not use `typing.cast()`.
- Do not use `# type: ignore` or similar suppression comments.
- If typing is awkward, fix the design or the annotations instead of hiding the problem.
- Prefer explicit type definitions, Protocols, generics, and overloads when they express the shape clearly.

## Repository Shape

- Rust core logic and CLI live under `src/`.
- Python-facing code and tests live alongside the Rust code and in `tests/`.

## Handoff Standard

- Leave the tree in a working state.
- Summarize what changed and what was verified.
- Call out any remaining risks or unfinished follow-up work explicitly.
