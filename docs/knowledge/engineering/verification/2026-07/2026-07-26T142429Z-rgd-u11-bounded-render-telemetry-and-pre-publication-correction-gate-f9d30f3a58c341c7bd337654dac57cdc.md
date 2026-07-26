---
type: "Verification Evidence"
title: "RGD-U11 bounded render telemetry and pre-publication correction gate"
description: "Verifies the bounded U11 render metric surface and records five independently reproduced P1 corrections that must precede final candidates."
timestamp: 2026-07-26T14:24:29Z
record_id: "f9d30f3a58c341c7bd337654dac57cdc"
tags: ["rgd-u11", "render", "telemetry", "review", "correction"]
status: "verified-local"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2d6d5a091281b26e355379d6a7de4ae1633d61e5"
verified_by: "codex-root"
---

# Verification

Commit `2d6d5a091281b26e355379d6a7de4ae1633d61e5` adds the two missing U11
measurement surfaces without admitting a general renderer SPI:

- `FrameExecutionStart` publishes one process-local App-frame boundary before
  the first schedule runs.
- `WgpuRenderBackend` exposes opt-in, finite App-frame-to-GPU-completion
  samples; bounded adapter and configured-present-mode observations; and
  conservative texture/instance-buffer logical-byte peaks for one device
  epoch.
- Callback admission and completed samples are capped. Reconfiguration,
  callback loss, capacity exhaustion, and device retirement remain observable;
  untracked resource bytes stay conservatively charged until epoch retirement.
- App and render frame identities remain distinct so exact fixed stepping does
  not create a false telemetry fault.

The native GPU completion wait is intentionally not claimed by unit evidence.
U11's production pressure probe must still exercise the callback tail on the
actual Windows and Linux candidates.

# Result

- **Telemetry surface:** locally verified and independently reviewed with no
  remaining P0/P1 finding.
- **U11 status:** active. This record does not certify the pressure workload,
  final candidates, hosted evidence, or a Publish decision.
- **Review disposition:** five independently reproduced engine P1 defects are
  now an explicit pre-publication correction gate in the active plan: schema
  owner lineage across optional-plugin omission, prefab-local entity-reference
  namespacing, unforgeable receipt-backed save advancement, bounded terminal
  asset-reload handling, and paused-input transition retention.
- **Authority precondition:** ADR 0081/0095 already prohibit treating an
  omitted optional owner as deleted, but OQ-044 still owns the public
  owner-catalog record, predecessor, and composition shape. That bounded
  architecture decision must be reviewed before the schema correction freezes
  new persistent or public Rust types; the other four defects do not inherit
  authority from OQ-044.
- **Non-gating follow-up:** spare `Vec<u8>` capacity is a P2 ownership/metric
  issue under ADR 0068's logical-payload budget. Broader SDK, contribution,
  capability, render/importer surface, desktop facade, identity, hierarchy,
  and transform work remains successor scope.

# Evidence

- `cargo nextest run -p nara_app --lib --locked --test-threads=1`: 37 passed.
- `cargo nextest run -p nara_render_wgpu --lib --locked --test-threads=1`:
  34 passed.
- `cargo nextest run -p nara_render_wgpu --lib --features
  sprite-submitter,ui-submitter --locked --test-threads=1`: 46 passed, one
  existing platform-sensitive test skipped.
- Focused strict Clippy passed for `nara_app` and the full wgpu submitter
  feature combination after explicitly allowing only the repository's known
  unrelated `result_large_err`, `collapsible_if`, `double_must_use`,
  `too_many_arguments`, and `derivable_impls` baseline lints.
- `cargo fmt --all --check` and `git diff --check` passed.
- Independent static review checked callback/sample generations, device-epoch
  retirement, in-flight byte accounting, frame-start coverage, and fixed-step
  frame identities; its final report contained no P0/P1 finding.
- A separate independent source audit reproduced the five correction-gate
  defects against current code and the relevant Accepted ADRs. No Cargo command
  was needed for that read-only audit.

# Follow-up

Resolve the minimal OQ-044 owner-lineage decision, then land the five focused
corrections with public regressions and reconcile their authority/ledger rows.
Because they change the executable source revision, finish the pressure
workload and author journey afterward, then renew U8, U9, and U10 in dependency
order before any separately authorized U11 evidence ingest. Record broader
architecture findings in U11's handoff and select one bounded successor rather
than expanding this delivery plan.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u11-complete-pre-publication-successor-and-candidate-evidence`
- `crates/nara_app/src/lib.rs`
- `crates/nara_render_wgpu/src/telemetry.rs`
- `crates/nara_render_wgpu/src/backend.rs`
- `crates/nara_reflect/src/{schema,registry}.rs`
- `crates/nara_scene/src/prefab.rs`
- `crates/nara_tooling/src/workspace.rs`
- `crates/nara_asset/src/reload.rs`
- `crates/nara_input/src/lib.rs`
- Commit `2d6d5a091281b26e355379d6a7de4ae1633d61e5`
