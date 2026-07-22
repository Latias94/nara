---
type: "Verification Evidence"
title: "RGD-U2 frozen component behavior authority verification"
description: "Verifies one executable component registry snapshot across composition, Editor, candidate startup, and managed runtime safe points."
timestamp: 2026-07-22T07:59:03Z
record_id: "84188e9196d242078d5e32c6368f7ca6"
tags: ["rgd-u2", "registry", "runtime", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5c06cdeb015612bfbe0feb93f21d8fe3e7603116"
verified_by: "Focused nextest, workspace check, source boundary audit, and independent correctness review"
---

# Verification

RGD-U2 was verified against implementation commit
`5c06cdeb015612bfbe0feb93f21d8fe3e7603116`. File-backed composition, Editor
authoring, candidate startup, and published managed runtimes now share one exact executable
`ComponentRegistrySnapshot`; code-first Apps retain one guarded World-owned construction path.

# Result

- `SchemaValidationInput` owns the frozen snapshot and stable provider receipts. Product candidate
  plugins validate that authority instead of rebuilding executable behavior.
- Candidate admission and every managed runtime schedule validate the registry instance and
  mutation revision before and after the safe point. Removal, replacement, and same-snapshot
  rewrapping become an owning-runtime sticky `RuntimeAuthority` fault.
- Faulted, stopping, and close-incomplete runtimes retain their typed retirement ports without
  reopening generic managed authority.
- Editor runtime edits use a scoped bridge with unwind restoration, while Editor documents and
  models continue to read the immutable plan snapshot without a Play runtime.
- `ProjectContentSnapshot` remains World-independent and contains no native binding, codec,
  migration, function pointer, fault reporter, or Host trust token.
- Independent correctness review reported no remaining P0/P1. Per-runtime Bevy failure attribution
  remains intentionally owned by RGD-U3.

# Evidence

- `cargo check --workspace --locked`: passed.
- Directed `rustfmt --edition 2024 --check` over every U2 Rust file: passed.
- `cargo nextest run --locked -p nara --features serde,runtime-2d --test
  runtime_driver_boundary --test-threads=1`: 8 passed, run
  `405127f1-e84d-4c72-b89c-abfeb089f2a1`.
- Focused root library, runtime, composition, content, Host, Editor, Play, and module-consumer gates
  passed. The final `workspace_play_runtime` rerun was 10/10 after scheduler-safe test polling in
  commit `c2a3905737f0fb766a6dd02ec11c706e3996823c`.
- `cargo nextest run --offline --locked -p nara_reflect -p nara_scene --test-threads=1`:
  136 passed, run `b01ef391-7b18-438b-ae65-1e1b8a779d97`.
- From `reference-game/`, focused `authoring`, `plugin_composition`, and `runtime_drive` nextest
  suites: 12 passed, run `4f6b4644-dc10-44cd-accb-9aa99d41eb21`.
- `git diff --cached --check` passed before the implementation commit; the staged scope contained
  exactly the 24 U2 implementation and regression files.

# Follow-up

1. Activate RGD-U3 and replace the process-global Bevy fallback route with bounded per-runtime
   attribution without weakening multithreaded execution.
2. Preserve U2's exact registry-instance guard while U3 changes fault routing; a peer fault must not
   alter this runtime's reporter or registry authority.
3. Keep hierarchy and unavailable-schema authoring as Proposed, trigger-gated follow-up slices.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u2-publish-one-frozen-component-behavior-authority`
- `docs/architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md`
- `AGENTS.md`
- Commit `5c06cdeb015612bfbe0feb93f21d8fe3e7603116`
