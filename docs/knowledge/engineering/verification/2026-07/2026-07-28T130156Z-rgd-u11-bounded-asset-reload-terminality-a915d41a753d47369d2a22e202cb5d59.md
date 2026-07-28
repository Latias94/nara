---
type: "Verification Evidence"
title: "RGD-U11 bounded asset reload terminality"
description: "Verifies bounded source-change, reload-request, watcher, event-observation, and image publication terminality."
timestamp: 2026-07-28T13:01:56Z
record_id: "a915d41a753d47369d2a22e202cb5d59"
tags: ["rgd-u11", "asset", "reload", "verification"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "46d8c55fdedcab0006d67d9d8c655ed821a81368"
verified_by: "codex-root"
---

# Verification

Commit `46d8c55fdedcab0006d67d9d8c655ed821a81368` closes the RGD-U11 asset-reload
source gate. The reviewed implementation covers source-change admission, dependency resolution,
reload-request ownership, concrete image consumption, terminal rejection, watcher backpressure,
bounded observation loss, and last-good/stale-publication behavior.

# Result

Verified. Every source change and reload request is subject to an item and retained-byte ceiling.
One private non-cloneable authority drains image requests. Unsupported, unregistered, missing, or
otherwise unclaimed requests become diagnosed terminal failures and release their charges in the
same task-update frame. Asset-event overflow is sticky and recoverable through an executable full
`AssetStates` rescan. A same-frame replacement invalidates an older removal attempt through
generation, state-revision, and slot-revision checks.

# Evidence

- `cargo nextest run --locked -p nara_asset --lib --no-fail-fast --test-threads=1`: 62 passed.
- `cargo nextest run --locked -p nara_image -p nara_asset_watch --no-fail-fast --test-threads=1`: 86 passed.
- Focused stale-removal regression: 1 passed.
- `cargo nextest run --locked -p nara --lib --features runtime-2d --no-fail-fast --test-threads=1`: 10 passed.
- `cargo nextest run --locked -p nara --test image_import_limits --features runtime-2d --no-fail-fast --test-threads=1`: 2 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --offline --test asset_task_flow --no-fail-fast --test-threads=1`: 1 passed.
- `cargo check --workspace --locked`: passed.
- Strict affected-crate and root `cargo clippy` gates passed with only the repository's explicit pre-existing lint allowances.
- `cargo fmt --all -- --check` and `git diff --check HEAD`: passed.
- Independent correctness, adversarial, maintainability, performance, security, testing, and final simplification reviews found no remaining P0/P1/P2 in this correction. The simplification pass applied no changes: reuse and efficiency reported none; two quality suggestions were rejected because one changed a pre-existing public API and the other added abstraction without reducing state or work.

# Follow-up

Paused-input retention remains the only open RGD-U11 source-correction gate. After it closes, U2/U7
authority review and U8-U10 hosted/candidate evidence must be refreshed in dependency order before
U11 evidence ingest. The delivery plan already defers dependency correction until that evidence
closes: trial `nara_hierarchy` plus 2D propagation, then deletion-test `nara_reflect -> nara_asset`,
then consider a workspace normal-dependency allowlist.

Residual observation: dependency fan-out traversal is bounded by the retained dependency graph,
not by the reload-request queue itself. No production untrusted-data path into that graph was found;
a future package/import path that makes graph cardinality attacker-controlled must add an admission
budget before publication.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md`
- `docs/architecture/adr/0036-event-message-and-resource-queue-lifetime.md`
- `docs/architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md`
- `docs/architecture/adr/0068-global-resource-budgets-metrics-and-diagnostic-privacy.md`
- `crates/nara_asset/src/reload/source_changes.rs`
- `crates/nara_asset/src/reload/requests.rs`
- `crates/nara_asset/src/reload/resolution.rs`
- `crates/nara_asset/src/state.rs`
- `crates/nara_asset_watch/src/observability.rs`
- `crates/nara_image/src/reload.rs`
