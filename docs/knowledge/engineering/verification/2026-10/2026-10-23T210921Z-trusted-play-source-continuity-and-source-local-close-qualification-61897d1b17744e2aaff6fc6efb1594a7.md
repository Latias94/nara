---
type: "Verification Evidence"
title: "Trusted Play source continuity and source-local Close qualification"
description: "Qualify the R5c trusted Play source through cancellation, Game handoff and physical retirement without stopping unrelated documents."
timestamp: 2026-10-23T21:09:21Z
record_id: "61897d1b17744e2aaff6fc6efb1594a7"
producer_id: "codex-foundation-integration"
run_id: "foundation-r5c-play-source-20261024"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "ebc0d48968bb318ee195abccb493199a6ee24428"
verified_by: "codex-root; CE full nara-r5c-play-source-review-gqcj3qw7"
---

# Verification

Qualified source: `ebc0d48968bb318ee195abccb493199a6ee24428`, following `3e8e85e`.
This is the R5c trusted Play-source continuity sub-unit, not completion of R5c, durable addressing,
projection/history redesign or the full foundation rebaseline. ADRs 0034/0047/0084 now distinguish
associated-source/whole-session Close from an unrelated document Close.

# Result

- Every real non-Empty trusted owner carries one private `EditorPlaySource` value: document plus
  admitted incarnation/revision. Preparing, unconsumed Starting, Game target wait/commit, Cancel,
  failed-start retirement, incomplete cleanup/retry and Active retain that association.
- Current revision resolves the retained source, not the active Prefab Inspector. Only Empty
  falls back to next-Play Scene context. Retained observations do not authorize Runtime Edit or
  Apply Changes: those still require Active, legal Runtime state and exact basis/generation.
- Unrelated Close restores the entire owner unchanged and completes only its own document/file
  cleanup. Associated Close/Exit waits for actual attempt cancellation, exact Game activation
  handoff or Stop and physical return. Start/shutdown failure provenance is not rewritten as
  success. Global Scene Reopen-versus-Play exclusion and completion-time checks remain intact.
- The move-only transition marker is source-less and cannot be taken, queried or driven. Its
  unconditional reentry assertion replaces duplicated caller bookkeeping. A temporary Option
  source in that marker was discarded during simplification; actual phases have required source,
  not a sidecar registry, duplicate digest or new public generation domain.
- Four public integration tests exercise held cancellation, failed start, Game handoff and
  Stop/CloseIncomplete with an unrelated active Prefab, exact operation results and physical
  cleanup/retry. A new actual Windows/DX12 test proves continuing submitted/composite Game frames
  through unrelated Prefab Open/Close, followed by associated-source Stop and exact target retirement.

# Evidence

All Cargo commands were serial in the existing root `target`, with `CARGO_BUILD_JOBS=1`,
Rust 1.95.0, Windows 11 and locked dependencies. No remote publication occurred.

| Final scope | Result | Nextest run |
| --- | --- | --- |
| Root, Tooling and Editor libraries | 597 passed, 26 ignored | `ffcb065b-46df-4e99-ae9a-7aac3b620544` |
| Public Play and persistence integration | 40 passed, 2 ignored | `03626f35-cd4e-4f8f-80e0-739f4d9575eb` |
| No-presentation/headless public Play | 21 passed | `4e19a68b-6014-47c5-96e1-e837f4586a14` |
| Independently compiled Editor consumers | 31 passed, 2 ignored | `9a113ac4-6a18-4a4e-a927-e133fb59a677` |
| Selected actual Windows/DX12 native trajectories | 6 passed, 201 skipped | `a12cbfb9-9968-49f8-892a-ea4a91702884` |

The final native group contains all three critical cadence/input/lifecycle trajectories, both
retained Scene/Prefab/Canvas trajectories and the new source-continuity trajectory. It uses hidden
native windows, production submission and exact composite receipts, not OS pointer injection.
The new case explicitly requires the DX12 backend. It checks completed native-frame observations,
not intra-frame ordering, monitor scanout, accessibility or Linux/macOS qualification. Held
cancellation/failed-start physical cleanup is proven separately through the public Host tests.

Root features for the public suite were `desktop-winit,presentation-2d,presentation-ui,
project-workflow-cargo-facts,render-wgpu,runtime-ui,tooling,serde`. The library command used the same
features qualified with `nara/`. Headless used `--no-default-features` and
`runtime-2d,tooling,serde,project-workflow-cargo-facts`. Independent consumers ran from
`tests/fixtures/module-consumer/Cargo.toml`, package `nara-editor-panel-module-consumer`, sharing
the root target directory. All tests used `--test-threads 1 --no-fail-fast`.

Affected-library Clippy with `-D clippy::correctness` passed; non-correctness warnings remain
(210 root, 18 Tooling and 68 Editor warnings, plus dependency-owner warnings). Root and independent
consumer `cargo fmt --all --check` passed. `git diff --check` and 112 changed-document link targets
passed before source commit. The stable native procedure was added after the frozen 14-file
review scope; it changes no production/test source and was included in those document checks.

Red run `688d3d02-3cba-4b81-acc6-b3174750542a` observed `None` instead of the admitted source
revision during failed-start retirement. The assertion/teardown process abort (`0xc0000409`) is
not evidence of production memory corruption. Focused green
`6be0dbfe-9317-4bd1-b023-190fa9902d94` passed. Initial full-feature integration
`a88f8caa-a80b-4335-b24b-53ab3a23d228` passed 23 tests; the final table supersedes its
pre-simplification source. Initial libraries passed 597 before marker simplification; the final
597-case run above reran the simplified source. The new native test also passed alone in
`c57bf757-a0ac-4294-89bd-55ca4111a774` before the final combined native group.

An initial headless command failed to compile because pre-existing parent test imports referenced
presentation-gated exports. The imports now share their consumers' feature gates and the final
21 cases pass. One combined command named the nonexistent `workspace_scene_persistence` target
after its successful library run; no integration/native result is claimed for that failed
selection. The corrected `editor_persistence` command produced the final 40-case and native runs.
Raw execution logs remain local temporary `nara-r5c-play-source-*.log` files.

## Review

CE full run `nara-r5c-play-source-review-gqcj3qw7` returned **Ready to merge**, zero findings and
zero unresolved gates. All seven Codex personas completed: correctness, project standards,
testing, maintainability, API contract, reliability and adversarial. Capacity backpressure queued
the seventh until a slot was released; no reviewer was omitted or marked failed. The merge leaf
selected zero findings, so no validator launched. The report leaf returned complete in-band JSON;
`review.json`, `metadata.json` and stage-cost status all report complete. Every handle was closed.
These are separately dispatched same-family reads, not cross-model independence. The user-required
local Codex adversarial route was used; no external-model CLI or peer job started.

Four simplification lenses completed beforehand: reuse, quality, efficiency and test value.
The marker's redundant source copy/repeated branch guard and presentation-only local bindings
were removed without weakening safety. No proof test was deleted or rewritten. No new production
comments needed a comment pass; new public accessor documentation defines the source contract.
The later native test keeps cleanup/frame-boundary comments to state its actual verification limits.

The documentation follow-up reconciles the four owning ledger rows, preserves earlier evidence's
historical scope and removes stale present-tense singleton/unstarted claims from the integrated
design and R5 plan. Its 826 local link targets and diff checks pass. Engineering-memory validation
passes with existing legacy ID, absolute-path and old-plan warnings; those historical shards were
not rewritten. Derived views are rendered by the integration owner.
Read-only documentation reviewer `01a16a9b-37c6-7d61-a7b3-38cfde62242d` confirmed the five final
log counts/IDs, complete seven-persona receipt and bounded completion claims, with no discrepancy.
It did not re-execute tests or independently classify every skipped case; the parent owns those results.

# Follow-up

The root foundation goal remains active. Next work must select and qualify projection lifetime/
bounded identity-history policy and durable source/property addressing before freezing Behavior
and declarative Prefab overlays. Do not silently clear tombstones, return Missing for expired
knowledge or re-admit a formerly Deleted identity. R5a/R5b, full R2, independent execution scopes,
Behavior/tool installation, delivery, render composition and the remaining waves are not closed
by this source-lifetime correction. No new compatibility or migration layer was introduced.

# Citations

- [R5c custody design](../../../../architecture/editor-document-persistence-custody-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0034](../../../../architecture/adr/0034-editor-play-mode-world-boundary.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0084](../../../../architecture/adr/0084-executable-runtime-ownership-and-isolation.md)
- [Public continuity tests](../../../../../tests/support/play_source_continuity.rs)
- [Native continuity test](../../../../../editor/src/platform/imgui/game_input_tests/source_continuity.rs)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
