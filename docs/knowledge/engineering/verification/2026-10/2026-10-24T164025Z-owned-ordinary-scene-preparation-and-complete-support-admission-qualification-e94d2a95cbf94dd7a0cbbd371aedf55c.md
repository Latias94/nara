---
type: "Verification Evidence"
title: "Owned ordinary Scene preparation and complete support admission qualification"
description: "Serial owner, public, independent and native qualification of move-only unbound Scene preparation and complete support-resource admission."
timestamp: 2026-10-24T16:40:25Z
record_id: "e94d2a95cbf94dd7a0cbbd371aedf55c"
producer_id: "codex"
run_id: "session-01a1629b-ordinary-scene-prepare-final"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_parent: "ffe4fe2f5b752ce2a46fb6fe913b81156b1c8f3f"
rust_source_manifest_sha256: "87709a808382741c7b5f9ef278eb40861a2b16fdfd792f9eedc1f00dc871856e"
---

# Verification

The ordinary Scene unit exposes move-only `prepare_scene` and `prepare_scene_replacement` over
actual native values before Scene target allocation. Immediate spawn and ordinary authoring sync
use the same split kernel. Drop needs no World; consuming commit rechecks current facts and binds
current Identity proofs without rerunning codecs. Advanced native-retirement custody remains
target-bound and retains its explicit discard protocol.

The source basis is the parent above plus 18 owned Rust file deltas. The manifest sorts paths,
hashes LF-normalized content, formats each row as `sha256  path\n`, then hashes the complete rows.
It includes the new preparation module and separately compiled consumer. The same digest was
rechecked after owner/product/native qualification; subsequent edits at that point were Markdown
only. Build artifacts are not authority for another source revision.

All Cargo commands use Rust 1.95.0, `CARGO_BUILD_JOBS=1`, existing `target`, and run serially.
Independent manifests explicitly use `--target-dir target`; no cleanup of user artifacts is used.
Raw logs remain outside the repository under `%TEMP%/nara-scene-prepare-final-*.log`.

# Result

| Scope | Actual final-source result | Nextest run |
| --- | --- | --- |
| ECS, Identity, Hierarchy, Reflect, Scene, UI, tooling; all features | 857 passed, one ignored allocation observation | `3959ac39-4ed6-475c-88b7-b29a9c4b4fa9` |
| Selected root libraries and 15 public/product integration binaries; all features | 588 passed, seven skipped | `f7fefaa0-05c2-4a5b-ae21-900c4a0fc7fd` |
| Selected actual hidden-window Windows/DX12 Editor trajectories | Six passed, 202 outside selection | `8efc38c6-a189-46f0-a55e-8eb8fe002c78` |
| Separately compiled Scene module consumer, eight selected binaries | 47 passed, one ignored intent-timing experiment | `276b4178-32e0-4ffc-a90e-1fea02ec5b08` |
| Reference Game `first_wave` and `authoring` | 31 passed | `4a7c5b79-49fe-4367-a2c3-7ca8467e2438` |

Workspace all-feature/all-target compilation and Scene no-default-feature/all-target compilation
pass. Scoped all-feature/all-target Clippy passes with `clippy::correctness` denied. Eight Scene
compile-fail doctests pass, including one-shot ordinary-token consumption. Root, module-consumer
and Reference Game workspace format checks all pass. These final commands completed serially in
exec session `77875`; the earlier owner/product/native chain `96706` also completed successfully.

# Evidence

The owner cohort runs:

```powershell
cargo +1.95.0 nextest run -p nara_ecs -p nara_identity -p nara_hierarchy -p nara_reflect -p nara_scene -p nara_ui -p nara_tooling --all-features --locked -j 1
```

The root cohort uses `-p nara --all-features --locked --lib -j 1 --no-fail-fast` and explicitly
selects `project_content_boot`, `project_content_limits`, `project_content_delivery`,
`project_scene_content`, `project_workflow_creation`, `project_workflow_cli`,
`scene_authoring_session`, `scene_inspector`, `scene_component_composition`,
`scene_sprite_serialization`, `stable_runtime_identity`, `plugin_composition`,
`scene_product_public_surface`, `startup_scene_public_surface`, and `project_runtime_boot`.
The generated independent Cargo-product test actually executes its nested offline build; its
slow result is not a stalled Cargo process or a reason to start a concurrent build.

The module manifest is `tests/fixtures/module-consumer/Cargo.toml`, selecting package
`nara-scene-module-consumer` and `scene_preparation`, `scene_spawn`, `nested_schema`,
`durable_identity`, `prefab_sources`, `source_context_trial`, `domain_reconciliation`, and
`image_reconciliation`. The Reference Game manifest is `reference-game/Cargo.toml`; both use
locked focused nextest runs with one job. Prior passing runs before the cached-resource fix are
intermediate evidence only; the table records actual final-source reruns.

The remaining checks execute:

```powershell
cargo +1.95.0 check --workspace --all-features --all-targets --locked -j 1
cargo +1.95.0 check -p nara_scene --no-default-features --all-targets --locked -j 1
cargo +1.95.0 clippy -p nara_ecs -p nara_identity -p nara_hierarchy -p nara_reflect -p nara_scene --all-features --all-targets --locked -j 1 -- -D clippy::correctness
cargo +1.95.0 test -p nara_scene --doc --all-features --locked -j 1
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 fmt --all --manifest-path tests/fixtures/module-consumer/Cargo.toml -- --check
cargo +1.95.0 fmt --all --manifest-path reference-game/Cargo.toml -- --check
```

The public preparation consumer covers partial native failure and exact disposal, world-free
cancel before target allocation, two independent tokens committed in reverse after registry
drop, replacement after individual member retirement, stale activation and empty-slot refusal,
foreign-World rejection before its pending commands flush, exact managed definition/authority,
touched versus untouched assets and late queued lifecycle changes. Explicit product projection
also proves frozen execution definition E can differ from managed Host composition H while H's
authority remains mandatory. Ordinary entry requires E==H before native preparation; there is no
foreign-definition fallback or authority bypass. ADRs 0038/0058/0081/0098 adopt these distinctions.

The final no-flush proof interval reborrows `&World` after materializing support/deferred work.
Hierarchy admits absent GenerationState insertion and present completion removal, not merely
Parent/Children/member/root changes. ECS's shared resource-insertion primitive admits the actual
cached resource entity when present, including entity and entity+component observer scopes.
The same primitive covers Reflect, Identity and Scene support. The red cases, corrected Bevy
archetype-flag control and independently rechecked owning fix are in the linked checkpoint.

Native execution sets `WGPU_BACKEND=dx12`, selects ignored tests explicitly with one test thread,
and executes these six trajectories through actual production presentation paths:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_unrelated_prefab_close_preserves_presented_play_source_until_source_close`

Code review: skipped (ce-code-review unavailable)

The actual portable invocation `01a16e75-ae68-7920-b49b-76afb7a7b8ec` terminated without a receipt
after its three failed child-agent capability discoveries; it launched no descendants and is
closed. The separate read-only scoped review `01a16e78-761c-7dc3-ab49-57de01280458` rechecked the
owning fixes and found no remaining scoped defect. Pipeline simplification and an explicit
main-owned final diff scan are additional evidence, not a completed portable receipt. No
cross-model independence is claimed.

# Follow-up

This qualifies ordinary preparation and the affected shared admission corrections, not the full
foundation rebaseline. Runtime/Canvas still omit source/occurrence coordinates and require their
coordinated key replacement and ADR 0102 refreeze. The separate representation findings prefer
complete typed addresses with shared context as an initial candidate; segmented storage must earn
its cost through the bounded lifecycle experiment. No new key format or memory result is claimed.

The ignored allocation/timing observations did not run. Selected native execution establishes
only the named Windows/DX12 trajectories, not the entire Editor suite, monitor scanout, OS IME,
accessibility, Linux/macOS native qualification or performance. Build/lint warnings remain,
including large owned Scene reports/retained-token error variants; this is not warning-free
qualification or a measured reason to add error-path allocation. No complete desktop Export or
dynamic code-unload qualification follows from this unit. The complete foundation goal remains active.

# Citations

- [Implementation and red-case checkpoint](../../progress/2026-10/2026-10-24T160924Z-ordinary-scene-preparation-and-complete-resource-effect-admission-checkpoint-20639c6f67a24f389226e694eba5ba2d.md)
- [Source/occurrence and preparation design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Public preparation consumer](../../../../../tests/fixtures/module-consumer/tests/scene_preparation.rs)
- [Owning preparation module](../../../../../crates/nara_scene/src/spawn/preparation.rs)
- [Shared resource admission](../../../../../crates/nara_ecs/src/__private.rs)
- [Key representation follow-up](../../subagents/2026-10/2026-10-24T162730Z-complete-ui-key-representation-constraints-and-bounded-experiment-cf1460fb992640a7a0d9addfd0f2de54.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
