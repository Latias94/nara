# Owned Background Editor Save

Status: Completed within the verification and measurement limits below.

Baseline: `48f1d2b`. This implements the measured Save slice in the
[responsiveness investigation](../benchmarks/2026-10-20-editor-responsiveness.md).

Delivery evidence is recorded in the
[verification record](../knowledge/engineering/verification/2026-10/2026-10-20T013307Z-owned-editor-save-custody-native-observation-and-measurements-9828019f2f934061a1773899ce356ffb.md)
and the [v2 measurement report](../benchmarks/2026-10-20-owned-editor-save.md).
Implementation: `78c8ddd` (owned Scene/Prefab readmission), `1a90307` (background Save,
physical custody, native lifecycle, consumers and measurement). Reopen remains a separate slice.

## Scope And Contract

Keep ordinary Save actions, linear captured checkpoints and existing Scene/Prefab persistence
hosts. Encode, authoring re-admission and filesystem publication operate on owned inputs in the
existing authoring task pool. Workspace acknowledgement stays on the session owner. The original
file host, its uncertain state and temporary cleanup remain in retained custody independently of
the task's logical terminal. No World, Workspace, live session or history entries move to workers.

Save may take several frames. Later edits remain possible and dirty after the captured revision
is saved. Result acknowledgement must bind the exact original document/source and preserve the
existing uncertain-publication outcome. Save has no new public Cancel command. Close waits for
physical work and file cleanup; task cancellation cannot erase a publication result.

Reopen remains synchronous in this slice. Project Open, Browser Open, automatic code-currentness,
journaling, complete project transactions and a generic asynchronous service are out of scope.
ADR 0091 remains Proposed; Accepted ADRs 0047/0052/0070/0118 own the affected contracts.

## Delivery Sequence

1. Record focused encode / authoring validation / file-publication attribution on the owned
   worker path using the existing optimized workload and private temporary probes. This separate
   exploratory run cannot retroactively attribute the frozen synchronous baseline.
2. Capture a narrow owned authoring readmission basis and reuse current validation logic. Provide
   one lower Save operation for native and data-only Scene/Prefab owners.
3. Add one private retained operation using existing task pools. Preserve Busy/Saving until the
   physical job returns its original host. Handle rejection, panic, failed writes, uncertainty and
   close without losing custody.
4. Update native maintenance and presentation of completed persistence. Defer/retry Close/Exit
   and replacement intents that encounter a still-pending Save.
5. Migrate consumers to bounded terminal waits. Add deterministic held-work cases for later edits,
   delayed publication, failure cleanup and session close. Verify native and data-only Scene/Prefab
   save bytes, source identity and receipt acknowledgement.
6. Run focused owner/public/native checks serially, compare optimized owner-drive costs separately
   from operation latency, review independently, update ADR/guide/evidence/HTML and commit.

## Ownership

- Lower-owner lane: Scene readmission capture and root persistence host implementation.
- Native lane: application close/replacement, maintenance and result presentation.
- Root: private background custody, session integration, consumer migration, measurement and
  authoritative serialized verification/commits.

## Acceptance Limits

The prior 8 ms target is a local investigation trigger, not a timing assertion or product SLA.
Separate request latency, maximum owner drive and operation completion time. Source-receipt
refresh after a bound external Play and Prefab runtime-content refresh retain their separate
costs; do not claim that moving filesystem work eliminates every Save-time owner cost.
Physical close must be proved independently of task terminality. Native qualification covers
only affected executed trajectories and does not extend release or other-platform claims.

The v2 maximum-drive metric is the per-operation maximum across all owner calls, rather than the
v1 single synchronous call. Completion includes the measurement driver's 1 ms polling, queueing
and acknowledgement, and does not estimate native UI latency under its 16 ms maintenance policy.
The experiment does not measure total owner CPU work. Two warm-ups create and close fresh
sessions/pools; they warm code/storage/system state rather than one persistent worker pool.
