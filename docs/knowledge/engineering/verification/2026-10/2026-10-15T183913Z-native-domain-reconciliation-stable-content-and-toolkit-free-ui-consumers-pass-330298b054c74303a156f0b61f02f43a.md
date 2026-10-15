---
type: "Verification Evidence"
title: "Native domain reconciliation stable content and toolkit-free UI consumers pass"
description: "Verification Evidence for Native domain reconciliation stable content and toolkit-free UI consumers pass."
timestamp: 2026-10-15T18:39:13Z
record_id: "330298b054c74303a156f0b61f02f43a"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7d7f778"
---

# Verification

- Independent scene consumer's six native-domain and five durable-identity tests:
  `6cdc5d7c-d379-4fc1-9eb7-faf8f03ca69b`, 11/11 passed, 0.905s after 9.37s compilation.
- Independent UI consumer: `8679af46-13eb-455a-822c-79aeeff2a800`, 3/3 passed, 0.142s after
  48.90s compilation, using its own locked manifest and the shared target.
- Reflect, scene and ECS all-feature libraries: `b6bdc644-8162-4801-9a64-0ed00022170f`,
  194/194 passed, 8.428s after 1m17s compilation.
- Native multi-view GPU probe after its hierarchy-fixture repair:
  `ee2a82d3-41df-4677-aa68-511c7f147feb`, 1/1 passed, 1.361s. This executes actual extracted
  packets through production target encoding and reads pixels for split views, affine rotation,
  clear/load and exactly one UI overlay. The preceding 207-test run passed the other 206 tests.
- Reference Game's real `authoring_bundle --write` completed in session `97981`, 2m59s. Its
  locked dependency check correctly requested lock regeneration after material dependencies
  changed; one offline run updated that lock and regenerated the canonical bundle.

# Result

U8's concrete domain consumer now retires its real native owner, required component and
hook-created component before the unchanged lifecycle-free scene synchronization. Valid data
remains accepted when subsequent native creation fails, and explicit Retry reconstructs that
revision. Replacement and removal release the prior allocation exactly once. Invalid input
preserves the accepted revision/native state; failed startup never publishes a RuntimeInstance.

No engine core API or lifecycle guard was relaxed to achieve this result. The small consumer
guide documents the owning phases and the boundary for domains with fallible cleanup.

# Evidence

The earlier six-test U8 run (`f9e2ed55-9716-4d8b-b6ea-a00adc9d2c4d`) passed four
characterizations and failed the two intended replacement cases at PersistentProjection. The
correction lives only in the consumer's ordinary domain implementation. Test assertions unwind
outside the managed Runtime owner so physical closure completes before the original failure is
resumed. Required and hook-derived cleanup have their own actual lifecycle observations.

The UI consumer proves immutable layout, controlled-button and text shaping through public
toolkit-free modules. It imports neither ECS/Scene nor WGPU/egui authority. Its maintained CI
commands are now present but the hosted workflow has not been run from this local checkpoint.

Disk pressure was handled by Cargo's package-scoped `cargo clean -p nara-editor --profile dev
--offline`, removing 6.7 GiB of rebuildable Editor artifacts before its pending presentation API
migration. No source, user index or unowned code was removed. Cargo commands remained serialized.

# Follow-up

Finish U10's actual presentation owner/mapping/fake adapter and extraction interpolation; it is
not completed by the native multi-view probe. Recheck repaired root runtime-edit and lease
fixtures, Reference selectors and final integrated CI/static consumers. Preserve the original
index and report native Linux/macOS render/process limits. The full goal remains active.

# Citations

- module-consumer/tests/domain_reconciliation.rs
- module-consumer/tests/support/domain_reconciliation.rs
- module-consumer/README.md
- ui-module-consumer/tests/text.rs
- crates/nara_render_wgpu/src/view_probe.rs
- reference-game/src/bin/authoring_bundle.rs
