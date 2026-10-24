---
type: "Verification Evidence"
title: "Retained source construction pinned public and native qualification"
description: "Rust 1.95.0 qualifies retained construction through owner/public/Session lifecycle paths, final static checks and selected Windows native regressions; full R2 remains open."
timestamp: 2026-10-24T22:49:39Z
record_id: "e1813acad6f04a93898a1e342989fa44"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7868d48072a893b5a9f176d93d4597d0a5ecb56a"
---

# Verification

Qualifies the retained construction unit on the uncommitted tree based on `7868d48`, not full R2,
a delivery wave or the foundation Goal. The final 18-Rust-file LF manifest is
`cc44a9915e7fdafce3a98fa286d067d705d49a2adfef7c8a23b2d20f4563b073`.
Entries are the sorted `git diff --name-only -- '*.rs'` paths, including intent-to-add files,
with SHA-256 of CRLF-to-LF normalized bytes. The composite is SHA-256 of UTF-8
`json.dumps(entries, sort_keys=True, separators=(',', ':'))`, where each entry is `{path,sha256}`.

Every execution below explicitly uses `cargo +1.95.0`, serially, with `CARGO_BUILD_JOBS=1` and the
shared root `target`. The recorded compiler is `rustc 1.95.0 (59807616e 2026-04-14)`.

# Result

Scene/Prefab own bounded additive construction intent, draft-affine opaque locals, fresh stored
IDs, template admission, one prepared candidate/inverse and one failure-atomic history publication.
Existing and staged property writes share the admitted Schema lowerer and raw-work limits before
root coalescing. Prepared and successful consuming results retain the same mapping. Workspace,
Session and direct Application carry the exact source result without parsing patches or inferring
IDs from a later frame. No reservation allocator, second executor/history/document or compatibility
alias is introduced. History's unchanged entry ceilings now use accurate `max_entry_*` names.

The real independent consumer qualifies non-active Scene/Prefab construction with two components,
local parenting, mutual references, drop/last-invalid/stale refusal, one Undo/Redo, Save/Reopen,
nonempty selection and historical receipts. Same raw document slots after project replacement do
not authorize old candidates. Specialized UI Prefab admission remains at Host prepare and commit,
including candidates obtained directly from Workspace. No registered-panel prepared API is claimed.

# Evidence

| Explicit Rust 1.95.0 scope | Nextest run ID | Result |
| --- | --- | --- |
| Scene/Tooling all features | `df3d3877-bed7-46ae-99c4-ae43f4f01163` | 393 passed, 3 skipped |
| Root/Editor all-feature libraries | `e2721e6d-f359-419e-afaf-e7c189812765` | 581 passed, 28 skipped |
| Independently compiled Application consumer | `f11d7348-1e88-4fb0-bf8f-fadd8eed8e2f` | 4 passed |
| Feature-off UI structure fence | `cf8b78ae-c4ae-499e-9b75-731b5206006a` | 1 passed |
| No-default native source creation | `60551488-e0b8-467c-b001-1cb2d9a26b54` | 3 passed |
| Direct Session construction close fence | `9459907c-3a5e-454c-a81e-ae5ab594fedc` | 1 passed, six Scene/Prefab/lifecycle combinations |
| Selected ignored Windows native trajectories | `56482099-d523-4aab-b7ca-55061f78af9c` | 3 passed, 205 excluded |
| Ignored retained/control owner comparison | `2b175295-820d-468c-879d-c20d2ae087c5` | 1 passed |

The root/Editor full run predates only the additional direct-Session test; production code did
not change afterwards. That focused test and subsequent workspace/all-target checks compile the
final test source. It calls Session directly in Scene/Prefab crossed with pending CloseDocument,
pending Exit and first physical close, keeping a valid current candidate and proving no receipt
and unchanged source/revision/history/selection/saved state and bytes. No Application pre-filter
can satisfy those assertions instead.

Final static commands all pass:

```text
cargo +1.95.0 check --locked --workspace --all-features --all-targets
cargo +1.95.0 clippy --locked -p nara_scene -p nara_tooling -p nara -p nara-editor --all-features --all-targets -- -D clippy::correctness
cargo +1.95.0 check --locked -p nara --no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib --target wasm32-unknown-unknown
```

Native selections exercise ordinary structural widgets/history/persistence, Canvas pick/move/
resize/persistence and retained Scene creation/Undo/navigation through hidden real Windows/DX12
Hosts. They are affected-path regressions, not a new panel-construction widget, all-platform,
IME/accessibility, browser execution or performance claim. The independent direct constructor
itself is exercised by the window-free Application consumer. The long generated-starter export
was not rerun: generator/build/export policy did not change in this unit.

The pinned debug comparison records retained/control prepare P95 499,900/409,700 ns and commit
P95 7,200/6,800 ns, with eight warmups and 64 samples per mode. It remains below the design's
735,000/10,200 ns investigation thresholds. Different ID lengths, control-first ordering and
unisolated scheduling/cache effects remain explicit; no speedup or allocator/RSS bound is claimed.
See the [cost report](../../../../benchmarks/2026-10-25-retained-construction.md).

## Review And Disposition

Actual full CE receipt `nara-r2-ce-review-8rwh6_hq` has `status: complete`, verdict `Ready with fixes`,
eight completed reviewer artifacts, one fresh validator and complete stage-cost metadata. Its
single confirmed P2 was a direct Session commit/physical-close regression gap, not a current
runtime bypass or P0/P1 defect. The test above resolves it without a production behavior change.
Bounded independent post-fix review `01a17012-73e3-7660-9051-3c73ac699639` confirms the direct ingress,
six independent cuts, valid retained basis and complete state-preservation assertions; it is closed.
The CE receipt is not rewritten to pretend its original finding never existed.

The five earlier simplification passes applied candidate-Schema reuse and borrowed/cached sibling
owner lookup, removed the unnecessary callback/macro parameter and one redundant private module
comment. The shared patch owner helper preserves lookup order. Existing proof tests were not
deleted, renamed or weakened. The Before/After characterization passes before and after that
refactor. A short comment explaining two-stage local-reference byte charging remains deliberately.

## Toolchain Correction

This explicitly pinned evidence supersedes the unsupported blanket Rust 1.95.0 label in the
[earlier unpinned checkpoint](2026-10-24T215522Z-retained-construction-owner-public-and-lifecycle-regression-checkpoint-82b171c17ee24213934e375e976c3e0c.md).
Those commands did not pin/capture the compiler at invocation; later inspection observed default
stable 1.99.0, not proof of the historical version. Preserve the earlier shard as history, with
this correction and the [intermediate record](../../progress/2026-10/2026-10-24T224111Z-retained-construction-complete-review-and-pinned-qualification-correction-5fc5eada19d14468877fc065bb2ff7ff.md).
No global toolchain setting was changed. The supplemental default-toolchain static/native pass
does not substitute for the explicit pinned executions above.

# Follow-up

The current verification chain (unified exec `8877`) is terminal; all review/validator/report and
post-fix handles are closed. No Cargo or agent remains live for this unit. Logs and complete CE
artifacts remain outside tracked source under the corresponding temporary names. Existing compiler/
non-correctness Clippy and legacy-memory warnings are not a warning-clean claim.

Finish document/format checks, inspect the exact final diff and make the authorized local commit.
Continue R2 with selection-independent component/root-value reading and registered-panel prepared
hooks, then the dependency-ordered foundation waves. Do not reset source/runtime identity axes,
claim the Goal complete, publish remotely or include the pre-existing untracked `vendor/`.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Construction design](../../../../architecture/authoring-construction-design.md)
- [Source implementation](../../../../../crates/nara_scene/src/authoring/construction.rs)
- [Workspace adapter](../../../../../crates/nara_tooling/src/workspace/construction.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
- [Direct Session proof](../../../../../src/project_host/runtime/tests/closing_session.rs)
- [Next query seam](../../subagents/2026-10/2026-10-24T215152Z-r2-selection-independent-field-reading-seam-389b97a2cf8f4275abbae7eeaa84bc6d.md)
