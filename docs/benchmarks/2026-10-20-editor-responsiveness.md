# Editor Responsiveness And Task Retirement

This is a decision-local investigation under G1/G2/G9 and ADR 0099. It separates source-confirmed
ownership issues from measured synchronous work. It does not establish a native frame budget,
supported entity-count ceiling, cold-storage behavior or a general asynchronous execution model.
The starting source is `f93552c`; task behavior is `3b70e48` and the measurement consumer/data are
`b93944e`. The [verification record](../knowledge/engineering/verification/2026-10/2026-10-19T182252Z-editor-responsiveness-measurements-and-standalone-task-cancellation-0e2521d0010a4d94940d83fa865f3d0c.md)
records actual commands, run identities, review corrections and limits.

This report retains the synchronous v1 source and data as historical evidence. The subsequent
[owned Save implementation and v2 comparison](2026-10-20-owned-editor-save.md) moves the measured
Save path to retained background work; statements about synchronous Save below describe v1.
Reopen and the other separately identified preparation costs remain follow-ups.

## Ownership Findings

| Operation | Work performed before returning to its caller | Correct future preparation seam |
| --- | --- | --- |
| Project Open | Root capability acquisition, then four synchronous advances: manifest/profile, bundle, startup closure and session publication | Owned source/schema candidates may be prepared separately. Session/runtime construction and frame publication stay on the owner thread. |
| Browser Scene/Prefab Open | Action dispatch reads, parses, validates and binds the new source before opening it or requesting the previous document's close | Owned indexed-source candidate; recheck index incarnation and document intent before publication. |
| Save request | Admission deep-clones the captured document into a linear checkpoint | Capture the intended revision explicitly; a later edit may remain dirty after this save succeeds. |
| Save drive | Encode, authoring validation, identity/digest check, temporary write, file sync, replace, directory sync, Workspace commit and frame projection | Preparation and actual filesystem publication need retained ownership. Cancellation after replacement is not rollback. |
| Reopen drive | Read/digest/decode/validate, source/revision admission, document publication and frame projection | Reject a stale base revision on owner-thread publication. |
| External Play request | Re-admit the saved startup closure and hash the manifest/lockfile before Building is visible | Owned source-receipt preparation with exact attempt/generation identity. |
| Build phase transition | Manifest/lockfile read/hash, bounded metadata decode and artifact admission | OS process spawn and immutable generation publication already have worker owners; measure the remaining synchronous work before extending them. |
| Generation retirement | Filesystem removal of generation work directories | Retain the exact cleanup owner through any background disposal; do not lose uncertain or unfinished cleanup. |

An Open phase boundary is an opportunity to accept Cancel; it is not a wall-clock bound on that
phase. Save and Reopen currently have no public Cancel command. A Future alone would not make
their transaction or filesystem publication safe to abandon.

The existing build receipt observes Cargo.toml/Cargo.lock and authored project content. It does
not prove a complete Rust source, path-dependency, build-script, macro or environment-input closure.
Manual Play invokes Cargo; Restart uses the same immutable executable. Automatic code-currentness
still needs its own finite observation or immutable-source decision under ADR 0116.

## Executed Task Characterization

Standalone pool abandonment previously cancelled pending handles when the reaper took each job
for capture destruction. Same-owner disposal is intentionally serial. Holding the first capture's
destructor therefore prevented the second pending handle from becoming logically cancelled even
though no worker could execute it. Explicit managed close already cancelled pending batches first.

The regression retains two handles and tokens, holds the first capture behind a channel barrier,
and checks both PoolShutdown terminals before releasing physical disposal. It fails on the baseline
with `a blocked capture delayed another pending handle's logical cancellation`. A release guard
unblocks the destructor during failure unwinding; a bounded receive is the final fallback.

The correction publishes pending cancellation at the existing abandonment owner before handing
captures to retention. It leaves physical disposal order, lane count and managed close unchanged.
The public documentation now distinguishes terminal publication, cooperative cancellation,
physical retirement and handle Drop. Dropping a handle keeps admitted work alive, preserving Image
supersession's generation-based publication policy.

Cancellation visits the bounded pending set in O(n); it is not a constant-time or low-latency Drop
promise. Rejected/coalesced captures can be destroyed by the submitting caller, and the last owner
of an untaken completed result may run its destructor. Those user-controlled costs remain distinct
from the corrected cancellation ordering and require a concrete consumer before changing ownership.

## Open, Save And Reopen Protocol

The ignored measurement lives in
[editor_responsiveness_measurement.rs](../../tests/support/editor_responsiveness_measurement.rs),
next to the existing persistence fixture and public action driver. It introduces no production
timing API, scheduler or telemetry storage.

- Windows 11, Intel Core i9-13900KF, 24 cores / 32 logical processors, 68,400,455,680 bytes installed
  memory; ordinary desktop background work, no affinity or machine isolation. Local NTFS temporary
  storage; no simulated slow disk.
- Rust 1.95.0, x86_64-pc-windows-msvc, Cargo release profile, shared target,
  `CARGO_BUILD_JOBS=1`, one measurement process at a time.
- Ordinary data-only Editor Open, built-in authoring schema, 64 / 1,024 / 4,096 shallow independent
  root entities with stable IDs and no components, images or Prefabs. These are scale probes, not
  representative rendered games or one Entity per map tile.
- Schema generation and fixture serialization are outside timing. Each population has two warm-up
  cycles and sixteen retained samples per process. The same source bytes are restored outside
  timing. Warm filesystem cache is expected; first-touch observations are not a cold-cache test.
- Capability acquisition and each public Open advance are measured separately. SessionPublication
  includes actual session/runtime construction and the first frame projection.
- Source indexing drains before persistence measurements. Idle frame drive is reported separately;
  independent percentile values are never subtracted to manufacture a pure I/O duration.
- Save and Reopen action dispatch are timed separately from the following drive. Save first adds
  `saved-edit`; Reopen discards another dirty `unsaved-edit`. Result identities, revisions, bytes
  and retained entities are checked outside timing. Each session physically closes before the next
  sample starts.
- Raw numeric samples are retained without paths or source text. Per-process P50, nearest-rank P95
  and max stay separate. With sixteen samples, P95 equals max; this is exploratory evidence rather
  than a statistically stable tail estimate or an acceptance assertion.

Run explicitly:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --cargo-profile release --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked --run-ignored only -E 'test(measure_editor_owner_thread_work)' --no-capture
```

## Optimized Results And Decision

All three processes passed the strengthened public persistence and physical-close oracles. Each
retains 480 rows: sixteen samples across three populations and ten phases. Debug and the earlier
optimized exploratory run are excluded. Final measurement source/data are `b93944e`, following
task behavior `3b70e48`; subsequent closure changes are documentation-only.

Figures are P50 / P95 milliseconds, separately by process. P95 equals max for these sixteen-sample
rows; the table does not establish stable tail latency. Original source sizes are 7,758 / 121,038 /
483,534 bytes for 64 / 1,024 / 4,096 entities. The saved source contains one additional entity.

| Population / operation | Run 1 P50 / P95 | Run 2 P50 / P95 | Run 3 P50 / P95 |
| --- | --- | --- | --- |
| 64 / Save drive | 3.319 / 3.600 | 3.014 / 3.431 | 3.229 / 3.872 |
| 1,024 / Save drive | 5.779 / 6.208 | 5.865 / 8.031 | 5.883 / 6.758 |
| 4,096 / Save request | 0.238 / 0.293 | 0.225 / 0.262 | 0.243 / 0.263 |
| 4,096 / Save drive | 14.973 / 16.357 | 14.926 / 15.559 | 15.117 / 15.792 |
| 4,096 / Reopen drive | 9.706 / 10.394 | 9.729 / 10.104 | 9.648 / 10.309 |
| 4,096 / Open startup closure | 9.920 / 11.194 | 10.048 / 10.312 | 9.911 / 12.028 |
| 4,096 / Open schema bundle | 3.097 / 3.289 | 3.138 / 3.370 | 3.074 / 3.211 |
| 4,096 / Open session publication | 5.851 / 6.627 | 5.663 / 6.478 | 5.743 / 7.564 |
| 4,096 / Idle drive | 0.288 / 0.311 | 0.287 / 0.330 | 0.291 / 0.307 |

The [complete per-process summary](data/2026-10-20-editor-responsiveness/summary.json) retains all
phases, including capability acquisition, manifest/profile and Reopen admission. Raw samples:
[run 1](data/2026-10-20-editor-responsiveness/run-1.jsonl.gz),
[run 2](data/2026-10-20-editor-responsiveness/run-2.jsonl.gz),
[run 3](data/2026-10-20-editor-responsiveness/run-3.jsonl.gz).
Nextest run identities are `afe87975-9498-4a46-a028-3eb96e5b3ddd`,
`9e4c8732-fa41-4778-a624-ca800ff7c5f0` and `8f24df9e-0f70-40db-8d2d-b4342cb8c410`.

Choose **8 ms per owner-thread preparation/drive call as a local investigation trigger**, leaving
approximately half of a 60 Hz frame for other work. This is an explicit design target for the next
experiment, not a product SLA, timing test assertion or a supported scene ceiling. All three
4,096-entity Save, Reopen and startup-closure medians exceed it even without components or rendered
assets. Their independent idle baseline is small, but subtracting its percentiles would not locate
the remaining costs. These rows justify focused phase attribution and owned preparation design;
they do not establish disk vs validation vs encoding as the dominant cause.

### Next Save/Reopen Design Slice

Start with Save's current caller and owner, since its measured drive is largest and it has a real
linear checkpoint. Keep the ordinary Save action and result projection. The internal operation
should own the captured document/revision/source binding and the physical file-publication work;
only its authoritative Workspace acknowledgement belongs to the session owner. Measure encode,
authoring revalidation and filesystem sync/replace separately before choosing which work moves.

The result must preserve an older captured save while later edits remain dirty. Reopen must check
its original base revision before replacing a document. A late operation cannot acknowledge a
replacement session, source binding or intent. Closing must retain the operation until its worker,
temporary file and publication outcome retire. Once file replacement may have happened, Cancel
cannot report an unqualified no-op. Preserve the existing uncertain-publication result and cleanup
receipt. Prove actual Send/ownership requirements for the specific inputs; do not move a World,
Workspace, frame borrow or the complete session across a thread boundary.

Implementation is a separate slice with rejected/stale completion, post-replacement cancellation,
close/retry and actual persisted bytes as acceptance. Reuse existing tasks/owned attempts where
they fit; no public generic operation service or Future executor is admitted by this measurement.
The analogous Open candidate can follow after its full closure/publication costs are attributed.

## Mature Engine Comparison And Disproof

Local Godot `81ab961f` separates threaded ResourceLoader request/status/result, but retrieving a
still-running load may wait. Its ResourceSaver and Editor scene saving remain synchronous. The
relevant lesson is explicit owned preparation and result publication, not making every owner trait
asynchronous. Its background Editor filesystem scan merges results on the main thread.

Local Bevy `85503941` dispatches owned asset handles/server/path into IoTaskPool and returns internal
events. Its task API accepts Futures and has different handle cancellation/detach semantics.
Neither kernel is a compatible substitute for Nara's existing retained runtime-close obligations.

A suspected unbounded supervised-process output drain was disproved by reading both platform
adapters. Windows and Linux already stop each poll after 64 records, retain reader/queue completion
separately, and expose overflow/loss. New output and reader completion wake the owner; retained
running output also has a maintenance deadline. No duplicate queue budget or production change is
needed. A bounded queue alone would not have proved this; the platform consumption budget does.

## Follow-Up Order

1. Completed: correct standalone cancellation and document the existing task contract, with the
   failed-then-passing regression and unchanged handle Drop semantics.
2. Completed within this protocol: three optimized public Open/Save/Reopen processes. Use the
   local 8 ms trigger to prioritize Save/Reopen phase attribution and the owned Save slice above.
3. Measure actual Browser Open and saved-source receipt, then representative components/Prefab
   closures, slow storage and native redraw interruption. Measure no-op Cargo stages and generation
   cleanup separately from compiler wall time. Do not infer those unmeasured costs from these rows.
4. If a measured caller exceeds its chosen budget, move only transportable preparation into an
   owned private attempt. Require stale-result rejection, owner-thread publication, explicit
   cancellation and physical retirement, including uncertain file publication.
5. Extract the touched responsibility when implementing that consumer: Tasks' private reaper,
   Editor build-admission helpers or generation retirement. Avoid moving a whole state machine
   solely because its file is long, and avoid introducing new public crates or interfaces.

Open/Save owner-thread work, source receipt admission and completed-result disposal remain
different problems. Plugin build/finish, component codecs, World commit and panel painting keep
their synchronous contracts. This investigation does not reopen U1-U15 or N05 or create a Goal.

## Private Module Candidates

Current counts include test code and helpers. They identify navigation cost; they are not a quality
score, production-line estimate or an independent reason to move code.

| File | Lines | Seam to extract when the corresponding responsibility changes |
| --- | ---: | --- |
| `crates/nara_tasks/src/runtime.rs` | 3,536 | Abandoned-owner coordinator and pending-capture disposal in a private runtime submodule, before changing its physical-disposal policy or adding an execution model. |
| `src/project_host/runtime/editor.rs` | 3,221 | Keep its existing submodules; new Open preparation belongs with the Open attempt, not another session owner. |
| `src/project_host/persistence.rs` | 1,649 | Separate owned file preparation from owner-thread acknowledgement only with a measured Save/Reopen consumer. |
| `editor/src/supervised_play/owner.rs` | 1,970 | Private build admission and generation retirement helpers; keep one authoritative lifecycle state machine. |
| `editor/src/supervised_play/build.rs` | 959 | Tool identity, metadata and artifact result admission; preserve one owned attempt and process cleanup. |
| `editor/src/supervised_play/handshake.rs` | 922 | Move its inline tests before substantial protocol expansion; do not invent another handshake abstraction. |
| `editor/src/supervised_play/process/windows.rs` | 1,914 | Native spawn/handle/reader/containment responsibilities, only when the corresponding native contract changes. |

The cancellation correction is local to abandonment and does not require moving the complete
reaper. A later completed-result disposal change should first make that existing private module
seam explicit and exercise its real consumers. Public facades should not expose the decomposition.

## Source Entry Points

- [Open phases](../../src/project_host/runtime/editor/open.rs)
- [Browser source admission](../../src/project_host/runtime/editor/document_open.rs)
- [Frame-driven persistence](../../src/project_host/runtime/editor/model.rs)
- [File publication](../../src/project_host/file_publication.rs)
- [Saved-source receipt](../../src/project_host/runtime/editor/external_play.rs)
- [Build attempt](../../editor/src/supervised_play/build.rs)
- [Generation cleanup](../../editor/src/supervised_play/generation/store/retention.rs)
- [Task runtime](../../crates/nara_tasks/src/runtime.rs)
- [Task cancellation/close tests](../../crates/nara_tasks/src/tests/close.rs)
