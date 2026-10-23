---
type: "Verification Evidence"
title: "Post-boundary U1 to U8 final integration and requirement audit"
description: "Verification Evidence for Post-boundary U1 to U8 final integration and requirement audit."
timestamp: 2026-10-23T07:56:53Z
record_id: "c11cf626f4494238bf3d1a2532e40634"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "96461f9"
---

# Verification

Final source integration for the complete approved post-boundary plan, at `96461f9`. Later changes
in this delivery are documentation, memory and attributed standalone-fixture formatting only.
The user explicitly authorized the full plan, subagents, fearless API replacement and local commits.
No push or PR is authorized or performed. `vendor/` remains pre-existing and uninspected;
`repo-ref/` remains unchanged.

## Requirement Audit

| Requirement | Implemented owner and removal | Primary executed evidence |
| --- | --- | --- |
| U1 / R1 / PB1 | `7fc9458`: returned output/fault envelope and typed owned-input entry; lossy scope and Host Option inference removed | `3f13b79a544a478b974d062c99662a12`: move-only output, real Game View port, image v2 bookkeeping, edit result retention and retirement/Stop controls |
| U2 / R2 / PB2,PB4 | `7502545`: disposal custody under existing queue lock, bounded cleanup-failure outcome through Task/Runtime/Host close | Same U1/U2 evidence: deterministic caller-disposal/close and snapshot/reaper/Host failure controls |
| U3 / R3 / PB3 | `53a62bf`: capability-owned logical write cursor, exact-handle positional writes, no reopen or duplicate lock owner | `89d81288b77f48d79370b7a9dbf9b089`: Windows mixed-cursor, lock and name-replacement qualification |
| U4 / R4 / PB5 | `c48e45c`: post-flush target/support facts, original asset interference baseline retained | `e1266d181d7043a3b9ce33b60697d53c`: queued/direct/batch conflict, hook, Scene/Prefab controls |
| U5 / R5 / PB6 | `533df48`: Image-issued complete immutable captures; public pairing constructor and unused generic writable store removed; every real consumer migrated | `a532b06bf3d94666bc5f1c2748052ef5`: red/green negative API, Image92, render244, Host13, direct/renamed consumer2, Reference38 and two real Image/Canvas native trajectories |
| U6 / R6 / PB7 | `ae7a93c`: pixel-free bounded recovery continuity distinct from exact attempts; issuer/store/empty-slot association and ordinary-write revocation | `1f48883676614cde8d61d8a59e3cef2d`: final Asset/Image/TileSet252, no-serde78, Host/Scene37; external-write, Loading supersession and post-Spawn revocation negatives |
| U7 / R7 / E1 | `17f292a`: borrowed CurrentScene view and scoped ProjectScenes, original owners/predicates, two-level consumer migrated | `24bb1ff684fa45c48abb28ebc9aded12`: root37, positive/two negative rustdocs and full two-level24 including fresh-process Save/restore |
| U8 / R8 / E2 | `96461f9`: single explicit full `bevy_ecs` entry; original types, ordinary prelude and six derives retained; old alias deleted | `b54c09bf3ae54a5dbc48b382336ea785`: ECS23, negative old alias, both actual renamed fixtures and all-feature/all-target workspace compilation |

The records are under this verification directory and linked from the ADR implementation ledger.
Their original source/platform limits remain intact; a later green cohort does not rewrite a
historical failed probe or claim an unexecuted platform.

Independent reviews cover U1/U2, U3/U4 and each U5-U8 owner boundary. Concrete findings were fixed:
Host result settlement/retention; a missed independent Image resource accessor; foreign recovery
issuance; and unavailable post-Spawn invalidation passing an old exact baseline. Final bounded
reviews found no remaining blockers. Dispatch failure, interrupted reviews and read-only analysis
were never counted as test execution. Native subagents handled the U5 disjoint consumers; subsequent
bounded local Codex CLI workers/reviewers were used after native dispatch rejected arguments.

## Final Combined Checks

All Cargo commands are serial, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, `--locked`, shared root target.
Results are appended only after the corresponding process reaches a terminal success.

| Command/scope | Actual result |
| --- | --- |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Passed; existing warnings retained |
| `cargo +1.95.0 nextest run --workspace --lib --all-features --locked --test-threads=1` | 2,002 passed across 35 binaries, 32 existing ignored/excluded tests; run `108fa077-3491-488d-8486-908a98b5d20f` |
| Native Editor critical3 plus Image/Canvas2, Windows/DX12 | Five actual ignored native tests explicitly selected, executed and passed; 195 unrelated cases excluded; run `decacc1d-4651-4fc5-bf22-1f97f0798bc8` |
| `cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --workspace --test-threads=1` | Complete independent module workspace: 77 passed across 25 binaries, two existing ignored; run `c17b10ef-2403-46e6-ab9c-1fc0051c67bf` |
| `cargo +1.95.0 nextest run -p nara --all-features --locked --test runtime_instance --test runtime_driver_boundary --test workspace_play_runtime --test image_import_limits --test-threads=1` | 90 passed, none skipped, including actual negative compilation and real Runtime/Host lifecycle paths; run `b843aee3-669d-467a-87b4-f0b53f9293e2` |
| `cargo +1.95.0 nextest run --manifest-path reference-game/Cargo.toml --features desktop --locked --test first_wave --test asset_task_flow --test desktop_flow --test desktop_render --test-threads=1` | 38 passed, none skipped; run `fd61feb1-248e-43e2-8f69-89cfe68c3785` |
| `cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --test-threads=1` | Complete two-level test suite: 24 passed, none skipped, including actual fresh-process restore; run `619cbb32-bd7c-40f4-bd16-060335185098` |
| `cargo +1.95.0 nextest run -p nara_fs --test filesystem_contract --locked --test-threads=1` | 37 passed, three existing privileged cases ignored; Windows cursor/lock/exact-handle contracts execute; run `6e61ae84-0d9f-41ec-82f2-5d64c0b7d2ab` |
| `cargo +1.95.0 test -p nara -p nara_image -p nara_ecs --doc --all-features --locked` | Five passed: one positive Scene example and four negative owner/API examples, none ignored |
| `cargo +1.95.0 check --manifest-path samples/collect-game/Cargo.toml --workspace --all-targets --all-features --locked` | Passed, including the separately compiled gameplay plugin and desktop feature combination |
| `cargo +1.95.0 check --manifest-path samples/two-level-game/Cargo.toml --all-targets --all-features --locked` | Passed, including desktop targets; no claim that the two-level desktop was natively played |
| Scoped all-feature library Clippy | Root/App/ECS/Tasks/FS/Reflect/Image/Render passed with `-D clippy::correctness -D clippy::suspicious`; other advisory lints remain |
| Formatting and source hygiene | Root, Reference Game, collect-game, two-level-game, module-consumer, renamed root/ECS and renamed schedule workspaces pass Rust 1.95.0 `cargo fmt --manifest-path <manifest> --all -- --check`; affected local documentation links and `git diff --check` pass |

```powershell
cargo +1.95.0 clippy -p nara -p nara_app -p nara_ecs -p nara_tasks -p nara_fs -p nara_reflect -p nara_image -p nara_render --lib --all-features --locked -- -D clippy::correctness -D clippy::suspicious
```

The formatting command above ran separately for `Cargo.toml`, `reference-game/Cargo.toml`,
`samples/collect-game/Cargo.toml`, `samples/two-level-game/Cargo.toml`,
`tests/fixtures/module-consumer/Cargo.toml`,
`tests/fixtures/derive-dependencies/renamed-root/Cargo.toml`,
`tests/fixtures/derive-dependencies/renamed-ecs/Cargo.toml` and
`tests/fixtures/schedule-extension/renamed-root/Cargo.toml`.

The exact final native command is:

```powershell
$env:WGPU_BACKEND='dx12'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts) | test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)' --test-threads=1
```

This is actual production submission/presentation and retirement in owned native windows, not a
skipped adapter probe.

The independent module workspace uses its own lockfile, including dependency versions different
from the root workspace. It rebuilt the actual current path dependencies rather than substituting
a previous root artifact. It includes native-domain reconciliation, direct/renamed Image captures,
manual codecs, persistent references, FS, material, Editor panel and project source-domain consumers.
The exact qualified source remains the same after `88b0b80`, which only formats two already tested
standalone durable-identity expressions.

Both reference checkouts were rechecked read-only: Bevy remains at
`85503941e488152711a4695bbe248e9ed9f453fc`, Godot at
`81ab961f91066a63bcbf3ec340962bb1a1ada558`, and both worktrees are clean. Their pinned mechanism
comparison is retained; no latest-release, equivalent-guarantee or performance claim is inferred.

## Architectural Outcome

The repairs retain the selected original Bevy ECS substrate, small ordinary authoring surface,
managed Runtime owner, frozen codecs and concrete domain execution. The confirmed bugs were
structural contract defects, not evidence that another ECS kernel or universal executor is needed:
returned output is not post-operation fault, physical disposal is not cleanup success, one exact
attempt baseline is not long-lived recovery eligibility, and a neutral identity is not a coherent
content association. Correcting those distinctions in the owning modules simplifies actual Host
and game consumers; it does not merely hide repeated checks in caller helpers.

The pinned Bevy/Godot comparison informs the mechanisms, not a claim that their guarantees match
Nara's stronger exact-admission contract. The Accepted ADRs own these durable decisions. This
qualification does not prove every future module abstraction is optimal, nor eliminate the need for
measured performance and complete future consumer workflows.

# Result

All U1-U8 requirements in the approved plan have implementation, migrated real consumers,
appropriate executable evidence and resolved independent review. No required implementation or
verification item remains. The final documentation commit records this closure; it does not change
the qualified runtime source. No new migration guide, compatibility alias, ECS replacement,
universal executor, provenance log, PR or push is introduced.

The existing engine-foundation migration file remains, with the active ECS example corrected and
one explicit superseding naming row. Original historical rows and audit evidence are preserved.
Engineering memory validates; existing legacy-record and local-path warnings are not rewritten as
part of this owner refactor. Derived memory views are refreshed at integration.

## Qualification Limits

- Windows/DX12 native scope is exactly the five executed trajectories; Linux/macOS native runtime,
  monitor scanout and accessibility parity are not newly qualified.
- The 32 ignored library cases, two independent-module ignored cases and three privileged FS cases
  are not counted as passes. The five relevant native cases were separately executed, not inferred.
- Reference Game qualification is the selected 38-test desktop cohort, not every optional probe.
  Both desktop samples compile, but the two-level consumer's executed gameplay proof is headless.
- Existing compiler/Clippy advisory warnings remain; correctness and suspicious deny gates pass.
  No frame-time, memory, build-speed or cross-platform performance improvement is claimed.
- Broader partial ADR features remain partial. Completing this bounded owner plan is not a claim
  that the entire engine has no remaining architectural or product work.
