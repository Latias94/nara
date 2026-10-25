---
type: "Verification Evidence"
title: "Source owned first property writes and bounded selector error qualification"
description: "Qualifies selected default-root initialization, exact authored absence Undo, bounded rejected IDs and real non-active panel persistence while retaining complete template and storage contracts."
timestamp: 2026-10-25T06:01:30Z
record_id: "0dfaa18252de473b90ff57ef150e64b0"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "53c0b82f8d60d1b47c97aef63f0a69594cbb3070"
---

# Verification

This records the bounded R2 first-write and unadmitted-input correction on `53c0b82` plus the
eight Rust files in the final source manifest. It does not complete R2, R1, a delivery wave or
the foundation Goal. ADRs 0026/0045/0047/0090 distinguish source mutation, default observations,
complete templates and operation history. No compatibility alias or new migration guide is added.

# Result

- Source privately initializes only the selected omitted declared root: a nested replacement uses
  that root's admitted default; an empty selector uses the supplied root. The existing complete
  Reflect replacement and Source root patch remain the semantic/replay authorities. Both Scene
  and Prefab retain their own session, candidate, revision, inverse and history owners.
- Forward publication is an ordinary `SetField`; its inverse is computed from the original
  authored document and uses `RemoveField` for original absence. Two edits of one defaulted root
  coalesce into one root patch and one Undo entry. Drop, late failure, stale commit and history
  refusal publish nothing. Unrelated defaults, unknown records and other documents remain intact.
- Reading a default is not source mutation. Explicit Null/partial values are not merged; missing
  storage ancestors and list slots are not created, even for a contiguous end slot. Existing empty
  parents remain empty after Undo. Complete authoring templates must still contain every root,
  including optional/defaulted ones; decode defaults do not relax template admission.
- `ComponentPropertyError::InvalidFieldId` is now a unit variant, not an owned rejected ID.
  Original mutation admission counts target entity/component bytes before candidate-key copying,
  in addition to selectors and replacements. Construction rejects component IDs above the existing
  ID ceiling before retaining declaration/write intent. Length admission is not Schema authority.
- Temporary default copies retain admitted Schema/default limits, including the existing 4 MiB
  shape-byte ceiling. They are not claimed to fit the smaller final root replay limit of 256
  value nodes/64 KiB. An 80 KiB default can legally shrink before final admission; a large untouched
  remainder still rejects. Complete candidate, forward/inverse replay and history budgets remain.

# Evidence

## Proof And Corrections

Three actual behavior failures precede their production fixes:

| Proof | Actual red run |
| --- | --- |
| Invalid root/member/list-identity errors must not retain or display private unadmitted input | `b699163d-c5cc-418b-818d-4bcb41ca65ae` |
| First nested Source write must use the selected omitted default root | `d09c4f37-b3c6-430e-be6f-e42838d6c213` |
| Construction must reject oversized component IDs before retaining intent | `ae725af8-a3bc-460c-a9fb-cf19860c64e8` |

The first Source fixture accidentally placed unknown payload inside a closed known component.
Its admission failure is a fixture error, not the first-write red above; the corrected unknown
component fixture reaches the intended mutation behavior. A later attempted staged omitted-root
creation test likewise contradicts complete-template admission. That experiment is removed,
not implemented by weakening validation. Its replacement explicitly requires the catalog's
`InvalidComponentAuthoringInitialValue` error for an incomplete optional/defaulted template.

The public consumer uses a real registered panel in non-active Scene and Prefab documents.
It synchronously reads `Defaulted(13)`, computes 15 from that observation and prepares through
the existing owner. Drop/hold/late failure/stale rejection preserve omission. Commit/Undo/Redo
check exact source and history; the inverse restores absence and reading again returns the default.
Complete JSON comparisons preserve unknown data, large siblings, another omitted default, source
identity and navigation. Two independent checkpoint/Reopen paths exercise original omission
and explicit edited storage.

The initial public helper incorrectly expected clean Undo Save to return `Accepted`. Production
correctly reports exact `NotDirty`; the helper now requires that result and acknowledges it,
while dirty Redo still requires `Accepted` followed by exact `Saved` before real Reopen. It does
not accept arbitrary refusal as successful persistence or claim a clean no-op performed a write.

The original broad owner run stops at an obsolete retained-construction byte expectation:
582 passed, one failed, 99 not run and four skipped. The correction retains the 64 KiB limit and
all three original writes. Three target identities cost `3 * (22 + 9) = 93` bytes, selectors cost
14 and the resolved local-reference replacement costs another 22. Payloads of 65,429 and 65,407
therefore respectively reject and accept at 65,558/65,536 total bytes before root coalescing.
The accepted branch still checks the actual generated ID length of 22. Omitting reference cost
or charging only the final coalesced roots would break this proof.

## Independent Review

The design sidecar selects Source-owned seeding, not all-default projection, mutable tool access
or a changed complete resolver. It identifies why automatic ancestor creation would prevent exact
absence-restoring Undo. Its initial staged-template suggestion is superseded by actual catalog
evidence above and is not a qualified capability.

Independent frozen-source correctness/security and test/spec/simplification passes inspect the
production boundaries and public proof. The former identifies the construction ingress's existing
oversized-ID retention path; two early guards address it. Five simplification rubrics run inline
in the separate quality context; no speculative helper or production framework is added, and
protected original public proof remains. These are not five separate reviewer agents.

The targeted correction review finds one P3 proof gap: rejection was tested only against an empty
draft. The strengthened test retains `weights.private-key = 6` before rejection and appends
`data.text = after` afterward, requiring the exact two non-coalescing root operations for both
declaration/write branches. The same reviewer independently rechecks this fix, the literal byte
boundary and all eight final hashes and reports no actionable finding. It runs no Cargo and
does not claim independent execution/native qualification.

Code review: skipped (ce-code-review unavailable) - actual invocation
`20261025-054356-2610cebe` terminates failed before reviewer dispatch because its isolated runtime
does not expose nested native spawn/wait/close. It launches no reviewer or report leaf. This is
not a complete CE receipt. The bounded native reviews and parent manual diff scan are recorded
instead. That scan covers default/explicit distinction, original-document inverse ownership,
replay versus temporary limits, pre-copy admission, removal of payload-bearing error matches,
complete-template refusal and the exact source/document scope.

## Serial Qualification

Explicit Rust 1.95.0, locked manifests, shared root `target`, `CARGO_BUILD_JOBS=1`; no concurrent
Cargo in this checkout. Final eight-file hashes are frozen before the corrected qualification.

| Scope | Actual result |
| --- | --- |
| Corrected focused property/constructor boundary, `bc2aaa5d-c6a9-4ec9-85d8-50b71a5335b2` | 19 passed; 596 excluded |
| Independent prepared-authoring consumer, `5c962ffa-4f88-4fbc-8f6a-2848796aaaca` | 6 workflows passed; one timing diagnostic skipped |
| Final Reflect/Scene/Tooling all-feature owners, `e4701011-47a7-4ba9-b998-3c46ed2614bb` | 682 passed; four skipped |
| Final root/Editor all-feature libraries, `5e3e6f6c-0343-443f-af26-b0b2c323f987` | 597 passed; 28 skipped |
| Selected actual Windows/DX12, `c40bbc2b-17e5-4303-bcde-9fe27612a1c2` | 3 passed; 220 excluded |
| Locked workspace all-feature/all-target check | Exit 0 |
| Reflect/Scene/Tooling/root/Editor all-feature/all-target correctness/suspicious Clippy | Exit 0; other existing warnings retained |
| Reflect/Scene `--no-default-features` check | Exit 0 |
| Root and independent-consumer workspace format checks | Both exit 0 |

The public run precedes only the final two owner-test corrections. Its production/public Rust
hashes are unchanged; it is not relabeled as a later execution. The earlier focused 18-pass run
`fffa4c71-5ee2-4f11-b071-0a84636d435f` is superseded by the final 19-case run above.

The final broad/native/static runs follow both owner-test corrections, not just the earlier
production source. The exact existing native cases are
`native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen`,
`native_canvas_widgets_pick_move_resize_and_save_reopen_real_source` and
`native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`.
They exercise affected existing structure, Canvas and retained-source paths using hidden native
windows and actual DX12 submission/presentation. They are not a native-window test of the new
defaulted panel, complete native platform-group acceptance, monitor scanout, Linux/macOS,
OS IME/accessibility or release-performance qualification.

The [normalized source manifest](../../../../verification/data/2026-10-25-source-first-property-writes-source.json)
has SHA-256 `d12e68484a36354ce3e16b656081bc5da726d91bac8892a5ceeeb1837e22b179`:
sort the eight paths, hash each file with CRLF normalized to LF, serialize the path/hash rows as
compact sorted-key JSON, then hash those bytes. The raw eight-file manifest is unchanged through
the final runs and document closure. Exact command arrays, logs and bounded review reports remain
in temporary `nara-property-write-2mhdfwtz`; they are not a new repository verification framework.

Changed-document local link targets and `git diff --check` pass. Engineering-memory render and
validate pass; pre-existing legacy-ID, local-path and historical large-ledger warnings remain.
All scoped review/design/worker handles are collected and closed. The separate R1 research lane
does not qualify product behavior or expand this unit's implementation claim.

# Follow-up

Resume R1's useful semantic proposal and finite explicitly enabled child-process comparison.
Its exact persistence acknowledgment identity and structured compiled-tool diagnostic transport
remain real owner obligations, not consequences of a prepared panel or this first-write fix.
Nested discovery,
broader index-maintenance costs and complete R2 remain open; do not add a universal query planner
to close them cosmetically. R3/R4 representation/physical lifetime, declarative Prefab overlays,
resource/delivery, composable rendering and complete product integration remain in the total Goal.
No push, PR, vendor mutation or release/platform/performance expansion is authorized by this record.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0026](../../../../architecture/adr/0026-editor-command-patch-and-undo-model.md)
- [ADR 0045](../../../../architecture/adr/0045-component-schema-capability-metadata.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [Source read/write design](../../../../architecture/authoring-read-projection-design.md)
- [Panel guide](../../../../guides/editor-panels.md)
