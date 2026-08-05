---
type: "Verification Evidence"
title: "NUI-U9 bounded deterministic text module"
description: "Verifies exact bundled-font admission, deterministic bounded shaping and raster output, no system discovery, and direct independently locked consumption."
timestamp: 2026-08-05T11:11:50Z
record_id: "9b67d709856c4004b32df7db37c0d2ae"
tags: ["nui-u9", "text", "font", "shaping", "raster", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "95e79e853a25fa922d1715f0326b1ffa1b360056"
verified_by: "serial focused nextest, independently locked consumer, Rust 1.95 compilation, strict Clippy, dependency-feature inspection, exact asset hashes, formatting, and diff checks"
---

# Verification

NUI-U9 landed at `95e79e853a25fa922d1715f0326b1ffa1b360056`.

- `nara_text` owns immutable admitted font bytes, a content-derived face identity, singleton-family
  Parley shaping, bidi and line layout, owned Nara result types, exact physical raster keys, and
  Swash alpha masks. Third-party, ECS, Scene, atlas, renderer, window, and backend types do not
  cross its public boundary.
- Hard policy ceilings are 2 MiB per font container, 16 KiB per text, 256 lines, 4,096 glyphs, 64
  diagnostics, a 256-pixel glyph extent, and logical text size in `(0, 256]`. Injected test limits
  may reduce but cannot raise the hard ceilings.
- The embedded Fira Sans Bold face is exactly 438,028 bytes with SHA-256
  `12b632bf96ea4e760691f345ef7743c549ca387a018681e11094fcf6146244f1`. Its pinned Mozilla Fira
  SIL Open Font License text has SHA-256
  `99bfb7c1e845b79d1031fcf37e678178eb4a8c7cc74a3f218ddf7223083a6fcd`; the adjacent Bevy Mona
  Sans license was not used.
- Parley 0.11.0 uses only its `std` feature and every `FontContext` is built with
  `CollectionOptions::system_fonts = false`. Swash 0.2.6 and `ttf-parser` 0.25.1 remain private
  implementation dependencies.
- The implementation is split by ownership: `face.rs` admits immutable font data, `limits.rs`
  fixes policy, `layout.rs` defines immutable requests/results, `engine.rs` owns shaping state, and
  `raster.rs` owns mask generation.

# Result

Passed for the NUI-U9 completion signal.

- `cargo nextest run --locked -p nara_text --test-threads=1` passed 14 tests covering exact
  provenance, malformed/truncated/collection/oversized faces, empty and bounded UTF-8, Latin and
  non-ASCII shaping, bidi runs, missing glyph diagnostics, wrapping/alignment, repeated
  determinism, hard limit exhaustion, recovery after rejection, fractional scale identity, and
  bounded raster masks.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1`
  passed its direct-consumer test without the root facade, ECS, Scene, or render dependencies.
- `cargo +1.95.0 check --locked -p nara_text --all-targets --all-features -j 1` passed, as did the
  separately locked consumer check under Rust 1.95.0.
- Strict `-D warnings` Clippy passed for `nara_text` and the independent consumer. Dependency
  feature inspection confirmed no Fontique system-font feature. `cargo fmt --all -- --check` and
  staged diff checks passed.
- The full workspace all-target check compiled all production packages, including `nara_text`, but
  its test-target tail remains blocked by concurrent unstaged edits in
  `crates/nara_ui_render/src/tests.rs` that still import removed
  `HierarchyConstructionWriter` and call removed `UiNode::with_sibling_order`. NUI-U9 did not
  modify, stage, or hide that user-owned WIP.

# Follow-Up

NUI-U10 should consume only `TextLayoutRequest`, immutable measurement/glyph facts, and bounded
errors while building the renderer-neutral frame compiler. It must not expose Parley/Taffy types,
reintroduce system discovery, or make the text module own atlas residency.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u9-build-the-bounded-text-module`
- `docs/architecture/adr/0031-text-and-font-strategy.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `crates/nara_text/src/lib.rs`
- `crates/nara_text/assets/PROVENANCE.md`
- `ui-module-consumer/tests/text.rs`
