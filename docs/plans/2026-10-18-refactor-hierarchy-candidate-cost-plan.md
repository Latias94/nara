# Reduce Hierarchy Transaction Candidate Cost

## Objective

Address measured F11 cost before larger consumers rely on the current whole-topology candidate.
Preserve ordered sequential batch semantics, failure atomicity, lifecycle preflight, deferred-work
baseline, non-linked entity lifetime and completed projection freshness. No new public API,
parallel Cargo work, dependency, physics/Tilemap feature or transaction merging is required.

Baseline: `e48b4b3`; [retained measurements](../benchmarks/2026-10-18-foundation-scale.md).
At 4,096 members, the old independent-eight P95 is 4.41–5.13 ms, versus batch-eight 1.04–1.06 ms;
trusted-generation independent-eight is 0.49–0.53 ms across three complete processes. Requested
allocation traffic falls from 11,428,052 bytes per cycle to 72,884 bytes per cycle in
the trusted run. This is an investigation result, not a platform SLA or capacity promise.

## Lanes And Ownership

- Root owns all source, tests, documentation, measurements and serial Cargo verification.
- Runtime/content reviewer reads generation, invalidation and Scene preparation contracts.
- Render/world reviewer reads overlay alternatives and ordered/cycle counterexamples.
- Editor/production reviewer reads real consumers, regression coverage and measurement protocol.
- Reviewers remain read-only; `AGENTS.md`, `repo-ref/`, `vendor/` and `myeasylog.log` are not modified.

## Work

1. Establish which validation proof can survive managed commits and which raw changes invalidate it.
2. Add adversarial regression coverage for sequential candidates, invalidation and unchanged failure.
3. Replace whole-world candidate copying with affected-owner state if it simplifies the measured path.
4. Reuse proven validity only within the hierarchy owner; preserve independent full validation.
5. Run hierarchy, transform, Scene and affected root/reference consumers serially.
6. Repeat unchanged timing and allocation protocols; retain raw evidence and independent review.
7. Update the owning ADR/status, audit/HTML and engineering evidence with actual limits; commit.

## Verification

Keep full topology validation and independent ordered/global oracles. Cover ancestor cycles through
earlier operations, remove/re-place, root/child movement, stale anchors, lifecycle rejection,
raw relation/member/despawn corruption, root-resource removal/replacement, deferred work and
private Scene construction/retirement. Failed transactions must not publish topology or completion.
No general memory or native Editor qualification follows from headless measurements.

## Progress

- Baseline and owner contracts read; three independent read-only review lanes are active.
- Source identifies repeated full validation and full candidate collection as distinct costs.
- Level-1 overlay plus Level-2 private baseline epoch are implemented; 31 Hierarchy tests, three
  trusted timing processes and trusted allocation evidence pass. Prepared Scene paths retain full
  validation and the owner-controlled no-flush/no-hierarchy-mutation commit window.
