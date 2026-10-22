---
type: "Verification Evidence"
title: "U11 low-level refactor public and native consumer integration"
description: "Serial qualification of the real public games, independent modules and affected Windows Editor trajectories."
timestamp: 2026-10-22T19:37:24Z
record_id: "ddb983fa80454657955de17ff65d1772"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "52d91db"
---

# Verification

Windows 11, ordinary owner/consumer tests on Rust 1.99.0. Native Editor uses the procedure's
Rust 1.95.0 and DX12. Cargo is serial, one job, with the root target reused explicitly for
independent manifests. Core source is committed in `89fdf6f` (U4), `fb6af18` (U5), `cfde813`
(U10) and their earlier owner commits. `52d91db` contains equivalent helper-signature simplification
and formatting; later dirty scope is evidence/documentation. Preexisting untracked `vendor/` is untouched.

# Result

R1-R13 have committed owner corrections and executed owner/consumer evidence. Independent core
and public API rereviews have no remaining concrete blocker. Final product evidence includes
all five Windows/DX12 Editor trajectories, complete Reference headless/desktop consumers and an
actual compiled custom-project Check/Run/Repair/Export. Broader ADR/product/platform limits remain
explicit; the refactor does not claim a warning-free workspace or untested native platforms.

# Evidence

- Reference headless first-wave/authoring/runtime-core: 32/32 passes in
  `c4cdd0f2-8282-47db-88ca-a1b940ae2b52` before the final U4 cost simplification. Includes
  historical pursuit, non-spatial grouping and newborn projectile consume/retire/Capture behavior.
- Final full headless Reference Game: `cargo nextest run --locked --manifest-path
  reference-game/Cargo.toml --test-threads=1 --no-fail-fast`: 87/87 passes across 30 binaries in
  `b1b7245e-ce3d-4fba-a7ac-e90652323d22`. Covers complete first-wave/Retry, source authoring,
  real Image task last-good recovery, managed runtime drive/retirement, UI input/capture and
  strict project/schema/content workflows after final U4/U5 fixes. Feature-specific existing
  root unused/dead-code warnings and Reference test/helper warnings remain.
- Reference desktop render: `cargo nextest run --locked --manifest-path reference-game/Cargo.toml
  --features desktop --test desktop_render --test-threads=1 --no-fail-fast`, Windows/DX12:
  6/6 passes in `974c1cca-b192-4287-b3f1-6db3980dfac8`. The real product child prepares and
  submits the committed texture; atlas-first-frame, HUD/terminal geometry, preserved Sprite fields,
  same-tick parented weapon and enemy/snapshot world-space consumers pass.
- Root persistence/Scene/Browser/Open library selection: 32/32 passes, 204 filtered (run ID not
  retained; no ID is inferred). Root Check/content/Workspace Play: 43/43 passes in
  `90026916-6653-44ba-8dfd-4fc941f0c303` before final U5 review repairs.
- Final complete root library with `--no-default-features --features
  tooling,presentation-2d,presentation-ui,serde,project-workflow --lib --test-threads=1
  --no-fail-fast`: 267/267 passes, one existing ignored, in
  `be76a879-ce02-4919-b7ae-35f50ec892c2` (88.689 s). Includes Image metadata/TileSet refresh,
  Scene/Browser/Open slot return, Save/Reopen/failed publication custody, session Drop, content
  residency/travel and full UI presentation state. Ten feature/test-specific existing warnings remain.
- Final content boot/boundary/limits/delivery/selected Scene, public Check and Workspace Play:
  78/78 passes across seven binaries in `6d2d9e24-16ab-4939-aee7-e34dd66b3e7f`. Root features
  match the complete library cohort. Includes read-only/private-owner compile-fail trybuild
  (111.805 s), aggregate budget refund, exact typed recipe, stable references, startup repair,
  parented paused edits and retirement/retry. This does not substitute for actual executable Export.
- Final project-tooling Export owner with the same presentation/tooling features plus
  `project-tooling`: 5/5 selected tests pass, 305 unrelated filtered, in
  `c5831413-17e6-4fdc-b282-a873f7483bc5`. Exact external domain members publish and reopen;
  changed/missing auxiliary sources, wrong schema/startup agreement and changed nonstartup
  delivery reject before staging or publication. Build agreement in this owner suite is synthetic.
- Actual compiled custom-project trajectory: `cargo nextest run --locked -p nara
  --no-default-features --features project-workflow-cli --test project_workflow_cli
  --run-ignored only -E
  'test(fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay)'
  --test-threads=1`: 1/1 passes, seven unrelated skipped, in
  `9b2ff96d-0d5f-44bf-a84d-80f70bfe3997` (503.523 s). The generated independent product builds
  its actual external source domain, Check precedes gameplay, headless Run and startup Repair
  succeed, real locked release headless/desktop Export accepts exact product observations, and
  the detached exported executable consumes the copied dependency closure. Removing an auxiliary
  dependency then rejects before gameplay. This is execution evidence, not a build-speed claim.
- Two-level actual Save owner: `cargo nextest run --locked --manifest-path
  samples/two-level-game/Cargo.toml --lib -E 'test(save::io::)' --test-threads=1`: 7/7 passes,
  one unrelated skipped, run `f222687b-fd93-4b4f-a151-5c1e424a1072`. Covers contention,
  candidate failure, validated readback, legacy-file migration and independent reopen.
- Collect Game: `cargo nextest run --locked --manifest-path samples/collect-game/Cargo.toml
  --test gameplay --test-threads=1`: 4/4 passes in
  `e0ef2025-d93e-4d83-a9c5-e48f0ae06ea8`. Movement collects authored targets, release stops,
  and reset preserves authored entities through the migrated spatial reader.
- Complete independent module workspace: `cargo nextest run --locked --manifest-path
  tests/fixtures/module-consumer/Cargo.toml --workspace --test-threads=1 --no-fail-fast`:
  77/77 passes, two existing skipped, run `0cd32434-1ad3-4c7c-bae7-4a997d585eb9`. Separate
  Scene/FS/asset-domain/Editor-panel consumers exercise Check/Play, real import/reimport, physical
  task retirement, native reconciliation, manual typed codec, durable identity, Save and reopen.
- Editor generation-store owner on Rust 1.95.0: 15/15 passes in
  `cbb91eca-37be-48da-a6b6-8b4392c82b6b`, selecting
  `test(supervised_play::generation::store::tests::)`. Includes retained active/last-good stores,
  scavenge lock contention, identity-preserving launch, exact replacement refusal and physical close.
- U4 final owner/integration 191/191 and release timing/allocation results are recorded in
  [U4 evidence](2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md).
- Windows/DX12, Rust 1.95.0 explicit five-test native group: 5/5 passes in
  `b0cb0546-ff0b-4e4f-8d96-aa713e37fab8`, 195 unrelated skipped. Ordinary Scene/Prefab Save/Reopen,
  paused resume cadence, Game input/refocus/resize/Step and view detach/reopen/recovery each execute
  the hidden native path. Supervised Play executes real Cargo builds, last-good retention,
  successor generations and physical child/runtime retirement (411.997 s; full group 429.392 s).
  No OS pointer or keyboard input is injected. This run's primary Editor binary predates the final
  slot-identity and same-frame coalescing repairs; affected native scope is rerun after final source.
- Final-source repetition of the same explicit five-test Windows/DX12 group on Rust 1.95.0:
  5/5 passes in `7552d87a-bd13-4c48-bed0-83e103048dcb`, 195 unrelated skipped; total 76.896 s,
  supervised Play 59.820 s with warm owned target. This binary includes shared checked spatial
  publication identity and the final metadata/removal coalescing repair. Rust formatting was
  applied before the final binary compiled. Exact source is the U4 working tree on `fb6af18`;
  later documentation-only closure does not require another native cycle.

# Follow-up

Final format/doc/lint checks pass within the scope below. No remaining runtime or product
acceptance is deferred by this record. Final-source cost is recorded in U4. Existing feature-specific
warnings remain; future domains and platforms keep their existing admission triggers.

# Lint Scope

Scoped all-feature/all-target Clippy for Transform, Hierarchy, Gameplay, Asset, Watcher, Image,
Tilemap, Render and SpriteRender passes `-D clippy::correctness -D clippy::suspicious --no-deps`.
Style warnings remain, including preexisting Asset hex chunks/default/iterator annotations, wide
ECS/system signatures, RenderTarget default, PNG chunks, TileSet/Watcher nested conditions and
test collection suggestions. The new Presented helper's redundant explicit lifetime and migrated
measurement `ok().expect()` are removed; this changes no temporal or runtime behavior. The fixed
helper uses the same elided signature. No lint allowance is added to silence these changes.

`cargo fmt --all` formats existing coordinated ECS/Identity/Gameplay additions and the
independent Reference/Collect/schedule manifests. Their format-only changes are
retained in `52d91db`; no user work is discarded.
Final `cargo fmt --all -- --check` and independent Reference/Collect/schedule fixture checks pass.
`cargo test --locked -p nara_transform --doc` executes and passes the Global-not-Component
compile-fail boundary (1/1). Final scoped Clippy repeats successfully after helper/harness cleanup;
the residual Gameplay lifetime warning belongs to its preexisting local-command helper, not the
new spatial helper. Current contract/guide/evidence local links, table widths, code fences and
benchmark JSONL parse checks pass. Engineering memory validation retains only existing legacy
metadata/path warnings; no new validation error is introduced.

# Requirement Coverage

| Requirement | Owner / implementation | Verification boundary |
| --- | --- | --- |
| R1 / L01 | U1 `d6dc05c`, unconditional App custody before fallible activation | Fresh App close/rejection/retry cohort; original failure retains exact owner |
| R2 / E02 / A04 | U2 `feb273d`, independent return observer and physical running accounting | Tasks 51-test cohort, disposal-focused regression and public Scene/Browser/Open preparation |
| R3 / L07 | U3 `b57008d`, unique lock owner with ownership-returning failures | Windows 71 and Linux WSL 77 FS tests, negative docs, root persistence, two-level Save7 and Editor generation15/native Save |
| R4/R5 / L03/L04/E04/A01/A02 | U4 private phase cuts, same-cut parent/order and unique publication | Spatial/render191, final combined329, real Reference pursuit/spawn/retire and final native5 |
| R6/R7/R8 / L05/L06/E03 | U5 `fb6af18`, desired intent, common baseline and runtime rescan custody | Asset/Image/Watch/Tilemap owners including combined329, independent module77 and real Image task consumer |
| R9 / E01 | U6 `6ca126f`, typed prepared candidates/encoders with private erasure | Reflect196, typed negative docs2 and external manual codec plus shared batch AssetServer |
| R10 / L02/L09/E05/A03 | U7 `8d3515c`, facade derives; U8 `6ca126f`, World-owned identity borrowing | ECS22, renamed fixtures, Identity/Scene311 and external durable identity workflow |
| R11 / L08/L10 | U9 `b44e4b2`, pre-copy key admission and finite interpolation | Allocation-observing serde16 and numeric endpoint/shortest-arc tests plus spatial interpolation |
| R12 / E06 | U10 `cfde813`, exact-batch successful execution | Failure/deferred/reporter replacement trajectories, external schedule9 and real Reference failure capture |
| R13 | U11 actual consumers, old API removal, amended ADRs and explicit platform/cost limits | Final public/native integration, no-blocker independent API migration review and precise commits |

ADRs with broader unimplemented domains remain partial. This refactor selects no new physics/audio,
post-affine/3D, save wire format, dynamic ABI, general resource acquisition or rollback service.
O1-O6 remain at their existing measured-consumer triggers; no compatibility layer is kept to avoid
migration. Windows/DX12 native acceptance and Linux FS mechanism evidence do not imply native
Linux/macOS Editor, monitor scanout, manual input-method delivery or remote filesystem guarantees.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [U3 native FS qualification](2026-10-22T192527Z-u3-owned-file-lock-capabilities-on-windows-and-linux-3ffaeab4142942bd9e4761ce4cf19af4.md)
- [U10 execution](2026-10-22T193136Z-u10-exact-gameplay-batch-success-and-reporter-identity-380c336814b2450abe14023d43cf6253.md)
