---
type: "Verification Evidence"
title: "Source-owned entity creation and generated product qualification"
description: "Qualified ordinary Scene/Prefab source-owned creation, removal of Workspace ordinals, independent persistence and Windows DX12 product workflows; full R2 remains open."
timestamp: 2026-10-24T20:39:44Z
record_id: "c5fd5454223046e38fc1761338ddffb0"
producer_id: "codex-root"
run_id: "foundation-rebaseline-r2-source-creation-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "89ba4303a5ec3162acc952475bc280f2339a0e34"
---

# Verification

Ordinary generated entity creation is qualified as an R2 prerequisite, not as full transaction-local
construction, R2, a delivery wave or the foundation goal. The source is the dirty worktree based on
`89ba430` with the unchanged 12-Rust-file LF manifest
`dcc43c19681958d36bb4d584187c90ae8f912ebc16010a91a78c29ddcffcdd51`.
The [progress checkpoint](../../progress/2026-10/2026-10-24T202546Z-source-owned-creation-implemented-generated-export-qualification-pending-982fcd60eda3477aa06a6ba835da0c56.md)
owns its exact inventory, composite-hash method, red evidence and review dispositions. This
successor records actual terminal results, not a reinterpretation of the former pending job.

# Result

Scene/Prefab sessions now own fresh ID generation, complete creation, validation and history
publication. `CreatedSourceEntity` returns the actual ID/report. Published empty creation and
registry/database-aware native component-bearing creation retain separate admission. Workspace
uses those results and no longer owns ordinal fields, ID candidates, success increments or Reopen
resets. Creation failure leaves source/history/selection unchanged. Existing authored IDs retain
their spelling; Undo/Redo and saved Reopen reuse the stored ID rather than generate another.

The private generator retains all 128 entropy bits in 22 URL-safe unpadded characters, checks
current conflicts exactly and explicitly refuses after eight attempts or entropy failure. No
allocator reservation, runtime-counter persistence or absolute historical non-reuse guarantee.
Eleven lockfiles were refreshed through offline Cargo resolution; their only changes are Scene's
two dependency edges and, where absent, the already-selected `base64` 0.21.7 package.

# Evidence

All Cargo commands were serial with Rust 1.95.0, `CARGO_BUILD_JOBS=1` and shared root `target`.
The generated-product test retains its existing shared `target/nara-editor-product-cache`.

| Executed scope | Nextest run | Result |
| --- | --- | --- |
| `-p nara_scene -p nara_tooling --all-features` | `40b2e7ee-092a-4958-a4b7-b586571b0c90` | 383 passed, 2 skipped |
| `-p nara --all-features --lib` | `85c6d2b4-db84-43f8-954e-5c8501b3f012` | 398 passed, 3 skipped |
| Selected `editor_persistence` preview consumers | `c9030471-93fb-46b9-9d90-9a93ffc29ad0` | 6 passed; 14 excluded |
| `-p nara-editor --lib` | `6f16e861-5c06-487d-acce-ae0527a80109` | 183 passed, 25 skipped |
| Selected ignored Windows/DX12 native trajectories | `435cd3fd-40ea-4199-9e3a-1c0c9e78ee4d` | 3 passed; 205 excluded |
| Ignored generated starter edit/Check/build/export | `09f71741-4a0c-4dbe-a0ad-3588c8a6ec93` | 1 passed in 808.333 s; 1 excluded |
| Final independent `property_authoring` consumer | `6c75c5be-42e3-4ff3-9c2a-3d926579bd85` | 5 passed |
| Final no-default `nara_scene::source_creation` | `1eb29107-7cae-4693-a985-41406aa5523f` | 3 passed |

The final independent consumer re-executes Scene/Prefab creation, one Undo/Redo, exact saved bytes,
Reopen identity and preservation of unknown/future payloads. It also retains the existing property
and non-active-document cases. Native selections execute ordinary structural widgets/history/
Save/Reopen, Canvas widgets/structure/Save/Reopen and retained Scene creation/Undo epochs/navigation
through hidden real windows and DX12. The generated trajectory executes the existing source-carrying
product/export workflow; it does not qualify R7's future source-free delivery or a new build-speed
claim. The SLOW job ended successfully; unified session 3174 is terminal and must not be restarted.

Final static checks all pass:

```text
cargo +1.95.0 check --locked --workspace --all-features --all-targets
cargo +1.95.0 clippy --locked -p nara_scene -p nara_tooling --all-features --all-targets -- -D clippy::correctness
cargo +1.95.0 check --locked -p nara --no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib --target wasm32-unknown-unknown
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 fmt --all --manifest-path tests/fixtures/module-consumer/Cargo.toml -- --check
```

Existing dead-code/unused-import and non-correctness Clippy warnings remain; this is not a
warning-clean claim. Portable wasm compilation is not browser execution. No Linux/macOS native,
IME/accessibility or complete native-platform claim follows. Logs remain outside tracked source
under `%TEMP%/nara-r2-final-*`.

The main owner rechecked the complete relevant source/consumer/lock/ADR diff and unchanged source
manifest. No source-level blocker remains for ordinary creation. No proof test, safety admission
or native workflow was removed to obtain these results. The owning ledger and guide updates do
not substitute for the executions above.

## Review Scope

Initial source review's missing database-aware entry was implemented and tested. Five simplification
passes produced one behavior-equivalent reuse and one redundant-scan removal; no proof test changed
through simplification. Final independent integration review found no supported defect. The
checkpoint records the exact handles and dispositions; all are closed.

Code review: skipped (ce-code-review unavailable)

The fresh actual portable invocation terminated failed because its runtime could not discover
nested dispatch/collection for the required merge/report leaves. It is not a completed receipt.
The independent static reviews and final manual diff scan are recorded separately, not relabeled
as that missing receipt.

# Follow-up

Continue the complete foundation goal. Full retained construction, field queries, panel hooks and
candidate cost comparison are still open, as are the remaining dependency-ordered waves. No
migration guide, compatibility layer, epoch reset, remote publication or vendor change is introduced.

Read-only next-unit investigation `01a16f9b-1207-7402-a8ea-e1dca0f7a3a1` is complete and closed.
Its concrete implementation seam is `authoring/property.rs::lower`, with a private borrowed staged
record map alongside existing Available records. Keep raw selector/value charging, ordered keyed
replacement and touched-root coalescing; retain each candidate's borrowed Schema for final lowering.
A staged template must never override an existing unavailable record. Extract matching Schema plus
owner/version/template from `AuthoringDocumentState::initial_component`, without cloning the whole
classification or asking the caller to copy metadata. Keep pristine AddComponent templates in the
additive prefix, append lowered fields, then call the existing nominal prepare exactly once.

This does not authorize reordering arbitrary deletion/replacement patches. Structural-only drafts
skip the nonempty property lowerer; scalar/reference writes share the raw eight-write bound, expanded
patches retain the 256-operation bound, and final root/inverse/history limits remain independent.
Decisive next tests combine existing/new records, two components per entity, local parent,
mutual/self and keyed references; key rename followed by new/old selector; and coalescing versus raw
work/replay refusal. The retained draft/result signature still requires an actual public consumer
experiment; no new draft API is implemented by this verification record.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Creation/construction design](../../../../architecture/authoring-construction-design.md)
- [Source owner](../../../../../crates/nara_scene/src/authoring/creation.rs)
- [Independent public consumer](../../../../../tests/fixtures/module-consumer/tests/property_authoring.rs)
- [Native qualification](../../../../verification/native-editor-qualification.md)
