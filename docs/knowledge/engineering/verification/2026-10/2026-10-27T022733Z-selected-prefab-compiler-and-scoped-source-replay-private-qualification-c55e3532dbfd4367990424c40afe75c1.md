---
type: "Verification Evidence"
title: "Selected Prefab compiler and scoped source replay private qualification"
description: "Qualifies selected semantic compilation and exact same-owner replay with 557 passing Scene tests; localization, contextual publication and the Editor conversion workflow remain unimplemented."
timestamp: 2026-10-27T02:27:33Z
record_id: "c55e3532dbfd4367990424c40afe75c1"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-27-prefab-conversion-plan.md"
git_branch: "refactor/prefab-convert-to-local"
git_commit: "13c6fb8"
supersedes: "d4d8d5cf51814d6aa3eb72e85e3d84f3"
---

# Verification

This record qualifies the private compiler/history comparison in the uncommitted working tree
after `13c6fb8` on `refactor/prefab-convert-to-local`. It does not qualify a production conversion
interface, localizer, dependency-fenced conversion publication or Editor workflow. The complete
foundation Goal remains active. `main` remains at `13c6fb8`; no remote operation occurred.

# Result

- The existing staged compiler can interpret one directly owned Scene or Prefab instance while
  borrowing its actual containing topology. Structural opening, retained bindings and selected
  component/value work use the same compiler and ownership scopes. Healthy A does not activate
  unrelated broken B or the independent Prefab link on ordinary local child C.
- The private result contains ordered projected roots, complete selected semantic records, typed
  provenance, omissions and actual source/asset demand. It is not a complete expanded-document
  candidate. Selected unavailable survivors remain distinguishable from complete semantic data
  without native bindings. Real containing IDs/shells still close reference/topology validation.
- The same source history can retain a closed conversion footprint with its exact patch, content
  states and admitted cost. Preparation/replay restrict declaration and created-ID changes without
  relaxing ordinary populated Add, template, deletion or Restore guards. Leaf-first removal and
  parent-first insertion keep the replay closure narrow. Scope bytes count against preparation,
  retained history and observed reverse costs.
- Scene and nominal Prefab tests replay manually localized complete records with no creation
  template or native codec, including read-only fields, through one Undo/Redo entry. Original
  unavailable anchor data, ordinary local children and unrelated instance declarations survive.
  A generic copy of the exposed inverse cannot obtain the private replay scope.
- The history fixtures deliberately isolate replay from localization. They do not generate local
  identities from a real semantic fragment, prove reference translation or retain real Host
  dependency evidence. Those remain the next owner integration.

# Evidence

Cargo ran serially in the checkout with Rust 1.95.0, `CARGO_BUILD_JOBS=1` and the shared root
target. All workers were frozen before each formatter/build; no Cargo command is now live.

| Scope | Actual result |
| --- | --- |
| Focused selected compiler/history cohort | `e4db9abe-ba9d-4fad-a5fb-cd40622a54d4`: 24 passed, 321 filtered; build 12.20s, execution 0.700s |
| Complete Scene all-feature package | `844a0bf8-3288-435d-9bcf-39d92929a06f`: 557 passed, 3 skipped across 28 binaries; build 1m53s, execution 21.125s |
| Minimal Scene library | `cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked`: passed in 1.87s |

The new tests are 11 selected compiler cases and six source-history cases; the 24-case selection
also includes existing related controls. Cohorts overlap. The three skipped observations were
not executed or counted as passes. No root/Editor, independent-consumer, Reference Game, native
presentation, performance or allocation-instrumentation run was repeated for this private unit.
The earlier source-intent qualification keeps its own distinct source/platform scope.

The first build failed on test-fixture API assumptions (`AssetRef: Ord`, a nonexistent Schema
builder/capability); these were corrected without changing production reference traits.
Run `b0e279b0-6f96-4ee1-9ab5-00bd68350702` then passed 23/24 and exposed a fixture assertion that
incorrectly expected the intentionally detached declaration to remain in the complete manifest.
The corrected assertion checks every unavailable record and every unselected declaration exactly,
the selected declaration's removal, and complete original-source restoration after Undo. No
production guard was relaxed to satisfy that assertion.

Read-only reviewers McClintock (`01a17b1c-3ad3-7410-83f9-10d716b7bebc`, source history/admission)
and Carson (`01a17b26-f3f8-7860-99aa-5c767fc77acd`, selected compiler/demand/budgets) found no
remaining actionable defect in their scopes. They did not independently execute tests or measure
allocations. Worker Noether and both reviewers are closed; the parent owns all remaining writes.

# Follow-up

Do not commit or merge the new private mechanisms as a complete owner workflow yet. The
non-test Scene library currently reports eight new unused-helper/variant warnings because the
selected compiler and conversion footprint are consumed only by the experiment tests. Do not
silence these warnings or add artificial callers; connect the actual source-owned localizer.

The immediate next unit must invoke selected compilation on the actual current session document,
not accept an independently assembled fragment/provenance as mutation proof. Generate fresh
source-local IDs using the existing source owner; translate only Schema-declared references;
preserve anchor/raw local data and local-child instance scope; lower one exact source transaction
and retain its closed replay footprint. Return a bounded typed remap for review. An empty fragment
still needs one exact declaration/history operation. Refuse surviving unavailable projected data.
Settle explicit current Schema-owner claims for newly localized interpretable unclaimed records.

Admit raw records, both declaration snapshots, remap/provenance retention, rewritten-reference
growth and replay costs before copies. Reuse existing cost/traversal owners. Reconcile the owning
ADRs when selecting the complete production contract. Keep the actual selected file/metadata and
working-copy evidence through future Host preparation/submission; a plain prepared-edit adapter
cannot silently erase that context.

After the private complete localizer workflow succeeds, qualify independent Scene/Prefab
consumers, stale root/Schema/dependency refusal, actual Save/Reopen and Editor review/history.
Foreign-source navigation, shared Canvas capture, inherited Inspector selection, outer-only
unpack, cross-source Apply, repeated Schema work and the remaining foundation waves stay open.

Preserve original untracked `vendor/`, keep `repo-ref/` read-only and continue Cargo serially.

# Citations

- [Conversion plan](../../../../plans/2026-10-27-prefab-conversion-plan.md)
- [Integration progress](../../progress/2026-10/2026-10-27T021313Z-selected-prefab-semantic-compilation-and-exact-history-footprint-comparison-in-progress-d4d8d5cf51814d6aa3eb72e85e3d84f3.md)
- [Comparison baseline](../../progress/2026-10/2026-10-27T014146Z-complete-prefab-conversion-source-constraints-and-comparison-plan-48bc7f5499704cd39c2f617a29916625.md)
- [Source-intent qualification](2026-10-27T012127Z-source-owned-prefab-intent-review-and-revert-qualification-4b14705bceb545e79f34b7afbc5144bf.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
