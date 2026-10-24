---
type: "Work Progress"
title: "Source-owned creation implemented; generated export qualification pending"
description: "R2 ordinary creation owns IDs and publication; owner/public/native checks pass while a real release export remains live."
timestamp: 2026-10-24T20:25:46Z
record_id: "982fcd60eda3477aa06a6ba835da0c56"
producer_id: "codex-root"
run_id: "foundation-rebaseline-r2-source-creation-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "89ba4303a5ec3162acc952475bc280f2339a0e34"
---

# Summary

The ordinary source-owned creation prerequisite is implemented in the uncommitted worktree on
`89ba430`. This is not full R2, a delivery wave, or the foundation goal. The real generated-product
export qualification is still live; no implementation commit or final qualification is claimed.
All dirty source/doc/lock files are this thread's work. The pre-existing untracked `vendor/`
remains excluded and unchanged. No remote publication, compatibility alias or migration guide.

# Details

Scene/Prefab sessions now own identity generation, complete creation and history publication.
`CreatedSourceEntity` returns the exact stored ID/report only after success. Native registry and
database-aware entries remain separate from Published empty creation. Both Workspace ordinals,
candidate-ID helpers, success increments and Reopen resets are deleted. Consumers obtain actual
selection/outcome identities instead of predicting `editor_entity_1` or parsing creation patches.

The shared private generator uses fallible OS entropy, all 128 bits as 22 unpadded Base64url
characters, exact current-source conflict checks and eight attempts. It has no reservation/cursor
or arbitrary historical non-reuse promise. Existing authored ID spelling and runtime/source
incarnation semantics are unchanged. `base64` reuses locked version 0.21.7; eleven lockfiles have
only the new Scene dependency edges and, where absent, that same package. Offline Cargo metadata
updated the independent locks without unrelated version upgrades.

The new design and ADRs 0026/0047/0118 adopt source-owned creation. The guide/map describe current
APIs and retained-draft limits. Pinned Godot/Bevy source comparison is recorded in the design.
The retained draft remains an experiment: bounded intents and exact basis, templates supplied at
prepare, shared R5a lowering, and success mapping through the existing candidate/history owner.
No public draft, new commit result protocol, candidate cost comparison, full field query or panel
prepared hook is implemented by this checkpoint.

## Source And Verification

The 12-Rust-file LF manifest is
`dcc43c19681958d36bb4d584187c90ae8f912ebc16010a91a78c29ddcffcdd51`.
The diagnostic manifest is `%TEMP%/nara-r2-source-creation-manifest.json`. Its composite input is
sorted `sha256(LF file bytes) + two spaces + repo-relative path + newline` for these files:

```text
crates/nara_scene/src/authoring.rs
crates/nara_scene/src/authoring/creation.rs
crates/nara_scene/src/lib.rs
crates/nara_scene/tests/prepared_authoring.rs
crates/nara_scene/tests/source_creation.rs
crates/nara_tooling/src/composition.rs
crates/nara_tooling/src/workspace.rs
editor/tests/authoring_loop.rs
src/project_host/runtime/tests.rs
tests/editor_persistence.rs
tests/fixtures/module-consumer/tests/property_authoring.rs
tests/support/fake_preview_toolkit.rs
```

Executed serially with Rust 1.95.0 and `CARGO_BUILD_JOBS=1`:

| Scope | Run | Actual result |
| --- | --- | --- |
| Scene/Tooling all features | `40b2e7ee-092a-4958-a4b7-b586571b0c90` | 383 passed, 2 ignored |
| Root library all features | `85c6d2b4-db84-43f8-954e-5c8501b3f012` | 398 passed, 3 skipped |
| Selected root preview consumers | `c9030471-93fb-46b9-9d90-9a93ffc29ad0` | 6 passed, 14 filtered |
| Editor library | `6f16e861-5c06-487d-acce-ae0527a80109` | 183 passed, 25 ignored |
| Actual Windows/DX12 | `435cd3fd-40ea-4199-9e3a-1c0c9e78ee4d` | 3 selected hidden-window trajectories passed |
| No-default Scene creation | `9b687859-52fa-45e6-9567-14070b352980` | 3 passed, including native project-database context |
| Independent property/creation consumer | `38fad70a-82af-4d08-9fa7-a5fc7b55cee8` | 5 passed before the two behavior-preserving simplifications; final source rerun remains pending |

Native selections are ordinary structural widgets/Undo/Save/Reopen, Canvas widgets/structure/
Save/Reopen, and retained Scene creation/Undo epochs/navigation. They do not qualify all native
Editor behavior or Linux/macOS. Local logs use the `nara-r2-final-*` names under `%TEMP%`.
Root and fixture `cargo fmt --all` passed. The scoped six-document check resolved 123 relative
links/anchors. Workspace all-feature/all-target checks, Clippy correctness and portable wasm
compilation have not yet run for final source.

The initial Reopen regression was red with two `editor_entity_1` creations and is now green.
The first source and database-aware API tests were expected E0599 missing-method reds. A first
database fixture omitted its scalar root Schema declaration; its valid declaration was corrected
without weakening the database requirement. The first updated preview consumer read selection
from an undriven frame after dispatch and panicked. Reading after the existing drive (and driving
the shared fake-toolkit helper before reading selection) corrected the fixture; all six consumers
then passed. Dispatch is not a fresh frame projection. No production workaround was added.

## Independent Review

All eight review handles are terminal and explicitly closed. Initial source reviewer
`01a16f79-6cf0-7f61-a3c2-440d66644abc` found the missing asset-database creation entry; it is fixed
and covered by the real context-sensitive native codec test in both source kinds.

The five simplification passes found one reuse and one efficiency change, both applied: Published
creation calls existing `apply_authoring_patch` after exact basis/authority admission; successful
Workspace creation clears the replaced selection before ordinary report retention, then selects
the receipt ID, avoiding redundant fresh-ID scans. No proof test or safety check was removed.
Quality/test-value/comment passes found no supported additional issue. Final integration reviewer
`01a16f88-6785-7ee2-a3fe-dbf65bd852a1` found no supported defect within this prerequisite's scope.
These are static reviews, not substitute Cargo/native executions.

Code review: skipped (ce-code-review unavailable)

Fresh actual portable invocation `01a16f85-059f-7942-8f13-c608d65f952c` returned `status: failed`:
its runtime could not discover callable nested Codex dispatch/collection, so required merge/report
leaves could not run. No completed receipt or approval verdict is claimed. Final manual diff
inspection still belongs to the main owner before commit.

# Next Action

Poll the existing unified execution session **3174**, not a new Cargo command. It owns nextest
run `09f71741-4a0c-4dbe-a0ad-3588c8a6ec93` for ignored
`starter_semantic_edit_crosses_editor_check_generated_product_and_export`. Its last observed state
is live, with a real nested release export build using the existing
`target/nara-editor-product-cache`. Child Cargo was last PID 124356; PIDs are observation hints,
not permission to kill or restart. Source, headless and CLI build work has progressed; no terminal
test verdict is available. Do not mistake SLOW output or elapsed observation for a stopped job.

After its actual terminal result, inspect/fix any real failure; otherwise finish serial checks:

```text
cargo +1.95.0 check --locked --workspace --all-features --all-targets
cargo +1.95.0 clippy --locked -p nara_scene -p nara_tooling --all-features --all-targets -- -D clippy::correctness
cargo +1.95.0 check --locked -p nara --no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib --target wasm32-unknown-unknown
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test property_authoring -j 1
```

Reuse root `target` through `CARGO_TARGET_DIR` for the independent consumer. Inspect all lock
diffs; finish formatting/document checks and final manual diff scan, write immutable final evidence,
update the owning ledger, and make a precise local Conventional Commit. Then continue the full R2
retained construction/field-query/panel/cost work and the dependency-ordered foundation waves.
The full goal remains active; do not redefine it as this ordinary creation prerequisite.

# Citations

- [R2 and complete foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source-owned creation and next construction experiment](../../../../architecture/authoring-construction-design.md)
- [Owner API](../../../../../crates/nara_scene/src/authoring/creation.rs)
- [Independent public consumer](../../../../../tests/fixtures/module-consumer/tests/property_authoring.rs)
- [Native qualification scope](../../../../verification/native-editor-qualification.md)
