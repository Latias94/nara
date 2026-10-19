---
type: Verification Evidence
title: Foundation correction U1-U15 integration and review
description: Final source, consumer, native and review evidence with explicit deferred and platform limits.
timestamp: 2026-10-19T10:49:00Z
record_id: 48e9e0ee951a488f8771c2d4731485d4
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Scope And Source

This record extends the immutable
[consumer checkpoint](../../progress/2026-10/2026-10-19-foundation-correction-consumer-checkpoint.md).
Its then-pending Export, external native runner and integration gates now have the results below.
The correction scope remains U1-U15 in the linked plan, with all 27 original A/B/M findings mapped
to implementation or explicit G1-G8 triggers. Added interface/module research is tracked by G9.
No tile tools, new executor, widget framework, push or PR entered this work.

Integration source is `bdd0e63`, following the independently committed units below. Subsequent
fixture-only lint cleanup in `0ca41f3` boxes the external runner's concrete error, borrows two one-element
adapter slices, and scopes an expected unused shared fixture to its Image test binary. No engine
behavior changes after that integration source are included in this record.

| Unit | Source | Owning evidence |
| --- | --- | --- |
| U1 | `2713396` | [Prompt identity and replay](2026-10-19-foundation-u1-close-prompt.md) |
| U2/U3 | `8b3e26b`, `ebbe005` | [Image retirement and device admission](2026-10-19-foundation-u2-u3-recovery-and-device-admission.md) |
| U4 | `0b078a0`, `964d25c` | [Cumulative scope demand](2026-10-19-foundation-u4-instance-budget.md) |
| U5 | `8cd3eb6` | [Associated Scene candidates](2026-10-19-foundation-u5-scene-candidate.md) |
| U6/U7 | `5419b10` | [Schedule admission and managed execution](2026-10-19-foundation-u6-u7-managed-schedules.md) |
| U8 | `eff95c2` | [Nonblocking Unix leaf admission](2026-10-19-foundation-u8-unix-leaf-admission.md) |
| U9 | `1ff253a` | [One text content box and neutral gallery](2026-10-19-foundation-u9-text-content-box.md) |
| U10 | `b0648b6` | [Closed Vec2 and explicit lineage](2026-10-19-foundation-u10-closed-vec2.md) |
| U11 | `7bec88f` | [Recursive native Inspector](2026-10-19-foundation-u11-recursive-inspector.md) |
| U12 | `ff64897` | [Ordinary defaults through actual Export](2026-10-19-foundation-u12-profile-defaults.md) |
| U13 | `e676117` | [Panel state/reconstruction](2026-10-19-foundation-u13-stateful-panels.md), [external public native runner](2026-10-19-foundation-u13-public-native-runner.md) |
| U14 | `dbe2900` | [Aggregate semantic lifetimes](2026-10-19-foundation-u14-semantic-lifetimes.md) |
| U15 | `31b18d1` | [Concrete TileSet resource join](2026-10-19-foundation-u15-tileset-access.md) |

The [public-consumer record](2026-10-19-foundation-public-consumer-integration.md) extends U5/U10/U14
with 12 separately compiled module-consumer cases and 38 Reference Game cases, including saved
predecessor migration, data-only editing and actual gameplay input. Those cohorts overlap other
owner selections and must not be summed as a total test count.

# Final Batch Gates

Windows, Rust 1.95.0, shared target, CARGO_BUILD_JOBS=1, no concurrent Cargo commands:

- `cargo +1.95.0 check --workspace --all-features --all-targets --locked`: passed in 4m17s
  against the final engine behavior, including the owner-helper simplifications in `bdd0e63`.
- Five ignored native Editor critical tests actually executed and passed, run
  `c55ccbc1-6dbd-4fd3-a158-5450275a6449`, 11.281s. The filter selected pause/redraw rebase,
  Game input/refocus/resize/step, independent view recovery/retirement, recursive Inspector
  Scene/Prefab Save/Reopen, and independent composed-panel drafts through GPU reconstruction.
  The 148 remaining cases were excluded by selection, not additional passing cases.
- Two Image-preview regressions actually executed with `tooling,presentation-2d,presentation-ui,serde`,
  run `40831e02-1a50-41f4-8a05-4dbced38a060`: both passed, 155 cases excluded. Their feature guards
  match Image-preview API availability; the minimal feature cohort remains separately recorded.
- The configured external module-consumer CI Clippy command passed after fixture-only cleanup:
  `cargo +1.95.0 clippy --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --workspace --all-targets -- -D warnings`.
  Existing dependency dead-code warnings remain capped; this is not a claim of a warning-free
  root workspace Clippy run.
- After that cleanup, external `nested_schema`, `vec2_authoring` and `image_reconciliation`
  selections passed 4/4 across three binaries, run `932e59d3-7b7d-4dac-9b08-0d2c175cb830`,
  0.247s after a 1m44s build. The shared fixture expectation does not disable test discovery or
  execution. Its command used the independent module-consumer manifest/lock, package
  `nara-scene-module-consumer`, those three `--test` selectors and `--test-threads=1`.
- `cargo +1.95.0 fmt --all -- --check` and equivalent `--manifest-path` checks for
  `reference-game/Cargo.toml`, `tests/fixtures/module-consumer/Cargo.toml` and
  `tests/fixtures/product-slices/presentation-ui/Cargo.toml` all passed.
- Extending the earlier U9 record's then-pending conformance gate,
  `cargo +1.95.0 run -p nara_ui_kernel --example layout_gallery --locked` passed at scales
  1.0 and 1.25: three roots, 11 layouts, 16 paint primitives and seven semantic facts, including
  the retained failure envelope and immutable prior frame. The full kernel/render owner cohort
  had already passed 123 cases. This closes U9's neutral gallery evidence.

The native command used WGPU_BACKEND=dx12 and DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1:

```text
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts) | test(native_recursive_inspector_drafts_round_trip_scene_and_prefab) | test(native_composed_panels_keep_independent_drafts_through_gpu_reconstruction)' --test-threads=1 --no-fail-fast
```

The Image-preview command was:

```text
cargo +1.95.0 nextest run -p nara --no-default-features --features tooling,presentation-2d,presentation-ui,serde --lib --locked -E 'test(generic_editor_retires_an_image_when_its_metadata_becomes_invalid) | test(generic_editor_failed_image_revocation_settles_catalog_and_blocks_metadata_writes)' --test-threads=1
```

Earlier critical consumer gates also executed rather than relying on compilation:

- `39fba8cf-ed30-41a2-ad3e-0599921dc6a0`: U12 actual compiled Check, Release Export with two
  exact product observations, packaged Headless and Windows/DX12 Desktop first presentation passed
  in 929.384s including cold builds. It verifies the ordinary selected FontFace through shaping,
  atlas ink and submitted glyph/page facts.
- `2ff99d9d-a23d-410a-98a2-70b5300bb27b`: U13 external public native runner passed in 1.240s.
  It observed an admitted selection on a later native frame, retained its draft and returned
  through normal Exit/graphics/panel cleanup. A timeout cannot produce a pass.
- `eb01fe90-95a9-4806-bb25-62e493aab320`: all 15 Editor application contracts passed after
  joining Session-accepted Exit to Application retirement, including dirty Cancel/acknowledge/
  new Exit/Discard and stale mixed-batch rejection.

# Independent Review

The full ce-code-review workflow completed with terminal status `complete` and verdict
`Ready to merge`, run `20261019-100555-5246f96c`. It reviewed base `9257bfd` through source
`0ca41f3` and the tracked integration tree: 187 changed files, diff SHA-256
`b39e6fce15d10e57b9d4b68a402df23fb5ea5c2e53d56eba73ab0aa07dfee21e`.

All ten selected lenses completed: correctness, project standards, testing, maintainability,
security, performance, API contract, data migration, reliability and adversarial. Final findings,
actionable findings, failed reviewers and unresolved validation gates are empty. The merge leaf
selected zero findings and the validator recorded `empty-batch`. Requirements R1-R17 and units
U1-U15 are marked met, with the reverse requirement check complete. These are independent local
Codex personas; no cross-model review is claimed.

Incremental correctness/adversarial review caught an earlier attempt to adopt Exit from the stale
published frame immediately after dispatch. The owner replaced it with adoption after Session
drive publication; correctness, reliability and adversarial re-review found no remaining issue.
The application and actual external native tests above passed against that correction. Later
fixture-only lint changes received their own narrow testing/API/standards reviews.

Untracked audit/evidence drafts and the standalone HTML were outside the formal code diff.
Their local links, record structure and final disposition are checked by the integration owner.
The terminal verdict is scoped to these corrections and their recorded platform evidence; it
does not authorize a push, PR, merge or release. U1-U15 are complete within that scope.

Document checks passed: 764 local Markdown links, 26 HTML local file targets, all internal anchors,
26 unique HTML IDs, a 103-row/nine-column ADR table and exactly 27 distinct A/B/M disposition
rows. The original 14 HTML finding cards and audit research body retain their prior SHA-256
digests. HTML parser nesting passes; no browser rendering/visual test is claimed. Engineering
memory validation passes with existing legacy metadata/path advisories; derived views are
regenerated and checked during this documentation integration.

# Remaining Scope And Evidence Limits

G1-G9 are recorded follow-ups, not repaired defects or hidden acceptance gates. Their precise
entry conditions remain in the plan: task submission/retirement and queue measurements; measured
authoring/render/cache pressure; existing Bevy schedule parallelism before another job kernel;
another asset/dependency role; live relocation; residency/save identities; reused domain fields;
tile identity/history; and async preparation/private module boundaries.

The [interface review](../../../../audits/2026-10-19-interface-and-module-boundaries.md) distinguishes
blocking-closure task pools from Future execution, and synchronous owner publication from
transportable preparation. The [Pele study](../../../../audits/2026-10-19-pele-interface-and-theming-study.md)
uses official docs and pinned source for semantic theme roles, metric-affecting invalidation and
local adapter retirement. It neither benchmarks Pele nor introduces it as a dependency.

Native evidence here is Windows/DX12. Hidden native tests do not prove global OS input, compositor
scanout, manual IME or Linux/macOS Editor release qualification. U8's Linux FIFO child/deadline
cohort runs original sources through a temporary standalone manifest; it does not qualify the
whole root locked workspace on Linux. Device extent admission does not prove injected allocation
OOM recovery. External consumers retain their independent lockfiles. No whole-workspace
all-features test run or complete new release qualification is claimed.

The standalone HTML remains outside docs at
`D:/Frankorz/Downloads/nara-foundation-audit-expanded-2026-10-19.html`. Original 14 research cards
remain historical source findings; its implementation table links subsequent execution evidence.
Only document checks are needed after documentation closure if production source remains unchanged.
