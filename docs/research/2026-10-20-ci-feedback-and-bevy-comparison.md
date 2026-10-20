# CI Feedback Cost And Bevy Comparison

Status: Source-grounded research and a first bounded correction; hosted before/after qualification
is pending. This does not reduce the current platform or feature acceptance scope.

## Observed Nara Cost

Hosted run [39418300517](https://github.com/Latias94/nara/actions/runs/39418300517), source
`49423aec546f68f55474aac8a820358e859686b3`, Windows root job `118612909108`:

| Stage | Actual elapsed |
| --- | ---: |
| Root workspace check | 4m34s |
| Feature matrix, examples and all-feature check | 15m26s |
| Nextest build before the first test starts | About 15m50s |
| Default workspace test cohort | 35m15.759s |
| Whole job, ending at the failed default cohort | About 71 minutes |

The default cohort executes 2,659 tests: 2,650 pass and nine fail, with 42 platform/ignored skips.
The all-feature cohort and Dear adapter gate do not run. Their elapsed time and duplicate work
cannot be inferred as measured results from this run. The former 75-minute job envelope cannot
cover the existing complete cold sequence; its temporary 180-minute replacement is a qualification
guard, not a satisfactory feedback target.

Eight selected compiler/consumer tests consume 1,614.084 seconds, 76.29% of the default test
elapsed time. These are not the globally slowest eight: the separate generated-starter Play test
also takes 262.394 seconds.

| Test | Seconds | Actual work |
| --- | ---: | --- |
| Scene public surface | 648.590 | Mixed trybuild positive/negative cases force full dependency code generation. |
| Persistence authority | 384.482 | Negative-only trybuild checks initially construct their separate target/flags graph. |
| Reflection derive | 159.551 | Nineteen negatives and two positives; one positive actually registers and freezes a registry. |
| Public prelude | 133.604 | Twenty-six independently checked bins with a fixed broad feature graph and a private target directory. |
| Runtime runner | 107.754 | A renamed external consumer executes a real product recipe under fixed runtime-2d/serde features. |
| Schedule extension | 70.086 | A renamed consumer executes scheduling, error and lifecycle assertions under fixed runtime-core features. |
| Derive dependency rename | 56.147 | Two actual manifests prove macro resolution through renamed dependencies. |
| Missing module prerequisite | 53.870 | Removing a real direct dependency must produce the expected import failure; another private target directory. |

Root and module-consumer CI reuse the root target within their respective jobs. The reference
game has its own target. There is no cross-runner build cache. A prior `cargo check` produces
metadata rather than the linked test objects, so the subsequent test build is not all redundant.

## Bevy Source Comparison

Reference: local read-only `repo-ref/bevy` at `85503941e488152711a4695bbe248e9ed9f453fc`.
The source describes that revision, not an unverified current online configuration.

| Responsibility | Bevy evidence | Appropriate Nara adaptation |
| --- | --- | --- |
| Job ownership | `.github/workflows/ci.yml:27`, `:58`, `:129`, `:253` separate platform tests, Linux lint, compile tests and Wasm. | Separate static checks from costly execution when measurements justify it. Keep Windows/Linux acceptance. |
| CI build settings | `ci.yml:17-20` disables dev/test debug information; cache preparation uses the same settings. | Evaluate consistent CI-only debug settings, keeping debug assertions and individual deadlines. Do not silently change local development or exact external-launch inputs. |
| Cache authority | `ci.yml:37-50` restores; `update-caches.yml:6-14,128-185` writes from main/daily/manual owners. | Restore across runs with OS, toolchain, target and profile boundaries; let trusted main write. No shared mutable cache key across unrelated jobs. |
| Feature coverage | `tools/ci/src/commands/test.rs:26` runs workspace tests with track_location; `clippy.rs:17` uses all-features. | Bevy does not execute every feature combination as a full runtime suite. Preserve Nara's existing distinct combinations until their actual obligations are mapped. |
| Compile diagnostics | `tools/compile_fail_utils/src/lib.rs:49-54` uses ui_test annotations rather than requiring a complete stderr candidate list. | Use precise negative trait assertions when they express the contract. Retain trybuild for source/privacy diagnostics that need it; no framework migration is required. |
| Platform journeys | `example-run.yml` explicitly selects Metal/Vulkan/DX12 and publishes diagnostic artifacts; platform/event policies differ. | Keep Nara's newly repaired Linux Play and Windows pixel regression in current acceptance. Learn explicit environment and diagnostics ownership, without copying Bevy's event exclusions. |

Bevy's compile-test documentation contains a source discrepancy: root `Cargo.toml:24-26` includes
compile-fail crates in the workspace and the test command uses `--workspace --tests`, while
`commands/compile_fail.rs:22-35` still describes them as outside ordinary workspace tests. This is
not evidence that its ordinary and dedicated compiler commands are completely deduplicated.

## First Correction

Pinned trybuild 1.0.118 selects `build` instead of `check` when any positive case exists
(`src/cargo.rs:97,130,153`) and only batches negative checks when no positive exists
(`src/run.rs:75`). This is a concrete source mechanism behind the Scene cohort's extra work.

Move the Scene and Startup positive imports/type proofs into ordinary tests in their original
integration targets. Preserve their runtime-core gates, so both existing outer feature combinations
continue compiling and running those positive proofs. Keep all five Scene and seven Startup
negative fixtures and their diagnostics. Delete the two obsolete positive fixture files.

The positives only reference public types/functions; they do not need a second Cargo-built
executable. This adjustment uses the current test framework and preserves the external-crate
boundary of Rust integration tests. Local warm execution can prove correctness, but only successor
hosted data can quantify a cold feedback improvement.

Implemented at `29e8262`. All four tests pass under default and all-features, with all twelve
negative cases retained. Independent review confirms equivalent public-boundary coverage. These
warm runs take 13.500 and 18.531 seconds; comparing them directly with cold hosted time would be
invalid. Cache/profile and job-layout recommendations below have not been implemented by this slice.

## Reflection And Ordinary Consumer Follow-up

The next selected correction moves the two reflection positives to ordinary integration tests,
preserving explicit crate override, nested aliases and actual registration/freeze. Merely removing
their trybuild `pass` entries is insufficient: a real check-only run unexpectedly accepts two
non-finite initial-value cases and changes a third diagnostic to a literal-overflow lint. The
existing float proof is an inline const inside schema construction, so it depends on a later
evaluation/code-generation phase. Restoring a positive case or weakening snapshots would conceal
this owning declaration-validation gap.

The correction emits an unnamed constant for each floating initial value or numeric range bound.
An unselected `Some(component)` field access in a match on `Option::<&Component>::None` lets Rust
infer the literal's actual field type. The selected branch contains only the literal; it neither
constructs nor reads a component. This retains aliases and Self-associated field types without
re-parsing Rust names or adding a helper trait that could shadow user types. The schema path still
owns typed assignment, encoding, numeric interval construction and registration/freeze validation.

The [Rust Reference](https://doc.rust-lang.org/stable/reference/items/constant-items.html#evaluation)
specifies compile-time evaluation of free constants, including unused ones. A Rust 1.95.0
metadata-only probe accepts a wide alias and a Self projection, while rejecting an overflowing
actual f32 even when overflowing-literal lints are allowed. Actual derive tests additionally
cover initial/range rejection without schema calls, integer-shaped float suffixes, and successful
wide-alias/associated-type registry construction. All original nineteen negative sources remain;
three new cases strengthen check-only coverage. Only inspected diagnostics are updated to the
new literal-local E0080 span and the retained wrong-suffix type error.

Ordinary public-prelude and missing-prerequisite consumers now select external `CARGO_TARGET_DIR`
or the repository target instead of separate artifact directories. Their independent manifests,
feature graphs, command flags and exact error assertions remain. This is artifact reuse, not
cached success/failure. Existing trybuild target/flags remain untouched. These source changes
still require separate hosted timing observations before a cold speedup claim.

The reflection correction is committed at `23496a3`; both focused default/all-feature cohorts
pass 21/21, each including all 22 negative fixtures. Ordinary target reuse is committed at
`dc2d855`; both consumers pass with the repository fallback and with relative
`CARGO_TARGET_DIR=target`. These are local correctness results. Hosted run `39437200611` uses
the earlier `e856a5f` source and cannot qualify either follow-up or their timing.

## Correction Order And Remaining Work

1. Observe the first correction in complete hosted execution before claiming a speedup.
2. Reflection organization and declaration validation are locally qualified at `23496a3`, with
   real register/freeze execution and all nineteen original plus three new negative cases.
3. Ordinary external fixture target reuse is locally qualified at `dc2d855`. Independent
   manifests/locks and expected failures remain; trybuild's private graph is unchanged.
4. Add one maintainable cache configuration with consistent CI profile inputs. Measure restoration,
   cold misses, upload cost and total hot-run time. Cached dependencies do not eliminate real
   compile-contract and native workflow execution.
5. Separate check and execution owners if their critical path still warrants it. Map each command
   to its retained coverage before moving or deduplicating it. Default/all-feature runtime and
   feature-inheriting trybuild cases remain distinct obligations. Fixed-manifest nested consumers
   may later run once in an explicitly owned compiler cohort.

Do not introduce a general CI command framework, dozens of unmeasured shards, changed runtime
test semantics or a new compiler diagnostic parser. Cache hits must not become correctness
evidence. Report actual cold and warm measurements separately, including unsuccessful runs.

### Small Cache Candidate

The next candidate keeps the current native jobs and profiles. Use pinned `actions/cache/restore`
and `save` steps, with a distinct root/reference/module namespace per OS, architecture, Rust
version and profile policy. Hash checked-in manifests, locks and workflow inputs; preserve those
same boundaries in any restore prefix. PRs may restore; only successful main push jobs save.
All existing commands run regardless of cache hits. A stable dependency-input key avoids uploading
another full native cache on every source commit, but an exact hit is immutable and will not
refresh later workspace artifacts. This is an explicit limitation, not a correctness shortcut.

Candidate paths are Cargo registry/index/source and Git database plus the actual build paths:
root `target/debug` and trybuild output, reference `reference-game/target/debug`, module consumers'
already shared `target/debug`. Do not cache complete Cargo home, binaries/credentials or the
whole target tree with temporary projects, measurements and process state. Do not cache generation
stores. First measure each candidate's size and actual restore/upload cost against the repository
quota; six large native caches can evict one another. Supervised Play's separate build target also
needs attribution before it is included. Keep this candidate disabled until that bounded ownership
and size check; then compare miss/seed/restore runs without a new prewarming workflow or sharding.
The [pinned action strategy](https://raw.githubusercontent.com/actions/cache/55cc8345863c7cc4c66a329aec7e433d2d1c52a9/caching-strategies.md)
and [GitHub cache reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
describe immutable keys, restore prefixes and branch scope.

## Owning Sources

- [CI repair and feedback follow-up](../plans/2026-10-20-ci-portability-repair-plan.md)
- [Nara workflow](../../.github/workflows/ci.yml)
- [Scene positive and negative contracts](../../tests/scene_product_public_surface.rs)
- [Startup positive and negative contracts](../../tests/startup_scene_public_surface.rs)
- [Local Bevy workflow](../../repo-ref/bevy/.github/workflows/ci.yml)
- [Local Bevy cache owner](../../repo-ref/bevy/.github/workflows/update-caches.yml)
- [Local Bevy compiler utility](../../repo-ref/bevy/tools/compile_fail_utils/src/lib.rs)
