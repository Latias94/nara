---
type: "Verification Evidence"
title: "Selector directed property reads and indexed root retirement qualification"
description: "Qualifies borrowed nested source observations, independent scan and copy budgets, real computed panel editing and the reviewed root-tombstone correction; full R2 remains open."
timestamp: 2026-10-25T05:11:25Z
record_id: "bd267ec01e4d44d3ad5f2a2d4543514d"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "e55ae213b4a5d458fa378758ff1c5a9eb79ed6d2"
---

# Verification

This qualifies the bounded R2 selector-directed read successor on `e55ae21` plus the final
20 Rust files, not full R2 or a delivery-wave/foundation completion. ADRs 0047/0118 adopt the
read/provenance/budget boundary; the [design](../../../../architecture/authoring-read-projection-design.md)
and [panel guide](../../../../guides/editor-panels.md) distinguish current support and open scope.

# Result

- Unreleased Root read types/actions are replaced, not aliased: one Property read carries the
  existing ComponentPropertyPath; empty steps read the root. Existing root consumers retain their
  proof while nested Member/keyed ListItem/literal MapEntry access uses the same operation.
- Reflect shares its selector/lineage walker with complete resolve/replace, but the admitted
  root read neither validates/materializes unrelated component values nor constructs a storage
  locator. Scene/Prefab retain Available/current admission and explicit/default interpretation.
  Default origin propagates to a selected leaf; Null, omitted root, OptionalAbsent, missing keys
  and retired targets remain distinct. Mutation's existing complete validation is not weakened.
- Lookup probes/compared logical bytes, raw selector metadata and selected output cost are separate
  finite limits. Work is admitted before the relevant scan/comparison; owned invalid-ID errors do
  not precede Tooling's raw metadata/step preflight. These are not allocator/time bounds, a global
  batch budget, allocation-free storage lookup or an O(depth) keyed-list promise.
- Session and source-built panels can synchronously borrow a bounded observation. The same actual
  owner candidate still supplies preparation, inverse, deferred consuming commit and history.
  No mutable Session, query session, candidate registry, second inverse store or generic DSL is added.
- The independent non-active Scene/Prefab panel refuses whole-root output at one node/eight bytes,
  reads the selected keyed/map scalar and computes its edit from that returned value. Exact callback,
  cancellation/holding, stale candidate, Undo/Redo, Save/Reopen, unknown records, unrelated large
  values, saved identity and navigation are exercised through actual public owners.

# Evidence

## Characterization, Proof And Reviews

Original root-output limits are characterized at `e55ae21` by
`38c4e547-569f-4ec8-bc60-3d992552d0f9` (one passed). The new admitted-root API initially shares
the walker without lookup charging: `a421d7d8-2a5c-47df-85c7-0b1f34c90bfc` has one successful
borrow test and the expected exhausted-budget failure. This is actual behavior red, not an absent
API compile error. Adding work admission yields all 27 property tests passing in
`97383030-6d08-4114-8c08-96540f2a81e1`; later exact branches and Source/Tooling proof are below.

One independent design sidecar identifies the raw-validation-before-owned-error risk, root
tombstone ownership and storage-locator work. The implementation preflights raw metadata/step
counts, keeps root retirement in Scene and collects storage only for complete resolution.
Two subsequent independent frozen-source passes cover correctness/security and test/spec/ergonomics.
The latter confirms one P2: Source used a hidden linear root-tombstone `contains` scan. Catalog
canonicalization already sorts the retained slice, so the correction is binary_search, not another
index. Shuffled multi-tombstone Source fixtures cover every retired entry, live/defaulted roots
and unknown roots separately. A fresh targeted reviewer confirms that canonicalization reaches
this exact view and that the three-file correction has no new concrete finding.

Literal exact/one-below scan proofs cover 11 probes/107 bytes for keyed/member selection,
2/3 for optional-plus-map and 2/12 for a retired-member prefix plus live member. Source tests
cover default provenance and absent/retired target distinctions; Tooling checks 70,000-byte invalid
root/member metadata before owned validation errors and independently rejects over-depth selectors.
No existing proof test was deleted or weakened for the API rename. The new public fixture uses
an actual stored value of 41, a distractor 900 and a separate unchanged 777; output drives +2.
The direct Application action observation equals the real registered panel observation.

The simplification skill's five rubrics run inline in an isolated native context and leave the
frozen source unchanged; removing safety/order checks or protected proof is rejected, not a
line-count objective. This is separate from code review and does not claim five reviewer agents.

All seven Codex-native worker/design/review/follow-up/quality handles are collected and closed.
No external model, cross-model independence or complete multi-persona CE receipt is claimed.
Code review: skipped (ce-code-review unavailable) - actual current invocation
`20261025T052328Z-3a90f72d` terminates failed before dispatch because the isolated context exposes
no nested generic-agent spawn/wait/close. It launches no reviewer/finish leaf. The bounded reviews
and final manual diff scan are recorded instead of inventing a completed receipt. Artifacts remain in the temporary
`nara-property-read-6z37b9kl` directory. The manual scan checks owning contracts, API removal,
shared walker semantics, budget order, sorted-catalog assumption and exact changed-file scope.

## Serial Execution And Source Scope

Explicit Rust 1.95.0, locked manifests, shared root `target`, `CARGO_BUILD_JOBS=1`, no concurrent
Cargo in this checkout. The initial broad cohorts preceded the private indexed-root correction:

| Earlier broad scope | Actual result |
| --- | --- |
| Reflect/Scene/Tooling all-feature libraries/integrations, `0a44708d-0645-40ee-b2ec-9500749a710d` | 668 passed, 4 skipped |
| Root/Editor all-feature libraries, `bb324de8-88fc-48bd-95c7-8785dbdedc57` | 597 passed, 28 skipped |
| Selected actual Windows/DX12, `f33abbf3-5665-4fe5-9fe3-f56f345fce42` | 3 passed, 220 excluded |

Final corrected-source verification is separate, not a relabeling of those runs:

| Final scope | Actual result |
| --- | --- |
| Property-addressing, Source reads and Tooling authoring queries, `6ff2dd52-ceda-46db-b8f7-2306972b9a0c` | 41 passed, 1 diagnostic skipped |
| Independent prepared-authoring consumer, `235b4dd4-dbb1-4822-b5e5-f743895d3b1e` | 5 workflows passed, 1 timing diagnostic skipped |
| Locked workspace all-feature/all-target check | Exit 0 |
| Reflect/Scene selected `--no-default-features` check | Exit 0 |
| Reflect/Scene/Tooling/root/Editor all-feature/all-target correctness Clippy | Exit 0; existing other warnings retained |
| Root and consumer `cargo +1.95.0 fmt --all -- --check` | Both exit 0 |

The final Clippy invocation starts after all final Rust file writes; all 20 hashes remain unchanged.
The workspace check and focused/public runs explicitly repeat after the correction. Full 668/597
cohorts and native cases were not repeated or claimed as final-source reruns. The private correction
has separate owner/public proof; the earlier native cases protect affected existing structure,
Canvas and retained-Scene workflows, not the new panel in a native window. Their exact names are
`native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen`,
`native_canvas_widgets_pick_move_resize_and_save_reopen_real_source` and
`native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`. This does not qualify
Linux/macOS, OS IME/accessibility, monitor scanout, release latency or a complete platform group.

The [normalized source manifest](../../../../verification/data/2026-10-25-selector-property-reads-source.json)
has SHA-256 `66ccb51c4d90691b0d5a7cc9250370e65b62d263c7c0743220ec151fb358d622` under this recipe:
sort the 20 paths, hash each file with CRLF normalized to LF, serialize the path/hash rows as compact
sorted-key JSON, then hash that byte sequence. It binds the final Rust scope despite later document
closure. Raw logs and exact command arrays remain in temporary `verification-results.json` and
`postfix-results.json`, together with the reviewed/pre-correction and final raw-byte manifests.

# Follow-up

An observed Defaulted leaf is not edit authority: the existing complete property mutation still
requires a stored root. First-write materialization of a defaulted root is a concrete adjacent R2
owner pressure, not an implemented consequence of this read interface. Do not silently synthesize
containers or spread all Schema defaults while addressing it. Nested discovery, value pagination,
unopened sources and broader query/index-maintenance timing remain separate.

The new read metadata preflight does not change mutation lowering's existing validation order:
`authoring/property.rs::lower` calls `ComponentPropertyPath::logical_bytes` before applying its
operation byte cap, and invalid-ID errors can own a cloned candidate ID. Recheck that owning
Reflect/Source input boundary before admitting bounded child-process proposals; do not mistake
this read-only correction for a complete mutation-input allocation guarantee.

Continue the total plan's dependency-ordered public workflow and representation/extension units,
including R1 exact semantic proposals and subsequent R3/R4 trials. Do not keep widening this read
unit into a universal query planner or treat it as full R1/R2 or the foundation Goal. Immutable
module/Behavior publications, durable Prefab overlays, resources/delivery, composable rendering
and final production integration remain open. No push, PR, migration guide or vendor change occurs.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Read design](../../../../architecture/authoring-read-projection-design.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Panel guide](../../../../guides/editor-panels.md)
