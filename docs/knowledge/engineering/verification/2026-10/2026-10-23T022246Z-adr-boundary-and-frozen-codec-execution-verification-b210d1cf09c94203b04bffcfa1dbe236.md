---
type: "Verification Evidence"
title: "ADR boundary and frozen codec execution verification"
description: "Serial workspace, independent consumer and focused Editor verification for the ADR boundary and frozen codec refactor."
timestamp: 2026-10-23T02:22:46Z
record_id: "b210d1cf09c94203b04bffcfa1dbe236"
producer_id: "codex-root"
run_id: "adr-frozen-execution-20261023"
related_plan: "docs/plans/2026-10-23-adr-boundaries-and-frozen-execution-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "05531a0"
---

# Verification

This record covers the successor to `12768dc` on `test/compiled-asset-domain-export`, implementing
the [ADR boundary plan](../../../../plans/2026-10-23-adr-boundaries-and-frozen-execution-plan.md).
The source includes frozen codec execution, explicit ECS authoring/upstream/internal paths,
real-consumer migrations, narrowed spatial/asset ADRs and the requested root-document cleanup.
It preserves Bevy ECS, target-World/lifecycle admission, asset baselines and cleanup custody.

Final source is committed as `05531a0`. Root-document cleanup is `4447dab`; narrowed spatial/asset
decisions and the preserved Proposed candidate are `999e909`. The final source differs from the
tested state only in independent-consumer rustfmt whitespace/import ordering, not behavior.

Commands run serially on Windows 11 with Rust 1.95.0 and `CARGO_BUILD_JOBS=1`. Root commands reuse
`target`; independent fixtures/games reuse their existing targets unless noted. The renamed ECS
and root nextest reruns explicitly use the root target. No vendor source is changed.

# Result

The owner, root integration, independent consumer and selected native checks below pass. This is
development qualification of affected paths, not full native release or cross-platform acceptance.
The full default Rust lint set is not warning-free; the specified correctness/suspicious groups
pass. Future 3D, affine-authoring representations, generic acquisition and eviction remain outside
the implementation scope.

# Evidence

| Check | Actual command or selection | Result |
| --- | --- | --- |
| Complete workspace | `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Pass after final source changes |
| Core execution/transaction owners | `cargo +1.95.0 nextest run -p nara_app -p nara_ecs -p nara_reflect -p nara_scene --all-features --locked --test-threads=1` | 607 passed, 1 skipped; `7b258f03-7db0-4319-bee0-1782443af1a0` |
| Root and domain libraries | `cargo +1.95.0 nextest run -p nara -p nara_tooling -p nara_sprite -p nara_tilemap -p nara_ui --all-features --lib --locked --test-threads=1` | 529 passed, 2 skipped; `b088a05a-4e8e-428b-91cb-674d3f17961d` |
| Root integration | `cargo +1.95.0 nextest run -p nara --all-features --locked --test runtime_instance --test project_content_boot --test scene_component_composition --test scene_sprite_serialization --test workspace_play_runtime --test scene_play_mode --test scene_inspector --test editor_persistence --test-threads=1` | 141 passed, 2 skipped; `6c8360ad-8952-408a-ac78-edff5306ed26` |
| Final independent modules | `cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --test-threads=1` | 77 passed, 2 skipped; `862efa4d-d83e-45ca-88e7-c66ca1e9edfd` |
| Renamed ECS | nextest through `tests/fixtures/derive-dependencies/renamed-ecs/Cargo.toml`, locked, one test thread | 5 passed; `8142696a-22a5-433f-9108-e1ebe2c410ed` |
| Renamed root | nextest through `tests/fixtures/derive-dependencies/renamed-root/Cargo.toml`, offline, one test thread | 7 passed; `674218b7-8b35-4a00-b882-f93df7dad6e8` |
| Renamed schedule extension | `cargo +1.95.0 test --manifest-path tests/fixtures/schedule-extension/renamed-root/Cargo.toml --locked` | 9 passed before the later import-only App dependency refinement |
| Reference game | nextest through `reference-game/Cargo.toml`, locked, one test thread | 87 passed; `493cb96d-6d6e-4947-8cba-59597797e9f3`; before the later import-only App dependency refinement |
| Final Reference targets | `cargo +1.95.0 check --manifest-path reference-game/Cargo.toml --all-features --all-targets --locked` | Pass, including desktop and Editor/host-parity targets |
| Collection game | nextest through `samples/collect-game/Cargo.toml`, locked, one test thread | 4 passed; `51e129b1-e413-40fe-a7f3-239fd3eea0ec` |
| Final two-level game | nextest through `samples/two-level-game/Cargo.toml`, locked, one test thread | 24 passed; `9d5b356f-2998-4205-ba2e-afc0199bf997`; earlier run `a51e97aa-f785-49ea-b35c-ddd0141d0b6e` also passed |
| Final Image preview helpers | root all-feature library nextest filter `test(project_host::runtime::editor::authoring_runtime::preview_content_tests)` | 5 passed; `e0e2aba8-39d8-497a-883c-67a2fffc8602` |
| Frozen-execution compile-fail contracts | `cargo +1.95.0 test -p nara_reflect --doc --all-features --locked` | 4 passed, 1 unrelated inventory example ignored |
| Renamed derive diagnostics | `cargo +1.95.0 test --manifest-path tests/fixtures/derive-dependencies/renamed-ecs/Cargo.toml --doc --locked` | 3 compile-fail tests passed |
| Scoped lint | `cargo +1.95.0 clippy -p nara_app -p nara_ecs -p nara_ecs_derive -p nara_reflect -p nara_scene -p nara_tooling -p nara --all-features --all-targets --locked -- -A warnings -D clippy::correctness -D clippy::suspicious` | Pass |
| Lock resolution | Complete offline `cargo metadata --locked --filter-platform x86_64-pc-windows-msvc` for every tracked workspace lock | All 13 workspaces pass; no package versions changed |
| Formatting and whitespace | `cargo +1.95.0 fmt --all -- --check`; `git diff --check` | Pass |

The owner tests distinguish unknown schema, failed freeze/missing native binding, actual native
absence and codec failure. A snapshot survives registry drop with exact definition identity.
Existing contextual codec, lifecycle-observer, target-World and asset-baseline tests continue to
run. The Editor regression distinguishes encoder failure from component absence without mutation.
No red-before-implementation claim is made for the new snapshot tests; their final observed result
and the compile-fail execution boundary are the qualification evidence.

## Native Editor Scope

With `WGPU_BACKEND=dx12`, execute the ignored tests in `nara-editor --lib --locked` with one test
thread:

- `native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`

Both pass in run `3464c70d-2512-4d2a-875d-3ca935263fe1` (198 unselected/ignored tests skipped).
They use actual native windows, Context input and production graphics presentation. This precedes
the final lexical-scope-only Image helper lint correction; its five focused owner tests pass
afterward. The final native rerun `8340164b-b495-4566-b87f-d68351cbfacd` also passes both tests
after that correction. No other OS/backend, monitor scanout,
manual IME, accessibility or complete critical-group qualification follows from these two tests.

## Review And Corrections

- Correctness reviewer `01a1665f-f8db-7850-922a-6902d9ebb5a8` reports no findings on the full diff
  and subsequent bounded App/import, lexical-scope and documentation changes.
- Architecture reviewer `01a16660-2e44-7c02-8d26-f86cb2142a6a` finds no blocking issue. Its two
  nonblocking observations were addressed: ADR 0005 no longer commits future Transform type
  names; App error imports use `nara_ecs::upstream`.
- Removing the normal App-to-Bevy dependency first exposed the test-only Bevy `Event` derive's
  generated path requirement. `bevy_ecs` remains a dev dependency; no additional adapted derive
  or macro framework was added. Independent lockfile resolution preserves all package versions.
- Complete offline metadata resolution updates each independent lock before locked use. Most
  changes remove only the normal App-to-Bevy edge; runtime-runner also records existing optional
  root `serde`/`serde_json` dependencies missing from its older lock. The root workspace retains
  the test-only Bevy edge in its lock.
- The first Clippy run identified two preexisting `drop_non_drop` sites in Editor Image helpers.
  Lexical scopes now end the same borrows at the same points, preserving publication order.
- An earlier plain `cargo test` module-consumer run terminated its two concurrent ImGui tests with
  Windows `STATUS_STACK_BUFFER_OVERRUN`. A one-thread rerun of those two tests passed, and the
  final complete nextest consumer run above passes with process-isolated tests and one test thread.
  The earlier in-process parallel test failure is not presented as passing or diagnosed beyond
  that observed scope.

# Follow-up

The root contains README, AGENTS and Strategy only. Editor terminology lives in its guide and
creation-template semantics in ADR 0118. Active links no longer target CONTEXT/GLOSSARY;
historical memory retains its original references, and the cited historical CONTEXT Git object
exists. The user's final direction retains the ABFE migration-guide entries.

Raw local command output remains in session/target artifacts; this record stores only outcomes
and reproducible selectors. Registration completion and final source commits are recorded by a
successor snapshot after integration.

Final completion audit maps plan units 1-5 to ADR commit `999e909`, implementation `05531a0`, root
cleanup `4447dab` and the tests/review listed here. There is no uncommitted code or unresolved
blocking review finding. Engineering-memory validation passes with existing legacy-record warnings;
derived views are current. Remote publication is outside this task.

Formatting covers root `cargo fmt --all -- --check` and every changed independent-consumer Rust
file using Rust 1.95 rustfmt with `skip_children=true`. An unrelated existing
`tests/fixtures/module-consumer/tests/durable_identity.rs` wrapping difference is left untouched;
there is no claim that the complete independent workspace had no preexisting format drift.

# Citations

- [ADR 0002](../../../../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md),
  [ADR 0044](../../../../architecture/adr/0044-root-facade-and-prelude-layering-policy.md),
  [ADR 0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Spatial candidate](../../../../architecture/spatial-affine-representation-design.md),
  [ADR 0037](../../../../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md),
  [ADR 0097](../../../../architecture/adr/0097-future-capable-2d-3d-spatial-transform-model.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
