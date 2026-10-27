---
type: "Work Progress"
title: "Complete Prefab conversion source constraints and comparison plan"
description: "Records the merged source-intent checkpoint, three executed conversion-basis probes and the next selected-demand/history comparison without claiming a conversion implementation."
timestamp: 2026-10-27T01:41:46Z
record_id: "48bc7f5499704cd39c2f617a29916625"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/prefab-convert-to-local"
git_commit: "f2fc60af1e040dc58eb1576959deeee2235bb8e9"
supersedes: "4b14705bceb545e79f34b7afbc5144bf"
---

# Summary

The source-owned Prefab intent review/Revert successor was committed as `f2fc60a` and locally
fast-forwarded to `main`, together with the preceding declarative compiler checkpoint `3636b9a`.
No remote push or PR occurred. The next branch is `refactor/prefab-convert-to-local`.
The full foundation Goal remains active.

The new [conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md) is a design
comparison, not conversion support or an Accepted contract amendment. Three real current-compiler
probes now execute. The next source-owner experiment and its history replay are not implemented.

# Details

- The complete source-intent qualification, final 912 owner/770 root-Editor/19 independent/13
  Reference Game cohorts, six selected Windows/DX12 cases and final review are recorded by the
  [qualification successor](../../verification/2026-10/2026-10-27T012127Z-source-owned-prefab-intent-review-and-revert-qualification-4b14705bceb545e79f34b7afbc5144bf.md).
  Those overlapping results are not a single summed test count or conversion evidence.
- Read-only source explorer Aquinas (`01a17af7-b68a-7963-90e2-81033c8d9997`) confirmed that typed
  provenance, owner-local persistent ID allocation, declared-reference traversal and the existing
  prepared source/history owners are reusable. Native World export has different codec, residency
  and identity semantics and cannot supply source conversion.
- The shared compiler currently interprets a whole containing document. Selecting only final
  output is insufficient: structural opening, retained binding demand, value interpretation and
  reference validation must all respect one selected instance demand inside that same compiler.
  Do not manufacture a placeholder document or another liveness authority.
- A selected instance's projection excludes its anchor's ordinary local children and their
  independent Prefab links, even when those are visually descendants. Preserve the anchor's
  raw components, ID and placement; insert converted roots before ordinary local children.
  Identify converted records through complete typed root-anchor provenance.
- A schema-only component may remain in the semantic candidate while native preview filters it.
  An unavailable component is already omitted before that native filter. Therefore neither
  filtered pixels nor candidate absence proves complete source data. Unavailable surviving
  projected data must refuse conversion; untouched raw records remain protected.
- Generic structure admission and replay intentionally restrict populated record insertion,
  component templates, deletion and restoration. The existing history entry stores a patch,
  content states and cost, but no conversion footprint. A Tooling-only origin or `history=true`
  cannot prove safe conversion Undo/Redo. Compare an owner-created narrow footprint in the same
  history with a bounded owner checkpoint; do not add another Undo implementation.
- Host capture currently has a Scene-preview request. Prefab conversion needs its real nominal
  source kind and current receipts, including for a non-active source. A contextual preparation
  must retain root/Schema/actual dependency evidence through submission; ordinary prepared-edit
  adaptation must not drop that evidence.
- Unity 6.3 documentation distinguishes ordinary and complete unpack. Pinned Godot Make Local
  records source-link/owner changes and their inverses through its existing Undo manager.
  These are workflow/ownership observations, not evidence that either Editor implements Nara's
  source-ID, Schema-only conversion, reference-remap or closure-fence contracts. No upstream
  Editor binary was executed.

Worker Pascal (`01a17afb-ff5c-7050-80be-96642665cf63`) wrote only
[`prefab_conversion_basis.rs`](../../../../../crates/nara_scene/tests/prefab_conversion_basis.rs).
The parent reviewed the actual fixtures and assertions, formatted the checkout and ran:

```text
cargo +1.95.0 nextest run -p nara_scene --all-features --locked --test prefab_conversion_basis --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Run `42469edb-ab1e-4303-883f-7f602780128f`: **3 passed, 0 skipped**, build 39.69s, execution 0.241s.
Cargo was serial with `CARGO_BUILD_JOBS=1` and the existing root target. No production Rust
implementation changed after the preceding qualification.

The probes establish available/native-unbound semantic data and reference retention followed by
native omission, `FutureSchema` omission before native filtering with original data preserved, and
whole-document rejection caused by an unrelated missing instance after a healthy-anchor control.
The third result is expected whole-document behavior and design pressure, not a reported bug in
the current supported compiler.

Independent design reviewer Dirac (`01a17b01-7837-7ba0-a6a7-6fc7a21c3a61`) found two P2 wording
ambiguities. The parent corrected both before comparison-baseline publication: unavailable-data
refusal applies to surviving projected data needing interpretation/remap, not unchanged raw anchor
or local-child records; complete admission of this conversion's forward/inverse data does not
change the existing policy that may evict older history entries. The reviewer found no need for
a second interpreter or a broader framework. This was static source/design review, not executed
conversion evidence. Its follow-up confirmed both corrections closed, and the reviewer is closed.
Final formatting/whitespace checks and all 943 local link targets in the seven changed Markdown
paths passed. Memory render/validation passed with the existing historical advisory warnings.

# Next Action

Implement the private selected semantic-fragment comparison in the
existing compiler, followed by the same-owner exact conversion/history comparison. Qualify normal A,
broken unrelated B and broken independent local-child C without requesting B/C; require failure for
A's actually necessary broken dependency. Preserve local records and complete selected semantics.

Only after those comparisons select the narrow production interface and reconcile ADRs
0026/0038/0090/0047. Continue through independent Scene/Prefab consumption, contextual dependency
fencing, actual Editor review/history/persistence and affected native qualification. Keep
cross-source Apply, outer-only unpack, inherited Inspector selection, shared Canvas capture and
the other foundation waves separate and open.

All implementation workers/source explorers for this comparison are closed. Preserve original
untracked `vendor/`, keep `repo-ref/` read-only, reuse `target` and run Cargo strictly serially.
Do not push or open a PR.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Conversion comparison plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Prefab overlay design](../../../../architecture/prefab-overlay-design.md)
- [Unity/Godot ownership study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
