# Editor Persistence Host Lifetime Custody

Status: Completed within the [qualification scope](../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md).
Baseline: `6071425`.
Implementation: `fe3ad96`.

## Outcome

Give each admitted Scene/Prefab file Host one private retirement owner for its entire lane
lifetime. Failed temporary-file cleanup must survive operation completion, lane replacement and
session abandonment. Close must remain incomplete until physical work and cleanup retire. Reject
new work after close begins, including an already stopped authoring runtime. Preserve exact file
capabilities, source bindings and conflict baselines; do not reconstruct authority from a path.

Remove operation-scoped Save custody once its responsibilities belong to the lane owner. Keep
ordinary public actions, owner-thread document publication and existing task retirement. Do not
add a generic operation framework, fake idle Save requests or new public async traits.

## Lane Map

Mode: execute_direct. Shared branch `refactor/foundation-contract-hardening`.
Root alone runs Cargo, formatting, staging and commits. Use shared `target` and
`CARGO_BUILD_JOBS=1`; preserve unrelated `vendor/` and `myeasylog.log`. No push or PR.

| Lane | Writable scope | Responsibility |
| --- | --- | --- |
| Root | Persistence/session implementation, focused tests, plan and contracts | Design and integrate lifetime custody, serialize verification and commit |
| Lifecycle challenge | Read-only | Counterexamples across failed Save, Reopen, replacement, Drop, runtime close and admission |
| Lower owner research | Read-only initially; assignment required before edits | File cleanup ownership and fault-control/test seams; propose smallest coherent repair |
| Remaining findings | Read-only | Reconcile existing audit findings and deferred triggers against current source; identify concrete unfinished defects |

## Acceptance

1. Deterministically retain cleanup after a terminal failed Save, then prove abandonment preserves
   retry authority until release. Cover Scene and Prefab without exposing test-only public APIs.
2. Cover failed/stale Reopen followed by Drop, lane replacement, explicit close/retry, queued/running
   Save and admission after close starts or completes. Logical task completion cannot prove a file
   call returned.
3. Independently review lifecycle ownership and check affected library/public Editor consumers.
   Run focused nextest and relevant workspace checks serially; record actual scope and limits.
4. Update the owning ADR/ledger and standalone HTML reports, capture verification and commit.
5. Reconcile remaining confirmed findings before choosing the next slice. Capability limits and
   evidence-triggered features are not automatically implementation requirements.

## Disposition

The focused final engine/public cohort passes 125 tests, the Editor/application cohort passes
189 tests, and three selected actual Windows/DX12 native trajectories pass. Workspace all-features
all-targets check, formatting and diff checks pass. Two integration reviews find no current
reachable blocking defect. The qualification record preserves commands, run identities and limits.

The selected N/I correctness corrections are closed within their evidence. The next measured
responsiveness trigger is Project Open startup-closure admission, which remains synchronous.
Browser admission, source receipts, task capture disposal and high-batch GPU retention require
their own measurements; multiple same-kind documents and Tile tools remain separate capabilities.
