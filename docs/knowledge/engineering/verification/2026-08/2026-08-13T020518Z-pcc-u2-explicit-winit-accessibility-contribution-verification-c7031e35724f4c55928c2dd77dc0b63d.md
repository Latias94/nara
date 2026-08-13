---
type: "Verification Evidence"
title: "PCC-U2 explicit Winit accessibility contribution verification"
description: "Serial verification of explicit Winit accessibility contribution, disabled/partial runner modes, and exact receipt-stamped platform publication."
timestamp: 2026-08-13T02:05:18Z
record_id: "c7031e35724f4c55928c2dd77dc0b63d"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "18ec878"
verified_by: "codex-root; pcc_u2_correctness_review"
---

# Verification

PCC-U2 was verified from commit `18ec87808388f3aee9eec0ae25b91883ba52175c` on
`refactor/engine-foundation-contracts`. Verification commands were run serially with the
workspace `target` directory and `--test-threads=1` where applicable.

# Result

Implemented and passing. Native Winit accessibility is now an explicit product contribution.
Cargo feature availability no longer installs or activates the platform bridge by itself. The
runner captures one `Disabled`, `Enabled(opaque access)`, or `Partial(error)` activation fact before
native window startup; partial activation fails before the runner starts.

The root presentation bridge remains neutral when the contribution is absent. When the explicit
contribution is installed, one private presentation port publishes the exact committed receipt to
the Winit platform queue. The focused integration test compared the queued `Present` snapshot's
surface, frame, slot, route, geometry, and presentation epoch with the committed receipt and found
one exact match.

# Evidence

- `cargo nextest run --locked --all-features --lib -E 'test(ui_presentation::tests::explicit_winit_accessibility_publication_uses_the_exact_presented_receipt)' --test-threads=1` — 1 passed.
- `cargo nextest run --locked --all-features --lib -E 'test(ui_presentation::tests)' --test-threads=1` — 12 passed.
- `cargo nextest run --locked -p nara_winit --features platform-accessibility --lib --test-threads=1` — 48 passed, including disabled, enabled, and partial runner modes.
- Focused all-features product-composition matrix — 5 passed, including desktop-only/UI-only absence, requested pair exactly once, and explicit direct-App contribution.
- `cargo check --locked --all-features --lib --tests --examples` — passed.
- `cargo check --locked --no-default-features --features runtime-ui,presentation-ui --lib --tests` — passed.
- `cargo check --locked --no-default-features --features runtime-core,desktop-winit --lib --tests` — passed.
- Scoped Winit and root Clippy checks with `-D warnings` — passed under the documented lint allowances.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Dedicated correctness review of all 11 changed files — no P0/P1 findings.

# Follow-up

PCC-U3 is the next active behavior unit: converge ordinary Host composition on `ProductRecipe`,
while retaining an explicit advanced/direct-App path. RUP-U3 remains paused until PCC-U3 and the
PCC-U4 composition baseline are complete. This record does not claim Runtime UI product validation,
Editor UI completion, or native-pixel/user-visibility proof.

# Citations

- [Product composition correction plan](../../../../plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md)
- Commit `18ec87808388f3aee9eec0ae25b91883ba52175c`
