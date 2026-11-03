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
```

Set `CARGO_BUILD_JOBS=1` and do not run these concurrently. The second command deliberately enables
Bevy's transitive `multi_threaded` feature. Raw parallel comparisons use at most two ECS workers;
NonSend observations remain on the driver. This does not qualify production parallel support.

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
cross-platform determinism or production automatic parallel profile.

## Authored collision derivation

The optional `physics/authoring` feature supplies one plugin-specific persistent box definition
and a concrete Rapier owner. The definition stores world-space center, bounded half width/height
and fixed/dynamic intent. It contains no native handles or solver state. The consumer installs
the exact native Schema provider through ordinary App composition, then materializes Scene and
Prefab documents through public Scene interfaces before deriving native bodies and colliders.

Reconciliation receives the complete selected set of current Scene receipts. It validates every
receipt, source association and geometry before changing native membership. Generated body,
collider and numeric debug output retain typed source, occurrence, structured Prefab ancestry,
component/property selectors and the caller-accepted input revision. Debug vertices come from
actual collider shapes and poses; a debug capture alone does not certify a contact. The fixed
simulation test also observes a dynamic body resting on a floor and an actual solver contact.

Source edits, Undo/Redo and disk Save/Reopen use the ordinary exact native binding. Scene source
synchronization currently replaces that occurrence's activation; the fixture retires those old
native pairs and derives replacements. It synchronizes only dirty sources, preserving unrelated
occurrences and their solver state. Unchanged live entities keep native pairs, while an in-place
changed definition rebuilds its pair and resets dynamics. Retained immutable debug captures remain
readable across edit, removal and managed close without keeping the old solver objects alive.

Tests additionally cover duplicate definitions, two occurrences of one source, two structured
Prefab projections, component/entity removal, occurrence unload, invalid source/native values,
foreign/stale/duplicate receipts and missing-binding refusal at registry freeze with repair of
the original candidate. This is a translation-only public authoring/derivation workflow. It does
not qualify Editor widgets, a physics device, affine hierarchy integration, an official production
backend or a domain cook. The [owning unit](../../../docs/plans/2026-11-04-authored-collision-derivation-plan.md)
retains the comparison, evidence and limits.
