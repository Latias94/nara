---
type: "Work Progress"
title: "Low-level contract redesign research and independent challenge"
description: "Proposed owner-preserving transfers, phase spatial snapshots, owned locks, asset intent and typed codecs; source-confirmed gaps and consumer migrations."
timestamp: 2026-10-22T16:54:57Z
record_id: "ea78e11f9cd34f21a77c8eda0239f9ac"
producer_id: "codex-root"
run_id: "low-level-contract-redesign-20261023"
git_branch: "test/compiled-asset-domain-export"
git_commit: "74cbe3ba5586944bf24c60c11c06f441ae001e6b"
---

# Summary

The user authorized continued first-principles low-level research, recording all prior findings,
fearless pre-1.0 redesign and further independent architectural audit. The resulting
[proposal](../../../../architecture/low-level-contract-redesign.md) remains Proposed. No product
code, persistent format or Accepted ADR was changed; no Cargo or product test was run.

# Details

- The [reassessment](../../../../audits/2026-10-23-low-level-architecture-reassessment.md) maps all
  L01-L10 and E01-E06 findings and preserves prior O1-O6 triggered opportunities. It adds the
  non-spatial-parent contract conflict (A01), consume-then-spawn snapshot design pressure (A02),
  single-entity identity retirement's pre-admission extraction (A03) and running statistics ending
  before worker-owned disposal (A04). The new defects are source-confirmed, not newly executed
  regressions.
- D1 recommends App custody through rejected activation plus narrow executor-authored task return.
  D2 selects private phase snapshots, removing Global's Component identity. Historical simulation
  reads seed mutation from Startup/previous publication; Resolve uses this tick's cut. Consumers
  use same-cut effective spatial parents rather than live hierarchy joins. New projectile local
  behavior remains an explicit game rule, not a general missing-global fallback.
- D3 recommends an owned lock capability. D4 separates desired recipe, current attempt and usable
  publication, preserves invalidation scope and verifies store association at baseline capture.
  D5 retains typed candidates through deferred/batch preparation before private erasure. D6
  removes avoidable identity extraction and records the derive path-injection feasibility gate.
  D7/D8 keep numeric/budget corrections bounded and specify success-sensitive finalization.
- The [reference study](../../../../research/2026-10-23-low-level-contract-mechanisms.md) compares
  fixed Bevy/Godot mechanisms and costs, not latest-version superiority or equivalent guarantees.
  Accepted substrate and domain ownership remain; universal transactions/import executors and
  anti-tamper trusted-World wrappers are not selected.
- Independent read-only lanes challenged spatial, ownership/task/FS and asset/codec/identity
  seams; a fourth wrote only the mechanism study. An adversarial reviewer found two P2 migration
  gaps (pre-propagation historical pursuit and non-spatial parent conversion). Both were corrected
  and narrow re-review closed them with no new finding. No other model/CLI was invoked.
- The implementation ledger records confirmed counterexamples without rewriting historical
  acceptance. ADR 0002/0052/0058/0117 implementation coverage is partial; this does not reject those
  decisions or implement the proposed replacements. Other affected partial entries link their gaps.
- Document checks pass for local links/anchors, balanced fences and whitespace. Engineering-memory
  validation passes with retained legacy warnings; derived views were regenerated and checked.
  No production test, native lock qualification or performance result is claimed. All five Codex
  lanes were completed and closed after collection.

# Next Action

Use the proposal's owner slices: custody/lock regression and task-return proof, spatial snapshot
consumer/cost proof, desired Image state plus managed mutation association, typed codecs/facade and
identity, then explicit Gameplay success and integration. Reconcile each affected ADR clause before
changing its contract; remove old public paths in the same slice. Run serial focused nextest with
one Cargo build job and root target reuse. Do not interpret document review as runtime qualification.
The preexisting untracked `vendor/` remains excluded. No active Goal was created or resumed.

# Citations

- [Original low-level review](../../../../audits/2026-10-22-low-level-module-and-interface-review.md)
- [Reassessment](../../../../audits/2026-10-23-low-level-architecture-reassessment.md)
- [Proposed redesign](../../../../architecture/low-level-contract-redesign.md)
- [Mechanism study](../../../../research/2026-10-23-low-level-contract-mechanisms.md)
- [Implementation ledger](../../../../architecture/adr/implementation-status.md)
