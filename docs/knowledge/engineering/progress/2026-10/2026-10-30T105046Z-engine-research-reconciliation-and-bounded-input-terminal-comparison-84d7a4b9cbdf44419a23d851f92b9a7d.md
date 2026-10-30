---
type: "Work Progress"
title: "Engine research reconciliation and bounded Input terminal comparison"
description: "Current-HEAD research dispositions, five reproduced Input failures and selected shared-credit design; production replacement remains open."
timestamp: 2026-10-30T10:50:46Z
record_id: "84d7a4b9cbdf44419a23d851f92b9a7d"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "b939da2"
---

# Summary

Continue the complete active foundation Goal on `refactor/foundation`; local main and the branch
started this unit at `b939da2`. Processed Input was already qualified and merged locally. The new
work does not repeat it or close R10, target delivery, rendering or the self-drawn Editor.

The user supplied the October 30 engine research archive during the Input endpoint replacement.
Import only its ten session research documents, verified against their manifest bytes/SHA-256.
Preserve all current supporting sources and different local reference revisions. The
[current-HEAD disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
maps every research lane to implemented, planned, refined, deferred or rejected scope and updates
the existing foundation plan rather than creating another omnibus roadmap.

# Executed Input Evidence

The expanded red baseline used actual `RuntimeInstance`, ordinary driver ingress, frame cleanup
and physical/semantic observations. Nextest run `c912b9a5-bc85-4b02-ae64-63fee80d37f4` executed six
tests: **one passed, five failed**, with 51 unselected. The passing mixed-output trace confirms
six focus-loss records, one CloseRequested record and one SurfaceRetired record, ordered once.
The failures are:

| Regression | Actual predecessor result |
| --- | --- |
| Balanced keyboard/pointer churn | Retained 64 empty keyboard and 64 empty mouse rows after logs drained. |
| Cross-frame held growth | Admitted 4,097 distinct held pointer memberships while each frame's log/transitions drained. |
| Focus loss after live growth | A 4,097-record terminal batch refused after 4,096 holds; ingress rows cleared while ButtonInput and SemanticActions remained pressed. |
| Loss at binding-generation exhaustion | Focus loss required a new generation, failed and left physical/semantic state held. |
| Geometry loss with another prepared route | Returned generation exhaustion with `route_invalidated: false`; the affected route remained bound while its position had already changed. |

Earlier run `ea435e87-5cbd-42e8-8a6d-4d86741df626` had three clean failing cases. The first fixture
attempt asserted before Runtime finish and caused an abort; it was corrected to capture values,
finish, then assert. That fixture error is not evidence of a production memory failure. The
expanded six-test run above is the useful final red baseline.

Confucius delivered the private `timeline/capacity.rs` count ledger and test-only dedicated-pool
control. The parent wired it under `cfg(test)` and executed:

```text
cargo +1.95.0 nextest run -p nara_input --all-features --locked --lib
  -E 'test(timeline::capacity::tests::)' --test-threads=1 --no-fail-fast
  --status-level fail --final-status-level fail
```

Run `ad6abf2b-2496-4043-bb5b-0ce2fd2cae33`: **8 passed, 57 unselected**, build 4.67 s, tests 0.227 s.
`CARGO_BUILD_JOBS=1`, root target and serial Cargo were preserved. These tests cover pure planning,
stale/double spending, exact/one-short mixed eight-terminal/nine-shared capacity, lagging retained
prefixes, geometry plus an unpositioned held pointer's future leave, release/control retention,
actual reclaim versus refund, checked overflow, final sequence headroom and dedicated merge order.
The model contains no actual endpoint topology or consumer cursor implementation. It establishes
the bounded comparison; it does not turn the five predecessor regressions green. Its test-only
module and eight controls are committed separately as `9886635`; the red production baseline
remains uncommitted development input for the owning replacement.

Temporary logs remain outside Git: `nara-input-endpoint-expanded-baseline.log` and
`nara-input-terminal-capacity-model-tests.log` under the task's temporary directory. No new native,
whole-workspace or broad Input qualification was performed for this document integration.

# Selected Design And Review Findings

The Input successor selects explicit bounded raw-ID connection/retirement, private non-reused
record/member identity and one physical/synthetic ingress owner. Adapters must drain/discard old
external operations before label reuse; pending internal references prevent recycling. Current
Winit delivery and Editor request fencing were source-reviewed. They do not prove arbitrary
native recreation or retained external batch replay, and a newly minted public handle would not
recover missing origin. `ButtonInput` becomes read-only and semantic links derive from admitted
members. The direct mutation queue and parallel synthetic resolver are removed by that replacement.

Shared terminal credits retain one ordered log and all its existing consumers. The model's complete
post-operation retained-plus-obligations and sequence equations passed; the dedicated-pool control
adds a second storage partition and ordered merge without fixing any missing topology. Actual
release/leave/control rearming and all-consumer retention still require production implementation.
Revocation/unbind allocates no new route generation and cannot rely on global poison.

Boyle's Input/UI review exposed another required owning change: loss may occur after route/UI
prepare while the token remains pinned. Preserve immutable declaration separately from mutable
invalidation; a total Published/Invalidated commit consumes it without restoring cancelled
focus/capture/reducers. Returning a stale complete stamp is insufficient because UI installs a
whole prepared session and root then publishes observation/accessibility success. ADRs 0041,
0102 and 0108 now adopt that successor; the changed public checkpoint stays open. Backend pixel
acceptance and interactive publication remain distinct. Production code is not yet changed.

Independent read-only research lanes:

- Euler (`01a18c64-84d2-7731-b0b8-a50078472233`) confirmed RE1-RE6 against current source. The
  phase comparison must use joint motion before freeze, executor selection must be explicit,
  borrowed same-tick solver jobs differ from Tasks and narrow Ack differs from whole-tick success.
  Spatial scan costs remain an Accepted tradeoff, with affine import/KeepWorld conditionally open.
- Hypatia (`01a18c64-a21a-76d2-9e05-1da4a1f1ca7e`) confirmed existing semantic editing, panel-bound
  command contribution and singleton installed-tool routing. Added closed-panel Snap, two-tool
  coexistence and game-checkpoint/supervised rebuild/restore consumers, with external-author and
  actual platform evidence. Static erased objects do not prove executable unload.
- Boyle (`01a18c4f-92ca-7be0-835f-65b8dd1ad26e`) retained the selected finite render graph, then
  refined exact output versus display attempt versus one-shot UI completion. Admission designates
  one exact interactive completing attempt; passive mirrors carry no producer interaction/IME/
  actionable accessibility and cannot complete or revive it. Downgrade rejects old actions without
  gameplay leakage; passive failure has occurrence scope. Proposed 0121 and R8/R11 carry that
  acceptance without claiming implementation.

The parent verified content CP1-CP5 against actual Schema/overlay/admission/preparation owners and
separated validated destinations from current demand, and immutable authority from encoded backing
and decoded/native residency. R5 now names claim/address/history evolution plus interrupted upgrade
recovery. R6b records the durable execution recipe freeze gate. OQ-010 follows the already selected
self-drawn Editor and first editable Inspector. No ECS replacement, universal kernel, remote cache,
all-platform promise or new migration guide was selected.

Scoped follow-up reviews accepted the Input terminal/prepared-loss contracts and render completion
text. Boyle's nonblocking case was added: the passive mirror may display while the designated
completer is hidden/fails before its first success, settling the original permit once without
blocking another producer. Euler caught and rechecked two integration contradictions: D07/R4c
now leaves narrow Ack versus stronger whole-tick completion unresolved until the common failure
oracle, and R4c's actual solver-job/executor freeze gate is separate from R10's generated-content
authoring qualification. A parallel comparison is explicitly not production parallel support.
The final limited rereviews report no remaining blocker in those scopes; no fresh product test
is inferred from a document rereview. All four agents are frozen and closed. `cargo +1.95.0 fmt
--all` completed without changing unrelated Rust files.

# Next Action

Finish the Input owner replacement under the selected plan. The capacity comparison is the
test-only commit above; keep `timeline/retention_tests.rs` and its module declaration as uncommitted
development input until integrated and appropriately verified. Do not hide or weaken the red cases.
Implement actual finite
topology/member/projection lifetimes and shared credits together, migrate synthetic/physical drivers,
UI/root/manual total commit outcomes and all real consumers, remove obsolete writers/poison, then
run focused owner/public/independent/native checks and review the complete replacement.

Next architecture decisions remain the full foundation plan's responsibility: validated Scene demand
before travel/package freeze, meaningful Schema/source refactors, joint motion/executor selection,
finite render graph and editable custom UI, complete Rust rebuild/resume and target delivery. The
Goal remains active. No push or PR is authorized or performed. Preserve `vendor/` and references.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Endpoint plan](../../../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
- [Imported research and receiving disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Processed Input predecessor qualification](../../verification/2026-10/2026-10-30T100534Z-processed-startup-input-ownership-and-prebuilt-source-free-physical-process-qualification-35c2432f89d04982b10d2be3fa0f9d12.md)
