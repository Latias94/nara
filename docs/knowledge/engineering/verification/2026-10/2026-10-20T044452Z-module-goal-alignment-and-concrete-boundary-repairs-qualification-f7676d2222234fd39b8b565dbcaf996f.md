---
type: "Verification Evidence"
title: "Module goal alignment and concrete boundary repairs qualification"
description: "All workspace modules reviewed against Strategy, existing research and pinned mature engines; TileSet, native modes, GPU allocation and glyph clipping repaired with explicit evidence limits."
timestamp: 2026-10-20T04:44:52Z
record_id: "f7676d2222234fd39b8b565dbcaf996f"
producer_id: "codex-root"
run_id: "module-goal-alignment-20261020"
related_plan: "docs/plans/2026-10-20-module-goal-alignment-review-plan.md"
git_branch: "main"
git_commit: "09a18b5"
---

# Verification

Implementation baseline: `727dcbd`. Windows 11, Rust 1.95.0. Root owns all Git integration
and build/test execution with CARGO_BUILD_JOBS=1 and the existing target. Researchers were read-only
until assigned disjoint repair files. Unrelated vendor/ remains untouched and untracked.

## Integration And Review

Fetched origin; main was an ancestor of the correction branch. Fast-forwarded and normally pushed
main to `727dcbdde5f00bfd63734eb98a420166021f0aee`; ls-remote returned that exact object ID.
Seven bounded research lanes reviewed all 35 workspace packages, root joins, actual public
consumers, existing research and pinned Bevy/Godot source. The
[review](../../../../audits/2026-10-20-module-goal-alignment-review.md) records goals, evidence,
capability/measurement distinctions and the recommended complete-workflow sequence.

References: Bevy `85503941e488152711a4695bbe248e9ed9f453fc`; Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. They are local snapshots, not latest-release claims.
Current Strategy and Accepted ADRs take precedence over old research's implementation status.

## Repairs And Evidence

- `dfd6730`: ordinary runtime guide and ADR 0003/0084 wording match ADR 0117's managed execution.
  A separate reader checked synchronous startup/failure close, retained retirement, owner-thread
  affinity and hidden first-party staging against the actual API. No runtime behavior changes.
- `3f8b6c5`: recipe guide separates generic authoring from TrustedCompiledEditorProjectIntent;
  Collect Game now has serial all-feature/all-target checks and headless workspace tests in the
  existing Windows/Ubuntu consumer CI job. YAML parsing and independent review pass. Local locked
  all-feature/all-target sample check passes; **4 gameplay tests pass**, run
  `4f9b46c1-0a40-4c2f-9947-19ff2be2f3e7`. Hosted CI is not claimed by that local result.
- `39a11ab`: TileSet capture consumes authorized publication independently of a successor Image
  attempt. Only unpublished Loading waits. Exact identity/slot checks remain authoritative.
  Starting-source first-request/retarget regression fails in `8934b88e-d2b0-46ff-812c-eb0222e9dc87`.
  Final **39 TileSet tests pass**, `0b5910b8-a8b1-4cbd-a2ba-2ff1dce25b16`, covering Loading/Failed,
  exact allocation/version/pixels, cold recovery, removal and invalid identity/raw-slot rejection.
  **85 Image tests pass**, `12b9d715-414b-4299-b726-487be18c7372`. Public content/limits cohort
  **17 passes**, `2829b30f-d24b-4dbb-a9d1-21dd7cf138bd`, 179 excluded.
- `d718412`: native startup honors WindowMode; exact exclusive extent selects highest refresh,
  then bit depth, or explicitly fails before creation. **53 Winit tests pass**,
  `8de07f32-756d-425a-a2d5-06f7d4573130`, four native tests excluded. The separate actual Windows
  native cohort **4 passes**, `9b3500d6-9ea3-4173-b13f-a21b80bebf06`: Windowed, Borderless,
  Exclusive and unsupported exclusive rejection. They observe actual mode/extent, Runtime Stopped,
  NativeDestroyed, provider release and loss of the retained native Arc. No GPU is needed by these
  window-specific native tests; other native platforms were not run.
- `09a18b5`: checked GPU allocation precedes handle use and mapped access. Focused allocation
  **3 tests pass**, `e9ee50f2-9c19-49cd-8edd-179bd45a6f72`, across ten injected resource stages,
  actual invalid buffer usage and invalid bind-group layout. The complete all-feature WGPU cohort
  **128 passes**, `54903f92-c17f-47cd-aa0c-695a98558899`, three excluded. Production encoding and
  pixel readback prove retry recovery; failed preparation publishes no invalid cache entries,
  refunds instance reservations and records zero surface acquisition/submission/presentation.
- `8582427`: glyph paint mapping preserves visible raster ink when its baseline is outside the
  viewport. Starting-source regression fails in `be44d650-8c3c-4326-b16e-c1f8903e4843` after
  independently rasterizing actual visible ink; corrected **35 UI-render tests pass**,
  `2d0e5efb-1f93-427d-855a-25fa97dcfcf2`. Direct and fractional letterboxed cases also retain
  complete offscreen rejection. Input hit-test mapping is unchanged.

At final source `09a18b5`, the locked all-feature/all-target workspace check and separate WGPU
sprite-only and UI-only all-target checks pass. Existing compiler warnings remain; no clean-warning
claim is made. An independent reader reviewed the final GPU and glyph diffs and report synthesis.
The actual Windows/DX12 Editor regression cohort **2 passes**,
`162cd177-062d-4b8e-959a-b4f33bec0062`, 184 skipped: unchanged-document Image reimport updates
presented pixels, and ordinary Scene/Prefab widgets Save/Reopen preserved source. This qualifies
those consumers after the GPU repair, not the entire native Editor suite. Final workspace
formatting check and git diff whitespace check pass.

The public content/limits run initially exposed a historical Files=4 assertion, while source-kind
discovery now reads metadata before exact load-time rereading, giving five reads. The test now
compares duplicate-edge work with the same single-edge closure; it retains one unique Prefab,
one Image, one queued job, four dependency edges and complete charge-release assertions. It does
not remove the owning budget-boundary tests. A compile error while tightening Loading diagnostic
classification was corrected before the final passing TileSet/content runs.

## Commands

All build/test commands below run sequentially. The separate sample manifest explicitly reuses
the root target through CARGO_TARGET_DIR. GPU execution sets WGPU_BACKEND=dx12.

```text
cargo +1.95.0 check --manifest-path samples/collect-game/Cargo.toml --workspace --all-features --all-targets --locked
cargo +1.95.0 nextest run --manifest-path samples/collect-game/Cargo.toml --workspace --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara_tilemap --features serde --lib --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara_image --all-features --lib --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --lib --test project_content_boot --test project_content_limits --locked -E 'test(tile) | binary(project_content_boot) | binary(project_content_limits)' --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara_winit --lib --all-features --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara_winit --lib --all-features --locked --run-ignored only -E 'test(window_mode::native_tests::)' --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run -p nara_ui_render --lib --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara_render_wgpu --lib --locked --features sprite-submitter,ui-submitter -E 'test(allocation::tests)' --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run -p nara_render_wgpu --lib --locked --all-features --test-threads=1 --no-fail-fast
cargo +1.95.0 check --workspace --all-features --all-targets --locked
cargo +1.95.0 check -p nara_render_wgpu --no-default-features --features sprite-submitter --all-targets --locked
cargo +1.95.0 check -p nara_render_wgpu --no-default-features --features ui-submitter --all-targets --locked
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source)' --test-threads=1 --no-fail-fast
cargo +1.95.0 fmt --all -- --check
git diff --check
```

## Limits

Counts overlap. Research is source inspection, not an exhaustive behavioral proof. Deterministic
allocation failure seams and actual validation failures test typed rejection/retry, cache integrity,
preparation-stage zero submission/acquisition counters and reservation refund, with recovery
readback. They are not genuine physical OOM or a full native acquisition-failure campaign.
The glyph test proves real-font packet clipping in direct and letterboxed/fractional environments,
not monitor scanout or native accessibility. Existing measured populations remain frozen.
Current Rust edit-to-result, frame-job/aggregate-worker costs, Browser/catalog/edit latency, save
restore, dynamic content delivery, IME/fallback, multiple documents and Tile tools remain explicitly
separate measurement/capability work; this review does not implement or qualify them.

# Result

Pass within the recorded scopes at source `09a18b5`. Four confirmed correctness boundaries are
repaired; guides and the independent sample CI coverage match current public behavior. The
module/goal review is complete; production maturity and recommended capabilities remain separate.

## Evidence Locations

Local target logs retain this run's raw output and are not committed: `module-review-gpu-final.log`,
`module-review-ui-render.log`, `module-review-window-native.log`, `module-review-editor-native.log`,
`tileset-published-dependency-final.log`, `module-review-image-tests.log`,
`module-review-tile-consumers-final.log`, `module-review-sample-tests.log`,
`module-review-workspace-check.log`, `module-review-gpu-sprite-check.log` and
`module-review-gpu-ui-check.log`. The exact run IDs and commands above are durable evidence locators.

# Follow-up

Follow the [review's workflow order](../../../../audits/2026-10-20-module-goal-alignment-review.md#architecture-verdict-and-refactor-order).
Do not generalize this run into physical OOM, unrun native platforms or production latency claims.
