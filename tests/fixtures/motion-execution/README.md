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
captured-history and bounded prediction/effect gates before any production phase-interface freeze.
