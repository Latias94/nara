---
type: "Verification Evidence"
title: "RGD-U5 real three-host semantic parity verification"
description: "Verifies one bounded reference-game semantic command stream through public Headless, Desktop, and Editor product Hosts."
timestamp: 2026-07-22T21:58:22Z
record_id: "481fd3d8ac1e43b09fc91d38f47062da"
tags: ["rgd-u5", "reference-game", "host-parity", "headless", "desktop", "editor"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0c2dadd849e08d5b3f22dcedac963bbfccf08595"
verified_by: "Focused nextest integration tests, source-boundary audit, bounded child-process lifecycle checks, formatting, and staged-scope audit"
---

# Verification

RGD-U5 was verified against implementation commit `0c2dadd849e08d5b3f22dcedac963bbfccf08595`.
The proof uses the reference game's normal public product entry points rather than direct runtime
construction: `HeadlessRun`, Winit-backed `DesktopRun`, and `EditorProjectSession` each receive the
same bounded semantic gameplay submissions in an isolated fixture process.

# Result

- The three public Hosts produce the same bounded, stable-ID-sorted `WaveSnapshot` envelope at the
  same fixed tick for accepted, duplicate, too-far-future, oversized, and late semantic commands.
- Each path publishes the actual runtime fault identity from its Host diagnostic. The envelope records
  both `fault-kind` and `fault-source`; it does not hard-code a fault description that could mask a
  Host-specific regression.
- The Desktop fixture enters the real Winit-backed `DesktopRun` from its binary `main` function. Its
  oracle requires exactly one `project.desktop.runner-failed` diagnostic with
  `reason = managed-runtime`, so teardown noise cannot be mistaken for semantic parity.
- The parent oracle owns no raw runtime `World`, driver scope, or private Host type. Its source audit
  rejects those shortcuts, generic observation buses, and oversized release-evidence output.
- Fixture children receive randomized working-directory and home paths. Their stdout and stderr are
  drained through bounded pipes; a capture-limit failure kills and reaps the child instead of writing
  unbounded output to disk.
- Runtime scope and Desktop runner diagnostics preserve the original `RuntimeFault` kind/source across
  Headless, Desktop, and Editor failure reporting.

# Authority Matrix

| Authority | U5 treatment | Evidence |
|---|---|---|
| Semantic command stream | Shared bounded reference-game input | Every child injects identical stable keys, sources, ticks, and sequences through public gameplay ingress. |
| Product Host construction | Host-owned public API | The probe selects only `HeadlessRun`, `DesktopRun`, or `EditorProjectSession`; it cannot construct a managed runtime directly. |
| Snapshot observation | Test-owned bounded sink | A reference-game-owned plugin emits an existing stable snapshot into one bounded canonical envelope. |
| Fault classification | Host-owned diagnostic plus runtime identity fields | Child envelopes capture the actual `fault-kind` and `fault-source` fields from the expected Host diagnostic. |
| Child lifecycle | Parent test ownership | Randomized environment, deadline, bounded pipe capture, kill, and reap stay in the test helper rather than engine runtime code. |
| Winit main-thread ownership | Desktop child binary | The real Desktop runner starts from `host_parity_probe` binary `main`; no test thread emulates the platform loop. |

# Evidence

- `cargo nextest run --locked --features host-parity --test host_parity --test-threads=1` in
  `reference-game`: 2 passed.
- `cargo nextest run --locked --features host-parity --test desktop_render --test-threads=1` in
  `reference-game`: 3 passed.
- `cargo nextest run --locked --features 'runtime-2d serde tooling' --lib
  runtime_scope_failure_report_preserves_fault_identity --test-threads=1` at the root: 1 passed.
- `desktop_runner_failure_preserves_managed_runtime_fault_identity` passed with the supported root
  desktop feature profile.
- `rustfmt --edition 2024` completed for the U5-owned Rust files, and `git diff --check` passed before
  the implementation commit.
- `architecture_docs` was intentionally not run. It is not a U5 focused gate, and the user explicitly
  excluded it from the ordinary verification loop.

# Follow-up

1. RGD-U6 must prove that a locked external package with a renamed root dependency can own a concrete
   managed-runtime loop using only public APIs, without adding a Nara-owned Runner SPI.
2. The U5 source audit is a targeted regression guard for known shortcuts, not a transitive call-graph
   proof; U6 must maintain its own independent metadata and source-surface oracle.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u5-prove-real-three-host-semantic-parity`
- `reference-game/tests/host_parity.rs`
- `reference-game/tests/support/child_process.rs`
- `reference-game/src/bin/host_parity_probe.rs`
- Commit `0c2dadd849e08d5b3f22dcedac963bbfccf08595`
