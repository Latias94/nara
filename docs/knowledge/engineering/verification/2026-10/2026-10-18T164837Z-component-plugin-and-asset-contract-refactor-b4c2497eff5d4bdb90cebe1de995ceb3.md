---
type: "Verification Evidence"
title: "Component plugin and asset contract refactor"
description: "Checked runtime identities and typed claims, schema closure, exact TileSet admission, and consumer migration."
timestamp: 2026-10-18T16:48:37Z
record_id: "b4c2497eff5d4bdb90cebe1de995ceb3"
producer_id: "codex-root"
run_id: "component-plugin-contracts-20261018"
related_plan: "docs/plans/2026-10-18-2345-refactor-component-plugin-contracts-plan.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Verification

Execution of U1-U5 from the component/plugin/asset contract plan, authorized by the user's
request to create a goal and start the fearless refactor. The tested source is the uncommitted
worktree on `refactor/foundation-contract-hardening`, based on
`0ebc6603ccafc41e89acd0d478658610f7cf19ee`. It includes the preceding file-backed TileSet
cold-start slice. Unrelated `myeasylog.log` and `vendor/` contents were preserved.

Cargo commands run serially with `CARGO_BUILD_JOBS=1` and the existing root `target`. The toolchain
is `rustc 1.99.0 (b940084d7 2026-09-28)` on Windows. Native development checks use DX12; they do
not establish the Rust 1.95 release qualification or another platform's behavior.

# Result

U1-U5 are complete within the plan's selected foundation scope. Owning regressions, public
consumers, the affected native development trajectories and final build/format checks pass.

- Runtime IDs come from one checked process allocator. Independent catalogs and divergent new
  allocations cannot alias; existing snapshot bindings remain stable. Claims participate in
  snapshot equality; unrelated allocations do not invalidate prepared work.
- Typed reservations and conversions validate catalog membership and the principal Rust type.
  Unchecked public ID construction and Handle retyping are removed. Detached render targets
  allocate fresh typed identities and retain them for their owning view/session, including resize.
- Image routing secures its registered type claim before generation or Loading mutations. Success,
  failure and removal validate the retained handle against the current catalog as well as the
  existing slot/state/generation guards.
- Direct App sealing compares the final installed schema-provider union with actual receipts
  before freezing or publishing schema authority. Delegated registration, multiple declarations
  selecting one provider, standalone registries and asset-free/staged codecs remain supported.
- File-backed TileSet discovery precedes Image decoding, exact extent validation and identity
  publication. A rejected dependency preserves earlier snapshots and retained budget accounting.

# Evidence

## Behavioral Runs

Selections overlap; these counts must not be added as a unique-test total.

| Selection | Result | Nextest run ID |
|---|---|---|
| Asset, Image, Reflect, Scene, Tilemap, Sprite, Render, Sprite Render, Material and UI Render owners; all features, libraries | 447 passed | `f140abc3-84e6-44ed-9c31-eb58f50c8e33` |
| Image owners after current-catalog terminal guards | 71 passed | `853a4de8-a999-41a6-800a-595e0099599a` |
| Root plugin/project composition, content/runtime boot and saved Sprite/Tilemap serialization; `presentation-2d,serde,tooling` | 77 passed | `52138238-e48f-48fa-accb-94231954b80a` |
| Content admission after exact floating expansion | 5 passed | `a0184d95-99ba-4ee6-94fb-de200fb6b19b` |
| Asset owners after early registered type claim | 76 passed | `b07df54e-449d-4d07-b51c-5e2cf0470a15` |
| Separately locked module-consumer workspace, including public durable-ref/typed-claim consumer | 24 passed | `4056603b-db79-465b-a93b-c60f7c457a22` |
| Final Asset and WGPU backend owners, including isolated allocator exhaustion and adapter-required Dear integration; all features | 184 passed; 2 ignored native cases skipped | `3e8c28c4-780b-441d-aeac-c9b1b2d57611` |
| Explicit ignored native Game input/resize, independent view lifecycle and secondary-window bridge selection | 3 passed; 252 outside selection skipped | `b9964ba4-1270-4ea9-9e06-c2e8c72b0abe` |
| Root preview consumers; all features, library filter `test(preview)` | 8 passed; 201 outside selection skipped | `a0c7fbe6-34a5-4acc-8c04-53b51d8e361c` |
| Sprite Render presentation interpolation integration | 5 passed | `c0521b02-f650-4252-a10e-a669a7e3cd74` |
| Separately locked reference-game `first_wave` consumer | 21 passed | `2734dc46-e05a-431e-820e-871efa56b29c` |
| Final separately locked manual component/public asset API consumer after isolated allocator fixture extraction | 2 passed | `b8e7cb98-ab18-4999-b291-8be3695b6edf` |

`cargo test --locked -p nara_asset --doc` passed both compile-fail cases: external code cannot
construct raw AssetIds or retype an existing identity through `Handle::new`.

Final static checks passed on the integrated implementation:

```text
cargo check --locked --workspace --all-targets --all-features
cargo check --locked -p nara --no-default-features --features runtime-core --lib
cargo check --locked -p nara --no-default-features --features runtime-2d --lib
cargo metadata --manifest-path samples/collect-game/Cargo.toml --locked --offline --format-version 1
cargo test --locked -p nara_asset --doc
cargo fmt --all -- --check
cargo fmt --manifest-path tests/fixtures/module-consumer/Cargo.toml --all -- --check
git diff --check
```

The first format check found one newly added assertion requiring multiline layout. Only that
whitespace was corrected before both final format checks passed. Existing deprecation and
dead-code warnings remain; no unrelated warning cleanup was included.

Engineering-memory rollups were rendered and bundle validation passed. Advisory warnings concern
historical records with legacy metadata or absolute paths; those unrelated records were preserved.
Modified documentation's local links were checked, and the standalone HTML report remains outside
`docs/` with explicit implemented scope and successor boundaries.

The native run selects
`native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`,
`native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts` and
`secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images` in `nara-editor` and
`nara_render_wgpu`, with `--all-features --lib --run-ignored only`, one test thread,
`WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`. All three actually executed.
These hidden-window development trajectories cover the changed identity/lifecycle consumers;
the broader critical qualification group and Rust 1.95 release procedure were not executed.

## Independent Review And Reproductions

Two independent reviewers covered asset/schema contracts and TileSet/render-target integration.
Their final reviews reported no actionable findings after these corrections:

- A catalog fork could preserve an ID/path/UUID while claiming another Rust type. The initial
  Image publication reproducer unexpectedly admitted that fork
  (`d505c8ab-6d84-4741-b51e-14183859fa63`); the pending terminal reproducer changed its state
  (`d3caa523-d0a3-44ff-a9a1-88003a4979e9`). Admission, admitted-candidate commit, removal and
  failure now reject the incompatible current catalog without changing its value/state revisions.
- An ordinary system between exported ResolveSourceChanges and SpawnJobs sets could claim a
  newly resolved untyped ID after routing had already published Loading. Registration now supplies
  two domain-owned callbacks to validate and acquire the type before lifecycle mutation. The core
  asset crate still has no Image dependency and requests remain identity-only.
- Both f32 atlas totals and a single f64 sum can round away invalid positive margins. The red
  content run (`b9fea73f-f59f-45d5-ba55-d0005509f362`) reached successor capacity instead of
  rejecting extent first. A fixed six-term error-free expansion preserves product residuals and
  exact sums; the resulting denominator must also agree with the actual f32 UV representation.
  It has constant cost, no allocation and no new dependency. Cases cover margin `1e-20`, tiny
  spacing, rounded large counts, valid fractional layouts, shared Images and successor rollback.
  A separate review oracle agreed on 6,459 deterministic rational-arithmetic cases; that model
  check is supplementary and is not a Rust/Cargo test result.
- A stale render reservation regression must reuse one image identity while changing extent.
  Its migrated fixture now does so, preserving the original stale-authority assertion.

The missing direct-App provider regression failed before sealing validation was added
(`4fd70588-ff15-45c2-ba5d-13bd7a5d3fc1`) and passes in the integrated composition selection.
The isolated allocator exhaustion test uses a private local atomic fixture, leaving the
process-global test allocator untouched.

## Integration Notes

The module-consumer lock initially lacked the existing TileSet `ron` dependency. Offline full
metadata updated that single edge; reference-game and collect-game locks received the same edge
without version changes. The UI-only consumer lock does not contain Tilemap and needs no change.

The first independent consumer build exhausted the workspace volume while compiling Naga. A
read-only cache inventory found 232.13 GiB of dependency outputs and 68.5 GiB of incrementals.
After a Cargo dry run, `cargo clean -p nara` removed 38.6 GiB of regenerable package outputs.
Source files, unrelated caches and user changes were preserved; the consumer retry passed.

Extent validation remains private to the sole file-backed admission owner. Runtime preparation
only consumes the immutable admitted Image/TileSet closure and has no separate bypass that needs
to recompute this proof. Code-first `TileSet::from_image` retains its documented caller-owned
compatibility contract. No speculative public validation framework was introduced.

# Follow-up

No in-scope implementation or verification work remains. Successor work is typed TileSet
dependency reload and generic Editor hydration, logical tile definitions before
layout-changing authoring, semantic brush edits, and measured map scaling. Streaming needs a
separate acquisition owner; Copy handles remain identity rather than lifetime leases. This
refactor does not claim those workflows or a full native release/platform qualification.

# Citations

- [Implementation plan](../../../../plans/2026-10-18-2345-refactor-component-plugin-contracts-plan.md)
- [Follow-up foundation assessment](../../../../audits/2026-10-18-next-foundation-refactor-assessment.md)
- [Asset identity contract](../../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Reload and lifetime contract](../../../../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [Schema declaration contract](../../../../architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md)
- [File-backed TileSet guide](../../../../guides/file-backed-tilesets.md)
- [Native Editor development/qualification procedure](../../../../verification/native-editor-qualification.md)
