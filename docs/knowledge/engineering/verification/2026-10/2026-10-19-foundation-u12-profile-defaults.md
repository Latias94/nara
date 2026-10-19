---
type: Verification Evidence
title: Foundation U12 ordinary profile-default font configuration
description: One ProductRecipe configures compilation and presentation through direct composition, Check, Editor Play and actual exported native startup.
timestamp: 2026-10-19T10:11:10Z
record_id: b36a664918ed4ace83fc27ff8682cfdb
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract

`ProductRecipe::with_ui_font` records one immutable admitted FontFace. The recipe/profile-default
owner applies it to UI compilation and, when installed, the UI renderer. Direct composition uses
`with_defaults`; file-backed Hosts supply their selected profile to the same owner. Repeated font
selection, an unsupported profile and a genuinely missing required slot retain typed rejection.
Optional renderer configuration ignores only logical slot absence; divergent definitions, slot
contracts, plugin mismatches and dependency failures still reject.

The same pure recipe is reusable across reconstructed runtimes. It does not retain World state
or replace frozen definitions with process-global mutable configuration. Component schemas and
saved-source Check remain independent of gameplay execution.

# Focused Owner And Public Evidence

Windows, Rust 1.95.0, serialized Cargo with CARGO_BUILD_JOBS=1 and the shared target:

- The App/Input/Gameplay/Tilemap cohort `7f6e5032-bf1f-4cdf-8f1b-d5eef0ab784e` passed 242 tests.
  U12's optional-slot cases prove absent/present behavior, all logical occurrences, fingerprint
  intent and preservation of present-slot errors.
- Root cohort `3fa70695-ee5e-4ecb-9953-a013eb0c04bd` passed 224 tests using
  `--no-default-features --features tooling,runtime-2d,serde,presentation-ui,project-workflow`
  across the library, plugin composition and project content/runtime boot. It includes ordinary
  recipe configuration, strict Check, actual Headless shaping/rasterization, independent runtime
  state and two managed Editor Play starts/stops with the selected face.
- Renamed-root `presentation-ui` fixture `65a438a9-c570-45fb-94ea-12f946587641` passed its one
  public configuration test, resolving and instantiating both default owners from the ordinary
  recipe under the independently locked feature-minimal consumer.

Early fixture compilation incorrectly used an advanced facade and attempted to mutate a SealedApp.
The fixture now registers its test plugin during ordinary App composition before sealing. No
mutable sealed-World interface or new product authority was introduced to accommodate the tests.

# Actual Check, Export And Packaged Native Startup

Root-lockfile nextest run `39fba8cf-ed30-41a2-ad3e-0599921dc6a0` passed the ignored test
`ordinary_font_recipe_survives_compiled_check_export_and_native_packaged_startup` in 929.384 s:

`cargo +1.95.0 nextest run -p nara --all-features --locked --test project_runtime_boot --run-ignored only -E 'test(ordinary_font_recipe_survives_compiled_check_export_and_native_packaged_startup)' --test-threads=1`

Environment fixed RUSTUP_TOOLCHAIN=1.95.0, CARGO_BUILD_JOBS=1 and WGPU_BACKEND=dx12. One selected
test executed and passed; thirteen unrelated tests were excluded by selection. Cold Debug and
host-target Release builds account for most of the elapsed time; this is not a latency benchmark.

The trajectory created an independent desktop starter, copied the ordinary shared recipe into its
Rust library, selected the UI profile and generated the exact saved authoring bundle. The compiled
CLI Check passed without starting gameplay. Export then built the real Release Headless/Desktop
binaries and accepted two exact product observations through its existing prepublication and
published-candidate paths. The test subsequently ran both packaged executables from outside the
project directory; Desktop used `--verify-first-present`.

The shared canary asserts the selected face in shaping and nonempty rasterized glyph pages. Its
native assertion consumes the owner-produced PresentedUiObservation, checks the exact selected
face in the presented frame, nonempty retained glyph ink and positive WGPU submitted glyph/page
counts before reporting success. Headless's independent planner check is not used as a substitute
for this actual GPU evidence. All subprocesses returned normally through existing cleanup.

Retained local artifact: `target/nara-profile-font-86424-1/font-candidate`, containing `bin` and
the exported sibling `project`. No user project or global configuration was rewritten. The native
trajectory may briefly display ordinary Desktop windows; it injects no global pointer or keyboard
events. It establishes Windows/DX12 first presentation, not monitor scanout or Linux/macOS parity.

# Completion Boundary

The ordinary recipe, strict saved-source tooling, managed reconstruction and actual packaged
native presentation satisfy U12's focused contract. Final workspace checks, integrated review
and precise source commits remain separate goal gates. Font assets, fallback, CSS, asynchronous
Plugin lifecycles and arbitrary profile configuration were not added.
