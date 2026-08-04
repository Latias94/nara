---
type: "Verification Evidence"
title: "NUI-U2 Host-owned Editor workspace"
description: "Verifies removal of runtime tooling installation while preserving Host-owned persistence, Play, Retry, and adapter command behavior."
timestamp: 2026-08-04T14:24:26Z
record_id: "4bfdea95f38e4b6da30c4f16cc66562a"
tags: ["nui-u2", "editor", "tooling", "host-authority", "verification"]
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "92ee7f99603f363cfa568d947f721984cff6e996"
verified_by: "focused nextest suites, reference-game Host parity, compile-fail API proof, affected-library check, strict changed-target Clippy, rustfmt, stale-symbol inspection, and independent review"
---

# Verification

NUI-U2 landed at `92ee7f99603f363cfa568d947f721984cff6e996`.

- `EditorProjectSession` owns the live `EditorWorkspace` and paired persistence authority. The
  workspace is ordinary Host state and cannot be inserted into an ECS `World` as a `Resource`.
- `ToolingPlugin`, its plugin identity, slot, group, root exports, and project capability claims are
  deleted. Legacy manifest spellings `tooling` and `tooling-egui` fail before Host admission.
- Cargo features with those names remain compile-time Host boundaries. Pure tooling models,
  snapshots, commands, diagnostics, and egui command-returning adapters remain available.
- Headless, desktop, and Editor product Hosts publish the same bounded reference-game semantics;
  no product recipe installs an Editor workspace into a runtime World.

# Result

Passed for the complete NUI-U2 contract.

- `nara_project`: 32 tests passed.
- `nara_tooling` plus `nara_tooling_egui`: 33 tests passed.
- Root product capability and project composition suites: 7 and 9 tests passed.
- The two focused plugin-composition proofs passed.
- Editor persistence: 12 tests passed, including the new `EditorWorkspace: !Resource` trybuild
  contract.
- Workspace Play/Retry: 12 tests passed; Scene play and file-workspace suites passed 2 tests each.
- Project runtime boot: 8 tests passed.
- Reference-game Editor journey: 1 test passed. Host parity: 2 tests passed across headless,
  desktop, and Editor products.
- Locked affected-library check passed for `nara`, `nara_tooling`, `nara_tooling_egui`, and
  `nara_project` with all root features.
- Strict zero-allowance Clippy passed for `nara_tooling` and `nara_project`. Root strict Clippy
  passed with only the pre-existing Rust 1.97 `result_large_err` and `drop_non_drop` lints allowed.
- Exact rustfmt checks passed for every NUI-U2 Rust file. Staged diff and stale-symbol inspection
  passed, and independent correctness/authority review found no unresolved P0, P1, or P2 issue.

# Concurrent Worktree Gates

- `cargo check --workspace --all-targets --all-features --locked -j 1` reached the concurrent
  `nara_ui_render` proof-first tests and stopped because they call the not-yet-implemented
  `UiNode::with_sibling_order`. Those user-owned UI test edits were neither modified nor committed.
- Global `cargo fmt --all -- --check` reports formatting only in the concurrent user-owned
  `crates/nara_ui/src/tests.rs`; the NUI-U2 file set passes exact rustfmt checks.
- Unqualified Rust 1.97 `-D warnings` also exposes pre-existing large-error and egui adapter style
  lints outside the NUI-U2 diff. Focused changed-target linting remains clean as recorded above.
- One loaded Play teardown run observed `Stopping` beyond its 32 ms test budget while another
  repository compiled concurrently. An isolated exact rerun and a subsequent complete 12-test
  rerun both passed, so no Host state-machine regression was retained.

# Follow-up

NUI-U3 is the sole next unit. Remove the unconsumed generic typed/erased importer registry and its
executor-shaped public claims while preserving the concrete Image import workflow and its bounded
publication semantics.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u2-remove-duplicate-runtime-tooling-authority`
- `docs/architecture/adr/0104-host-owned-editor-workspace-installation.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u2-1-host-owned-editor-workspace`
- `tests/ui/editor_persistence/workspace_not_resource.rs`
- `reference-game/tests/editor_journey.rs`
- `reference-game/tests/host_parity.rs`
