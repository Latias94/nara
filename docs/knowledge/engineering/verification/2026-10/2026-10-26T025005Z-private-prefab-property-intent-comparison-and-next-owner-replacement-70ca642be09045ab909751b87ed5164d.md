---
type: "Verification Evidence"
title: "Private Prefab property intent comparison and next owner replacement"
description: "Compared real root-patch lowering with retained scalar leaf intent, reviewed the proposed full overlay replacement, and recorded structural/reference freeze blockers without changing production format."
timestamp: 2026-10-26T02:50:05Z
record_id: "70ca642be09045ab909751b87ed5164d"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "design/declarative-prefab-overlays"
git_commit: "f998e42"
---

# Verification

This is a test-private semantic comparison and a Proposed owner design after `f998e42`, not a
production Prefab overlay replacement. Accepted ADR 0038 still governs the existing patch-based
`PrefabInstance.overrides`. Reconcile it before changing that production boundary.

# Result

- The real existing source-authoring path lowers a nested `x` edit to a whole-root patch. Replaying
  that patch on a base whose uncovered `y` changed yields the old `y`. Repreparing the retained
  original leaf intent yields the new `y`. This proves the lowering cannot reconstruct original
  leaf intent; an intentionally authored whole-root override itself is not a defect.
- The private candidate retains an explicit pin even when it initially equals the base. After
  that base changes, the old candidate refuses and fresh preparation of the retained intent
  restores the pin. Comparing values to infer inheritance would not preserve this choice.
- Other tests reject changed base content, a different canonical source, a different exact
  Schema admission, an old-version replacement and a final unresolvable selector. Source and
  history remain unchanged; rejected intent stays in the private Rust result.
- The design separates durable identity, authoring interpretation evidence and exact activation
  closure evidence. It inventories every existing structural/component capability that must be
  replaced before deleting durable operation patches. It does not select a wire format early.
- Current plan/design text now distinguishes already qualified property selectors, construction,
  complete Runtime/Canvas keys and disposable epochs from the unimplemented overlay. Catalog
  file envelope v1 and fingerprint domain v3 are no longer conflated.

# Evidence

`crates/nara_scene/tests/prefab_overlay_comparison.rs` contains six test-private cases. The exact
same inputs go through existing `PrefabAuthoringSession` preparation for both candidate results;
the incumbent is not an intentionally weak substitute. The helper uses complete document equality
and exact Schema admission as its bounded activation oracle, not a production source-capture,
closure-cache or digest mechanism.

Serial Rust 1.95.0 verification reused the checkout `target`, `CARGO_BUILD_JOBS=1`:

- Initial Scene/Tooling run `5b6723a8-c734-4399-b212-989d57609824`: 426 passed, 4 skipped.
- Final post-review run `7bad9f62-09d0-4381-a90a-aa909c5358f2`: 426 passed, 4 skipped, including
  all six comparison cases. Other query/consumer scope is recorded in its
  [separate successor](2026-10-26T024129Z-pre-budget-query-ownership-and-independent-direct-member-panel-workflow-1acbda3fb8cc47218dd9b54bcd59f367.md).
- Scene/Tooling all-target correctness Clippy passed with ordinary existing warnings; subsequent
  changes corrected only proof assertions and documentation, followed by the final nextest run.
- Root/fixture formatting, whitespace and local-document link checks are integration checks.

The pinned read-only upstream comparison used Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. The design cites concrete scene/template, difference
storage, reimport, path/ID recovery and structural fallback sources. Godot supplies useful
base-plus-difference precedent, not a universal unresolved-intent guarantee. This Bevy snapshot
does not supply a mature persistent Prefab rebase system. No upstream runtime experiment ran.

# Independent Challenge And Corrections

Reviewer `01a1760c-0906-7ca3-95f9-edb70ac19085` found two P2 design gaps:

1. Ordered edits that change a keyed item's identity cannot be saved as an unordered set of
   original selectors. The proposed initial rule refuses key-changing leaf overrides and requires
   deliberate whole-keyed-container replacement; existing exact source edits remain sequential.
2. Complete targets do not bind references inside replacement values. The next structural
   comparison must select explicit bounded reference binding for inherited/local/repeated-nested
   targets, including identical local spellings. Schema traversal cannot invent that context.

Reviewer `01a17610-4474-7e50-8e18-0636c8a19800` found the equal-value-pin test initially prepared
only after the base changed, leaving its asserted gate unproved. It now prepares while equal,
checks retained intent/effective value, changes the base, rejects the old candidate and reprepares
that retained intent. The reviewer completed a targeted follow-up with no remaining confirmed
findings and also checked the bounded design corrections. Review is static; execution is above.

# Follow-up

- Next implement the structural/nested/reference comparison from the owning design, not another
  generic recursive query or a repeat of already qualified identity mechanisms.
- This prototype has no persistent serialization, repair transaction/history, repeated/nested
  closure compiler, real Host publication, conflict Save/Reopen, convert-to-local or native
  qualification. Retained Rust conflicts are not evidence that unresolved intent survives disk.
- Before production replacement, settle normalized overlap/placement rules, complete reference
  representation and bounded source-key encoding; amend owning ADRs, replace all capabilities
  together, and qualify the public workflow. Remove test-private losing mechanisms when selected
  production coverage supersedes them.
- No performance/allocation improvement, global ID service, second Undo owner, compatibility
  layer, migration guide or full-foundation completion is claimed. The overall Goal stays active.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- `crates/nara_scene/tests/prefab_overlay_comparison.rs`
