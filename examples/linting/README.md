# Linting Test Examples

Canonical linting test cases for `.actions` files — one directory per lint rule, each a paired before/after that both documents the rule and drives the CLI's snapshot tests.

## Purpose

1. **Specification artifacts** — executable demonstrations of the rules defined in [the linting specification](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/linting.md).
2. **Implementation tests** — consumed by `tests/linting_conformance.rs` to verify detection (on `error.actions`) and clean passes (on `fixed.actions`).
3. **Documentation** — show what each rule catches and how to fix it.

## Structure

Each case is a directory named `<CODE>_<slug>` (e.g. `E006_invalid_uuid`, `W013_incomplete_uuid`) containing two files:

- `error.actions` — parses successfully but contains 2–4 instances of one rule's violation.
- `fixed.actions` — the same actions corrected so that rule no longer fires.

The directory name *is* the rule reference. For what each code means — its severity, rationale, and whether it's fixable — see the canonical catalog in [the linting specification](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/linting.md). It is not duplicated here on purpose: a second hand-maintained table only drifts.

## Notes

**Examples use valid syntax, not syntax errors.** Linting runs *after* parsing, so every `error.actions` must parse. The grammar is deliberately permissive (relaxed parser / strict linter) so that near-misses reach the linter as data:

- `#123` parses as an id, but the linter flags it — invalid UUID (`E006`).
- `#01950000-0000-7000-8000` parses too, but reads as a uuid still being typed — incomplete UUID (`W013`).
- `!6` is a valid number but an out-of-range priority (`I003`).

**One rule per file.** Every violation in an `error.actions` is an instance of the directory's rule; cases don't mix rules.

**Configurable thresholds** (e.g. excessive-duration minutes) use their defaults here unless a comment in the file says otherwise.

## Running

```bash
# From clearhead-cli/
cargo test --test linting_conformance
```

`test_linting_error_detection` asserts each `error.actions` produces its rule's code (and snapshots the full diagnostics); `test_linting_fixed_version_passes` asserts each `fixed.actions` does not.

## Adding a case

1. Name the directory `<CODE>_<slug>` for a rule in `specifications/linting.md`.
2. Add `error.actions` (2–4 violations of that one rule) and `fixed.actions` (the same actions corrected).
3. If the code is newly implemented, add it to `IMPLEMENTED_LINT_RULES` in `tests/linting_conformance.rs`, then run the suite to record the snapshot.
