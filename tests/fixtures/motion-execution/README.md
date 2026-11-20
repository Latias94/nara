# Joint motion execution consumer

This independent Cargo workspace compares three finite compositions with the same separately
authored animation and physics crates:

- public Bevy sets in one fixed schedule;
- a fixture-only sealed three-segment plan;
- `bevy_app::App::empty()` with one explicitly selected update schedule, without another runner.

The finite plan checks its closed inventory; it is not a general scheduler or an engine API.
The raw comparison's translation-only publication oracle deliberately has no production spatial
authority. The managed tests additionally use Nara's actual lifecycle, ordered hierarchy and
fixed/presented Transform snapshots, including zero/multiple ticks, Pause and exact Step.

`StandIn` establishes sequencing first. `Direct` and `Borrowed` run the same pinned Rapier 2D
0.33.0 simulation with actual contacts. Borrowed work uses a scoped OS thread and an explicit join
before pose commands. This is not Rapier-internal parallelism, pool reuse, a backend selection,
general rollback or a performance result.

Run serially from the repository root, reusing `target`:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --test-threads 1
cargo +1.95.0 nextest run --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --features unified-multi-threaded --test-threads 1
cargo +1.95.0 nextest run --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --features production-parallel --test-threads 1
```

Set `CARGO_BUILD_JOBS=1` and do not run these concurrently. The second command deliberately enables
Bevy's transitive `multi_threaded` feature. Raw parallel comparisons use at most two ECS workers;
NonSend observations remain on the driver. The third command additionally enables Nara's explicit
`parallel-executor` feature and the managed production-policy cases below. Each invocation retains
the original raw comparisons, managed Serial tests, prediction/PCM and authored collision tests.

The [owning unit](../../../docs/plans/2026-11-03-joint-motion-execution-plan.md) retains the shared
captured-history and bounded prediction/effect evidence before any production phase-interface
freeze. The related [render consumer](../render-policy-consumer/README.md) combines these motion
authors with actual captured two-View history execution on Windows/DX12.

## Fixture responsibilities

| Crate | Responsibility |
| --- | --- |
| `contract` | Shared actor handles, motion input, root-motion intent and sequencing observations. |
| `animation` | Independently authored root-motion intent production. |
| `physics` | Stand-in and real direct/borrowed Rapier solve, explicit join and pose commit. Its optional `checkpoint` feature captures persistent solver state for the bounded comparison. |
| `audio` | Offline mono PCM tones, sample-time admission, semantic duplicate suppression and managed owner shutdown. |
| `consumer` | Schedule alternatives, managed lifecycle/Transform integration and the closed prediction/effect tests. |

These are independent test consumers, not production Physics, Animation or Audio modules. No
production dependency imports them, and this fixture does not choose a production backend.

## Explicit managed execution policy

`configured()` continues to select `ExecutionProfile::Serial`, including when Bevy features unify.
The `production-parallel` feature adds public `App::set_execution_profile(Parallel)` compositions
and checks the selection through both App and the original managed Runtime. The Host initializes
or reuses Bevy's process-global `ComputeTaskPool` through `TaskPoolBuilder` with two workers before
sealing. Nara admits the selected executor against that existing pool before Startup; it does not
create, resize or take ownership of the pool. Pool/build refusal cases belong to the separate
public App policy tests, so their process-global initialization order cannot affect this fixture.

The new managed cases exercise the following finite workflows:

- Compare eight real Animation/Rapier ticks across Serial/Parallel and Direct/Borrowed solver
  combinations, including dynamic-floor contact, actual deferred root-motion intent and pose
  components, fixed spatial cuts and presented interpolation. Pose comparisons use a tolerance;
  they are not a bit-exact replay guarantee.
- Drive zero and multiple fixed ticks, Pause, exact Step and Resume. The exact third tick moves
  both actors to a different parent through `HierarchyWriter`, checks KeepLocal before the next
  motion commit, and verifies the new parent and actual spatial samples.
- Rendezvous two independent ECS systems on distinct Compute workers, with bounded waits, while
  the real Direct solver runs through the same managed composition and NonSend stays on the
  driver. These are ECS peers; they are separate from the nested scoped Borrowed solver thread.
- Hold a real Borrowed solver job with the existing finite worker gate. The managed drive, pose
  commit, callback and fixed publication cannot pass its explicit join. After release, committed
  components and actual fixed spatial samples must match the completed solver result.
- Consume and acknowledge a command, then fail a later fallible system after fixed publication.
  The original Runtime retains its sticky fault, narrow consumption Ack and partial deferred
  effect. A second Runtime on the same pool remains healthy. Each original owner retires once on
  its driver and closes only its own offline PCM owner, retaining its own final solver sample.
- Preserve an explicitly driven custom Serial schedule inside a Parallel managed Runtime,
  complementing the existing Serial-managed/custom-Parallel control. Automatic drives do not run
  either custom schedule.

These cases qualify only the existing Bevy scheduler integration for this translation-only
independent consumer. The owning plan records the executed Windows results. They do not parallelize
Rapier internally, qualify every plugin or opaque effect under parallel execution, extend the
closed prediction model to Parallel, or claim performance, cross-platform determinism or affine
hierarchy support. Runtime retirement
does not join or destroy the Host's process-global Compute pool. The captured two-View native
history consumer remains a separate qualification and is not rerun by the commands above.

## Bounded prediction and effects

The test-only prediction consumer runs at most 32 logical ticks in explicitly driven custom
schedules inside the same managed Runtime and World. Its clock is distinct from App's monotonic
FixedTime. A checkpoint restores motion inputs/intents/poses, logical entity membership/values,
seeded RNG and phase, pending commands, ordered asynchronous outcomes, cue intents and complete
persistent solver state. The authoritative checksum includes encoded solver bytes. Corrected
resimulation must match a clean corrected trajectory and continue matching on later ticks.

Attempts carry Runtime generation and a non-rewound ordinal. Same-tick replacements and other
Sessions cannot reuse their results or confirmation tokens. Confirmed state has an ancestor digest
witness; even a later checkpoint refuses if it would replace that prefix. Confirmation and the
audio duplicate ledger are not rolled back. Audio publication occurs only for the exact completed
current attempt, and a failed publication does not advance confirmation.

PCM rendering produces actual finite sine samples. Controls verify silence before confirmation,
nonzero scheduled output, duplicate suppression across attempts, atomic batch refusal, sample
chunking and managed shutdown. Uncovered Local, native I/O and configured worker-gate state refuse
before their execution. This closed authored inventory cannot detect arbitrary hidden effects in
trusted Rust systems. The fixture establishes no general rollback, physical audio-device support,
cross-platform determinism or prediction under a production Parallel profile. The explicit managed
policy cases above retain these tests without changing their original Serial/custom schedules.

## Authored collision derivation

The optional `physics/authoring` feature supplies one plugin-specific persistent local-box definition
and a concrete Rapier owner. The canonical v1 component is `fixture.motion.LocalBox`; its center
and bounded half extents describe geometry in the entity's Transform frame, alongside fixed/dynamic
intent. It replaces the unreleased world-center fixture without an alias or old-format reader.
It contains no native handles or solver state. The consumer installs
the exact native Schema provider through ordinary App composition, then materializes Scene and
Prefab documents through public Scene interfaces before deriving native bodies and colliders.

Fixed affine box construction reuses the directly consumed
[nara_rapier2d](../../../crates/nara_rapier2d/README.md) package. This fixture retains its original
coordinate/size bounds, diagnostics and native dynamic solver. The shared collider's world pose
becomes the fixed body pose, with an identity local collider pose. Enabling `authoring` selects
this dependency; the base motion plugin does not inherit it.

Reconciliation receives the complete selected set of current Scene receipts and reads the actual
current `FixedInputSpatialSnapshot2d` published by the original Transform owner. There is no
identity/local fallback. It validates every receipt, source association and native shape before
changing native membership. Generated body,
collider and numeric debug output retain typed source, occurrence, structured Prefab ancestry,
component/property selectors and the caller-accepted input revision. Debug vertices come from
actual collider shapes and poses; a debug capture alone does not certify a contact. The fixed
simulation test also observes a dynamic body resting on a floor and an actual solver contact.

Source edits, Undo/Redo and disk Save/Reopen use the ordinary exact native binding. Scene source
synchronization currently replaces that occurrence's activation; the fixture retires those old
native pairs and derives replacements. It synchronizes only dirty sources, preserving unrelated
occurrences and their solver state. A live fixed entity keeps its native pair, contacts and solver
state after an equivalent reparent or reflection. A changed definition or changed fixed geometry
rebuilds the affected pair. Retained immutable debug captures remain readable across edit, removal
and managed close without keeping the old solver objects alive.
[Fixed collision continuity](../../../docs/plans/2026-11-04-authored-collision-derivation-plan.md#fixed-collision-continuity)

Tests additionally cover duplicate definitions, two occurrences of one source, two structured
Prefab projections, component/entity removal, occurrence unload, invalid source/native values,
foreign/stale/duplicate receipts and missing-binding refusal at registry freeze with repair of
the original candidate. The transformed successor exercises fixed affine convex shapes and root,
unit-scale dynamic writeback, while the separate joint Solver retains its translation-only motion
contract. It does not qualify parented dynamic poses, local-affine authoring, Editor widgets,
an official production backend or a domain cook. The
[owning unit](../../../docs/plans/2026-11-04-authored-collision-derivation-plan.md#fixed-spatial-inputs-and-transformed-collision-successor)
records implementation status, executed results and limits; source changes alone are not
qualification.

Source synchronization materializes or unloads Scene data without stepping physics. The next
explicit fixed tick admits geometry through the original spatial input, solves, then writes root
poses back before the completed Fixed result. The test harness can run a named admission-only tick
to inspect native pair/velocity preservation before solving; its Runtime clock still advances.
Snapshots retain the original input stamp and whether a solve completed. Failed admission revokes
solve permission and current debug availability; retained old captures keep their original values.

The November 15 Windows/Rust1.95 release qualification passes the complete default cohort 40/40
and `production-parallel` cohort 46/46. Ten transformed-collision tests run each workflow under
Serial and Parallel when that feature is enabled. They retain actual Rapier contacts and
independent affine-corner oracles, same-tick deferred edits, zero/multiple/paused exact ticks,
source/history/Reopen, late live writeback refusal and original native cleanup. Separate actual
DX12 hardware history and software joint Material/history trajectories use the same fixed-input
role. These correctness results do not measure throughput or memory, qualify other platforms or
admit parented/scaled dynamic bodies.

The domain owns `COLLISION_SCHEMA_PROVIDER_ID` and `COLLISION_SCHEMA_PROVIDER`, so independent
compositions install the same original owner/provider/native-binding receipts. Game plugins
retain their own Scene selection, scheduling and retirement integration. The original domain
also captures each admitted tick's actual positive fixed duration and revalidates it before
setting the native solver timestep; a custom App step must not silently run Rapier's default
1/60-second integration. Changed or zero duration refuses before native effects and permits
explicit readmission after the original clock is restored.

The [ordinary visible consumer](../render-policy-consumer/README.md#authored-collision-and-ordinary-sprite)
joins the same owner to Sprite/stock capture, source editing and complete GPU pixel/lifetime
checks. It is separate from this workspace's CPU/native-solver qualification; use the owning
plan for actually executed source/profile/platform results.
