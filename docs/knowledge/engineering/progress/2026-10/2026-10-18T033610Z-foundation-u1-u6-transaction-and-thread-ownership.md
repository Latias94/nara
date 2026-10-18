---
type: Work Progress
title: Foundation U1 transaction boundary and U6 owner-thread repair
description: Reproduced and repaired intermediate native validation and executable-owner mobility; serial owning-package and consumer evidence.
timestamp: 2026-10-18T03:36:10Z
record_id: foundation-u1-u6-20261018-033610
producer_id: codex-root
run_id: foundation-contract-refactor-20261018
git_branch: refactor/foundation-contract-hardening
related_plan: docs/plans/2026-10-18-1105-refactor-foundation-contracts-plan.md
---

# Completed Boundaries

U1 is `10efbdf`: per-operation path, wire shape, schema owner/version, capabilities, topology and
validation budgets remain enforced. Native/business validation runs on the final canonical candidate.
Scene and Prefab use the same boundary; public publication remains scratch-based and failure-atomic.
The private owned helper operates only on disposable unpublished candidates.

U6 is `722e3ee`: a single `PhantomData<Rc<()>>` in `RuntimeOwnerState` makes every configured,
candidate, failure, executing and retiring World owner non-Send/non-Sync. Putting the marker on
the state also covers `take_state`, retention guards and thread-local quarantine. World-free
definitions, plans, reservations, reporters and control metadata stay transportable. Independent
threads construct fresh Apps locally. No runtime ThreadId guard or unsafe Send exception is needed.
ADRs 0026 and 0084 explicitly describe these contracts.

# Verification

- U1 red: both coupled-field Scene/Prefab cases rejected operation 0 even when the final pair was valid.
- U1 green `319662ab-c409-4b28-ab4f-9a32b8172113`: 3 tests cover either operation order, invalid-final
  atomicity, history/Undo, private owned application and intermediate invalid shape rejection.
- U6 red: eleven executable owner/failure negative auto-trait assertions failed before the marker.
- U6 green `c1a0572d-ee1f-4df6-9f4b-1f560fd77dd4`: 3 tests cover transportable metadata,
  independent NonSend World destruction and owner-thread quarantine draining.
- Package run `4eb263a3-5fba-4eb1-be23-65f4a513ca70`: 269 tests passed across `nara_app` and
  `nara_scene`, with `nara_scene/serde`. An initial featureless full-suite invocation could not
  compile existing serde-only integration targets; no unrelated feature wiring was changed.
- Root run `f0100cd7-16b0-48f3-b009-34765a8383d0`: 107 tests passed across
  `runtime_driver_boundary`, `runtime_instance`, `scene_patch_transactions` and
  `scene_authoring_session`, including existing exact cumulative-budget boundaries.
- Reference-game `792a73b8-f677-4166-bcb0-a94a6a089099`: 21 first_wave tests passed,
  including stable-asset Retry and old-identity retirement.
- All Cargo commands were serial, with `CARGO_BUILD_JOBS=1` and existing root target. Windows,
  Rust 1.99.0. `cargo fmt --all` and `git diff --check` passed before these commits.
- Independent code/contract review found no blocking issue. No native audio SDK, full native
  Editor qualification or MSRV qualification is claimed.

# Continuing Work

The goal remains active; U1/U6 completion does not close the recursive schema, effective-reference,
durable identity, input or font units. Read the reviewed plan for dependencies and completion rules.
U2 research is in ignored `target/foundation-refactor-plan-research/u2-design.md`: one recursive shape
authority, concrete bounded visitor paths, root stable field IDs, trait-driven finite field codecs,
and an explicit catalog plus authoring-bundle v2 reset. U3 still owns effective defaults and target
validity. A present-value visitor migration is required in U2 for its real remap/export consumer.

U5 must stamp runtime references and mutation tokens with incarnation, rather than simply allow UUID
rebinding. Otherwise old commands or a token for the same surviving ECS Entity can affect a new
binding. Preserve existing scene-instance lifetime claims and restore behavior; persistent logical
records get a separate bounded ledger. Repeated new scene instances still have their existing
history budget; this unit must not claim to remove it.

U14 starts with a real second-font fixture and an existing compiler-to-planner red case. FontFace
already owns immutable validated bytes and content identity; reuse it rather than create a FontSet.
Its current dirty test/fixture is intentional next-unit work and is excluded from U1/U6 commits.

The external HTML stays in Downloads; its four fixed findings and 119 local/in-page links were
checked after U1/U6. Browser visual qualification remains unavailable. Pre-existing untracked
`myeasylog.log` and `vendor/` remain untouched. No remote publication is authorized.
