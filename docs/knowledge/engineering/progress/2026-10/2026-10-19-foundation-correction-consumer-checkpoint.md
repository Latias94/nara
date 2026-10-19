---
type: Work Progress
title: Foundation consumer acceptance and final integration checkpoint
description: Public consumers and saved-data migrations pass while actual Export and external native runner proof remain active.
timestamp: 2026-10-19T10:02:00Z
record_id: 1e3512867c3b4a6a9a2418b79ca77b62
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Active Contract

The goal still covers U1-U15. Tile tools, new execution frameworks, push and PR creation remain
outside the authorized work. Async/interface research asks where preparation should be asynchronous;
it does not ask to convert owner-thread traits to async. G9 now records that distinction and the
private module extraction map. The plan's GPU retirement description was corrected to distinguish
cumulative frame/transaction demand from physical submitted-resource retirement.

This record succeeds the earlier
[integration capture](2026-10-19-foundation-correction-integration.md) for current continuity.
That immutable capture's pending U11 removal IDs and U14 red/green work have since been resolved.

# New Commits And Consumer Evidence

- `7bec88f`: recursive Inspector drafts. Ordinary Editor cohort passed 147 with 16 native cases
  ignored. The actual native recursive Scene/Prefab workflow and three critical trajectories
  passed in `8a069d68-50e9-4b1e-af38-0b3bafd34ed9`.
- `8cd3eb6`: expanded Scene candidates associate document/provenance before materialization.
  Candidate rustdoc compile-fail tests passed 2/2. Earlier Scene 194, root retention/boot and
  public-boundary evidence now have independent consumer and Reference Game integration evidence.
- `dbe2900`: aggregate input lifetimes, non-reverting staging and the real WASD/Arrow consumer.
  Complete Input/Gameplay cohort passed 97, and the Reference Game's zero/multiple-tick,
  context exit/restoration and fresh-press trajectory passed.
- `b0648b6`: closed finite-F32 Vec2 records, explicit Reference Game component v2/catalog v5
  lineage and private erased codec trait. Historical catalogs remain unchanged; current bundles
  and packaging include the exact successor. The built `target/debug/authoring_bundle.exe --check`
  passed at 09:56 UTC; that executable was built at 09:47 UTC from the current behavior, before
  subsequent formatting and private-comment removal. This was a built-tool check, not another
  Cargo invocation.

External workspace run `68c7c4d9-caf4-4dfb-b476-7992e761d63a` passed 12/12 across Scene, Vec2,
manual codecs and the separately compiled Editor companion. Reference Game run
`af3ccd7d-0f69-4cef-8587-65113fc5bbbc` passed 38/38 after two source-explained fixture corrections.
See [public consumer integration](../../verification/2026-10/2026-10-19-foundation-public-consumer-integration.md)
for exact scope, prior failures and independent lockfile limits.

Renamed-root presentation-ui fixture run `65a438a9-c570-45fb-94ea-12f946587641` passed its one
ordinary ProductRecipe/default-owner case. Root font configuration, strict Check and managed
Editor Play already passed in the 224-test cohort `3fa70695-ee5e-4ecb-9953-a013eb0c04bd`.

U13's native state/GPU reconstruction rerun passed as recorded in
[U13 evidence](../../verification/2026-10/2026-10-19-foundation-u13-stateful-panels.md).
The external companion's embedded interaction passed, but its compiled public runner alone did
not prove plan scenario 3. A new external binary now calls ordinary `composition.run` on its main
thread, emits a real selection, observes it on a later native frame and requests normal Exit.
Its ignored parent test isolates child preferences and has a bounded failure watchdog. It awaits
root execution; a brief ordinary visible Editor window is expected, without global input injection.

# Research And Simplification

The [Pele study](../../../../audits/2026-10-19-pele-interface-and-theming-study.md) uses official docs
and pinned source. It supports semantic theme roles, metric-affecting invalidation and instance
retirement. Lazy module loading does not prove off-thread rendering. No dependency or additional
framework was introduced. The standalone HTML outside docs now distinguishes baseline findings
from current focused implementation and pending gates.

Five simplification lenses completed read-only inspection. Root applied reuse of the existing
TileSet source-kind helper, one per-paint shape-support calculation, and removal of private
implementation/history comments. Safety checks, public contracts and proof tests/helpers remained
intact. Their final check results still belong to integration. A possible stronger exact-value
oracle in the external Vec2 proof was noted without changing or deleting that protected proof.

# Running Work And Next Actions

Root is the sole Cargo owner, using Rust 1.95.0 and CARGO_BUILD_JOBS=1 in the shared target.
Session `67444`, nextest run `39fba8cf-ed30-41a2-ad3e-0599921dc6a0`, is executing U12's actual
compiled Check, release Export and packaged Headless/Desktop startup. Its subprocess builds are
cold and still active; no result is claimed. Do not start another Cargo while it runs.

After that result: run the external native runner test; format/review its added fixture; run the
two feature-gated Image preview regressions with presentation-2d; complete serialized workspace
all-features/all-target checks and appropriate configured lint. Finish the actual ce-code-review
workflow, which has not yet produced a formal receipt. Resolve findings, update final evidence,
ADR ledger/audit/HTML, and make the remaining precise commits before completing the goal.

The index currently contains both U12 and U13 paths, staged for inspection but not committed.
Keep those units separate when committing. Preserve unrelated untracked myeasylog.log and vendor/.
No destructive Git operation, source cleanup, push or PR has been performed.
