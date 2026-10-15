---
type: "Verification Evidence"
title: "Supervised Play static checks and initial opaque identity and ordered stream regressions pass"
description: "Verification Evidence for Supervised Play static checks and initial opaque identity and ordered stream regressions pass."
timestamp: 2026-10-15T17:34:08Z
record_id: "4a4c7b0afbcb4897b6aef21ad01ed38c"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
status: "verified"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e8ac2ba"
verified_by: "codex-root"
---

# Verification

All Cargo commands ran serially with one build job and the existing target directory.

- Workspace all-feature/all-target `cargo check --workspace --all-features --all-targets
  --locked --offline` passed in session `45255`, 1m28s.
- Affected-package Clippy (`nara`, `nara-editor`, `nara_tooling`, `nara_tooling_egui`, all
  features/targets, locked/offline, correctness and suspicious lints denied) passed in session
  `16545`, 52.21s. The preceding run found a non-Drop writer explicitly dropped to end a borrow;
  a lexical scope now preserves the same write/flush/sync order. Existing style/deprecation
  warnings remain outside this focused gate.
- U7 scene library: `cargo nextest run -p nara_scene --all-features --lib --locked --offline
  --test-threads 1`, run `b909ad5d-da21-4fe1-b245-390fc96c06a6`, 116 passed, 3.736s.
- U9 sprite/sprite-render/tilemap libraries: run `9611204c-6006-406b-9f96-0e4c59e1a624`,
  47 passed, 2.544s.

# Result

U12 now has native Windows evidence in record `320ccd19841f4449b30b7d20e2ee69b0` plus the
affected static gate. This is integrated-worktree evidence; pre-existing staged Host, filesystem,
Cargo, and supervised-Play changes remain preserved, and no independently buildable HEAD is claimed.

The first U7 slice replaces generated prefab path concatenation with a complete private mapping.
Tests preserve source provenance and verify nested collision avoidance, forward references,
authored/generated locator collisions, and exact byte limits. Reference consumer migration remains
in progress.

The first U9 slice shares one authored hierarchy extraction and total per-view layer/group/Y/tie
order across sprites and tiles. Primitive/material kind no longer overrides painter order. The
palette material/native GPU probe remains in progress.

# Evidence

U9's additional hierarchy-order counterexample failed before production changes in run
`d3ee0944-df0b-44f9-ad5c-e87669777a3c`: actor-first allocation incorrectly dominated the authored
tile-first placement. The assertion was followed by managed-owner abort on Windows; the fixture
now closes before asserting, and the corrected behavior passes. The earlier layer-order red is
run `59ec5436-f366-4193-903e-951c14ca9d25`.

# Follow-up

Root stable-reference catalogue, actual Runtime/preview consumers, durable fixture migration,
U8/U10/U11/U13, and the complete final review remain unfinished. Native Linux/macOS process and
render support is not established by these Windows/library results.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T170755Z-native-supervised-play-verifies-saved-generations-failure-recovery-and-physical-cleanup-320ccd19841f4449b30b7d20e2ee69b0.md
- crates/nara_scene/src/prefab.rs
- crates/nara_sprite_render/src/extract.rs
- crates/nara_sprite_render/src/queue.rs
