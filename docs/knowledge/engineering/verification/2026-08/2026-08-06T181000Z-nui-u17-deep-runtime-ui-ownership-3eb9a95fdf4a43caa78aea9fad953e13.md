---
type: "Verification Evidence"
title: "NUI-U17 deep runtime UI ownership and interface closure"
description: "Verifies exact presented-session authority, fail-closed routing, pause-safe journal delivery, route ownership, and private schedule/module boundaries without product-matrix expansion."
timestamp: 2026-08-06T18:10:00Z
record_id: "3eb9a95fdf4a43caa78aea9fad953e13"
producer_id: "codex-root"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commits: "7546bc9, 5bbc66b, 79910fa"
verified_by: "codex-root"
---

# Verification

NUI-U17 closes the runtime UI lifecycle checkpoint required before the reference-game product
matrix. The adapter is split into private projection, candidate, presented-history, interaction
transaction, timeline, and bounded journal modules. The exact presented frame, receipt, surface
descriptor, and immutable behavior snapshot are one interaction authority.

# Result

Passed. A candidate that has not been presented cannot replace the behavior of an older presented
frame; surface-local interaction and reducer state cannot cross surfaces; rejected, retired,
mismatched, and blocked routes fail closed; an explicitly non-UI stamped event can pass through;
paused actions remain retained until a resumed consumer acknowledges them exactly once; route
publication is idempotent for one surface, conflicts are rejected, and retirement releases only its
own endpoint. Technical schedule phases remain hidden behind the local-command helper and the
semantic `Consume`/`Capture` anchors. The Reference Game owns one cursor and unregisters it during
plugin shutdown.

# Evidence

- `cargo nextest run --locked -p nara_ui -p nara_ui_kernel --test-threads=1`: 59 passed, 0 failed.
- `cargo nextest run --locked -p nara_text -p nara_ui_kernel -p nara_ui_widgets -p nara_ui -p nara_input -p nara_gameplay --test-threads=1`: 141 passed, 0 failed.
- `cargo check --workspace --all-targets --all-features --locked -j 1`: passed.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --lib -j 1`: passed; existing dead-code warnings remain outside this unit.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1`: 3 passed, 0 failed.
- `cargo clippy --locked -p nara_ui --all-targets --no-deps -j 1 -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- The focused tests cover exact old-frame behavior, invalid-root atomicity, rejected-retirement
  reconciliation, modal unhandled-input closure, duplicate endpoint admission, bounded journal
  backpressure, cursor reclamation, and pause/resume exactly-once delivery.

# Follow-up

NUI-U15 may now provide the bounded Reference-Game product proof, but it remains a consumer of the
closed contracts rather than an authority for engine topology. NUI-U16 still owns the final public
surface, ledger, migration, and evidence reconciliation. Do not add a component gallery,
resolution laboratory, editor framework, generic registry, digest/smoke expansion, or render-topology
successor until an independent consumer or measured module invariant admits it.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u17-deepen-runtime-ui-ownership-and-interfaces`
- `docs/migrations/2026-07-engine-foundation.md#nui-u17-2-exact-presented-ui-sessions-and-pause-safe-action-delivery`
- `crates/nara_ui/src/runtime/{candidate,identity,presentation,projection,state}.rs`
- `crates/nara_ui/src/interaction/{journal,timeline,transaction}.rs`
- `reference-game/src/runtime_ui.rs`
