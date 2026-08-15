---
type: "Verification Evidence"
title: "FPER-U2 startup-closure export conformance"
description: "Verifies exact Cargo artifact selection, startup-closure publication, portable executable permissions, independent consumption, and arbitrary-CWD headless and desktop acceptance."
timestamp: 2026-08-15T05:44:30Z
record_id: "64c7375e92e64f958174aa1bc37b09ee"
resource: "nara-first-party-project-workflow"
tags: ["project-workflow", "fresh-project", "fper-u2", "export", "cargo", "arbitrary-cwd"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0919c8b"
verified_by: "focused-nextest-independent-consumer-and-real-arbitrary-cwd-export"
---

# Verification

FPER-U2 is implemented at commits `9ee01e0`, `8b41faf`, `ae2863d` and `0919c8b`.
The public project-workflow Module consumes the checked `StartupContentClosureReceipt` and one
move-only Cargo build receipt, publishes a finite source-carrying directory through `nara_fs`, and
keeps Cargo JSON parsing, process execution and desktop acceptance private to the CLI adapter.

The final build-input contract verifies the closed Cargo and project inputs before and after the
locked build. It intentionally does not claim hostile non-cooperative compare-and-swap protection;
that remains outside the TrustedLocal workflow selected by ADR 0110.

# Result

- Exact Cargo artifact selection rejects ambiguous project binaries and binds headless and desktop
  artifacts to one package/profile/target build.
- The exported candidate contains only the two executable products, `nara.toml`, the admitted
  startup Scene, and the required empty runtime content directories. It makes no completeness claim
  about unreferenced or dynamically requested content.
- Executable permissions are preserved by the filesystem owner, and both real products ran from an
  unrelated working directory using executable-relative project discovery.
- The retained `candidate-4` completed acceptance and printed
  `starter_headless: ok fixed_ticks=1` and
  `starter_desktop: ok first_present_verified=true`.
- A prior published candidate that omitted required empty runtime directories remained visible as
  published-but-unaccepted; it was not reported as success or silently deleted.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check --locked --no-default-features --features project-workflow-cli --bin nara`
  - passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --no-default-features --features project-workflow-cli --bin nara -E 'test(exact_headless_and_desktop_artifacts_form_one_selection) | test(an_additional_project_binary_is_rejected)' --test-threads=1 --no-fail-fast`
  - passed, 2 tests.
- `CARGO_BUILD_JOBS=1 cargo check --manifest-path module-consumer/Cargo.toml --locked --offline -p nara-project-workflow-consumer --jobs 1`
  - passed; the independent consumer used the public export transaction without CLI, Cargo JSON,
    process or presentation types.
- `CARGO_BUILD_JOBS=1 cargo build --locked --no-default-features --features project-workflow-cli --bin nara`
  - passed against the final implementation.
- `target/debug/nara.exe export target/fper-u2-manual-187332-20260815044623/projects/fper-export --parent target/fper-u2-manual-187332-20260815044623/exports --destination candidate-4`
  - built the generated release products, published the startup closure, and completed real
    arbitrary-CWD headless and desktop acceptance.
- Direct `rustfmt` checks on the owned Rust files and `git diff --check`
  - passed before the implementation commits.

# Follow-up

Activate FPER-U3 for one typed missing-startup-Scene issue and one explicit stale-safe repair
transaction. Keep the workflow synchronous and deterministic; a future Editor may schedule it
through `nara_tasks`, but this baseline does not add an async runtime, exporter SPI, archive
framework or generic recovery system.

# Citations

- `src/project_workflow/export.rs`
- `src/project_workflow/export/build.rs`
- `src/project_workflow/export/stage.rs`
- `src/bin/nara/export.rs`
- `src/bin/nara/export/cargo.rs`
- `src/bin/nara/export/acceptance.rs`
- `crates/nara_fs/src/capability.rs`
- `module-consumer/project-workflow-consumer/src/lib.rs`
- Commits `9ee01e0`, `8b41faf`, `ae2863d` and `0919c8b`
