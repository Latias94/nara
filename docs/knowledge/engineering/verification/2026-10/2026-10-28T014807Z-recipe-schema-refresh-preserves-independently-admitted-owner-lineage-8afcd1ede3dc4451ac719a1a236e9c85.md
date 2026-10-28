---
type: "Verification Evidence"
title: "Recipe schema refresh preserves independently admitted owner lineage"
description: "Qualifies bounded owner-preserving refresh and records the reviewed source/history successor design without claiming Session adoption."
timestamp: 2026-10-28T01:48:07Z
record_id: "8afcd1ede3dc4451ac719a1a236e9c85"
producer_id: "codex-root"
run_id: "authoring-owner-adoption-2026-10-28"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "feat/authoring-owner-adoption"
verified_by: "01a18026-2d3a-7231-a075-7e2566ec99fc,01a18030-a7b0-7520-a3ab-c4446e11525c"
supersedes: "3a4a90d3cc3042e5b89e8fa20e314c22"
---

# Verification

Continue the full active foundation Goal after `daead17` was committed and fast-forwarded into
local `main` (ahead of origin by 166, no push). The current branch is
`feat/authoring-owner-adoption`. Only the original `vendor/` was untracked before this new unit;
it remains untouched. Cargo is serial with Rust `+1.95.0`, one build job and the existing root target.
This prerequisite correction does not implement the full new-owner adoption workflow.

# Result

The compiled recipe schema refresher previously encoded only its own bundle and replaced all
existing project metadata, deleting independently admitted owners. It now boundedly decodes an
existing bundle, selects complete recipe owner declarations, and retains every complete owner
absent from this invocation's recipe. Reflect's existing `AuthoringSchemaBundle::new` validates
the complete successor, including immediate predecessors and reserved/tombstone claims.

Missing files still permit recipe-only creation. Invalid/oversized existing input is not an empty
bundle or permission to erase it. `BundleInvalid` and `BundleComposition` have distinct bounded
static compiled-tool failure codes and protocol round trips. Both Check and write refuse before
publication on invalid input or conflicting claims. A semantically identical admitted successor
returns no-change, retaining original JSON spelling, bytes and the live leaf. Real changes use
the same retained-file publisher and detect-only conflict contract.

Non-recipe is a set difference for the selected invocation, not a permanent ownership category.
An owner explicitly selected by a future compiled recipe follows that recipe's existing authority.
Retaining metadata grants no Runtime binding or executable approval. A public ordinary Check
accepts the resulting bounded metadata superset independently from selected Runtime definitions.
No new validator, provenance registry, compatibility alias or migration guide was added.

Independent source review found a further publication gap: two individually readable owners can
compose below the 16 MiB encoded ceiling while exceeding the reader's 8 MiB aggregate string
allowance. `AuthoringSchemaBundle::to_json_string` now invokes the existing format-neutral shape
preflight after encoding, sharing the reader's complete default limits without decoding another
bundle. A pair of 35,000-alias inputs proves each can encode/read independently while the combined
result rejects specifically at TotalStringLimitExceeded. The actual refresher counterexample
checks refusal before replacing the existing leaf or creating a temporary publication.

The public Check cohort also executed a previously uncovered selected-value diagnostic regression:
unregistered components correctly refused materialization but were mislabeled migration failures.
The staged compiler now reports `scene.unknown-component` for selected non-opaque missing
definitions before owner/migration interpretation. This remains after structural/value selection;
discarded source payloads and Authoring's opaque unavailable preservation are unchanged.
The existing public Check assertion remains intact, with a direct public Scene expansion regression.

## Next-Unit Design Review

The new owner-adoption plan traces the actual missing workflow: current installed tools assume
pre-admitted metadata, and no source-preserving whole-Session Schema transition exists. The
bounded review found five concrete design gaps before source transition APIs were implemented:

1. A history entry retains one direction only. Adoption must run both directions through a
   shared actual-history preparation/cost kernel, with `history=true`, original scope, frozen
   inverse and exact source restoration/regenerated patch equality. Ordinary edit preparation
   would reject legitimate owner-created RestoreEntity. Do not clone history and call public Undo
   to manufacture a proof that changes nominal identities or stack state.
2. Small retained patches do not bound repeated full-document copying/classification. Add a
   narrow proof allowance that separately bounds cumulative work and peak retained bytes, charges
   before copy/replay, and never retains every historical source snapshot.
3. A project successor needs the same single admitted successor basis as the documents. The
   current startup admission creates a new basis, so it cannot be called unchanged. Reprepare
   actual dependencies, startup, dirty open Prefab working copies, assets/database, unresolved
   facts, stable lookups, readiness/diagnostics/usage and the index bound to the bundle leaf.
   Closed raw Prefabs remain actual-demand inputs rather than forced standalone publications.
4. Whole-set adoption requires exclusive complete validation then infallible swaps in the same
   owner turn, not repeated fallible document commits. Disk-successor/live-predecessor recovery
   needs explicit fresh review/reconciliation, never false AlreadyAdmitted or discarding dirty work.
5. Admission must fence automatic restarts and distinguish conflicting writes from stale read-only
   results. An always-active renderer cannot make adoption permanently Busy. The adoption turn
   must invalidate preview stamps immediately because source revision deliberately stays unchanged.

The plan now includes those requirements and distinguishes source-local EntityRef closure from
composed Prefab references and Host asset availability/degraded policy. These are reviewed design
requirements, not implemented or executed new-owner adoption. Owning ADRs 0026/0090/0047/0114/0118
must be reconciled before changing those boundaries. This source unit amends ADR 0110's refresh
authority and ADR 0114's bundle encoder/reader shape agreement, without adopting the broader
whole-Session transition.

# Evidence

- Final focused root schema refresh, compiled failure protocol and selected data Runtime agreement:
  15 passed, 678 excluded; `f0dceea1-cd09-4481-adb8-2378d3b82ff0`; 3.743 s after a 1 m 12 s build.
  Includes actual file writes/Check, complete external generation-two lineage, recipe addition and
  owner replacement, no-change lexical/leaf preservation, four current/tombstone claim collisions,
  invalid input preservation, composed shape refusal and public Check over the retained superset.
- Before execution, parent review corrected tests equating FileIdentity across two independent
  capability sessions. Each now captures its own live expected leaf and compares only within that
  session; this is not weakening actual object continuity or changing rights/trust.
- Reflect bundle public integration: 7 passed, 0 skipped;
  `e009b5f1-b2a3-4fcb-8d5a-b42298049819`; 1.960 s, including the exact aggregate-shape refusal.
- Public Scene Prefab expansion: 9 passed, 0 skipped;
  `e7520b6c-8aba-4f34-a27f-846848c84822`; 0.445 s, including precise missing-definition diagnosis
  and existing selected/discarded dependency controls.
- The first complete Reflect/Scene cohort passed 996, failed one internal generic-diagnostic
  expectation and skipped 3; `2d533001-48f2-49b8-a2b0-b08d75e5f443`, 60.456 s. Only its missing
  component branch now asserts the exact unknown-component code, component ID and Component failure
  kind. Noncurrent-version and foreign-owner rejection assertions remain unchanged. The final
  complete cohort passes 997 and skips 3; `9cc38ece-7cef-45d5-afd0-4e1b291b99ba`, 56.454 s.
- The first public Check/CLI cohort passed 10, failed one exact unknown-component diagnostic and
  skipped 3; `7c5ffda6-5183-48c2-a71b-134910fbc881`, 68.137 s. The final corrected public cohort
  passes 11, skips 3; `8ea35af8-1e16-41fd-984e-607bad68ab82`, 14.938 s, preserving the assertion.
- Root/Reflect/Scene all-feature/all-target Clippy with `-D clippy::correctness` passes in
  1 m 54 s. Existing non-correctness warnings remain; no mass lint rewrite was performed.
- Feature-minimal root `runtime-2d,serde,tooling` library and no-default Scene library checks pass,
  respectively 7.10 s and 1.30 s; existing feature-conditional warnings remain.
- `cargo fmt --all --check`, Git diff checks, affected Markdown local links and engineering-memory
  render/validation pass during integration. Historical memory warnings remain advisory.

The owner shape fix is committed as `464277e`; selected-value diagnostics as `3eb083e`. The
containing refresh commit adds the concrete CLI correction, owning documentation and this evidence.
They form one verified local integration chain without remote publication or native UI changes.

The final focused source review confirmed the aggregate encoding fix and selected unknown-component
diagnostic placement, with no remaining concrete P1/P2 in its assigned scope. It did not execute
tests or qualify the future adoption plan's APIs.

# Follow-up

Implement the owner-adoption plan's source/history allowance and bidirectional prepared transition
first, then exact project receipt custody, whole Workspace adoption and Session recovery. The
ordinary new-project installed tool consumer must eventually stop pre-admitting EnemyStats.
Existing-owner evolution/value migration, installed dependency closure/content preservation/removal,
source-free delivery, rendering and Nara-owned UI remain independent full-Goal obligations.

The previous installed-selection verification remains at `daead17`: root/public persistence 702,
Editor 245, filesystem 87, Tooling 86, independent language 24, actual Application and four selected
native trajectories. Those source-specific cohorts are not newly executed by this CLI correction.
The accidental nested fixture target still remains after automatic policy rejection; do not bypass
that rejection. No Cargo is currently needed outside the root target.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [ADR 0110](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
- [Installed selection evidence](2026-10-28T012206Z-durable-installed-source-tool-selection-and-independent-paired-attempt-qualification-3a4a90d3cc3042e5b89e8fa20e314c22.md)
