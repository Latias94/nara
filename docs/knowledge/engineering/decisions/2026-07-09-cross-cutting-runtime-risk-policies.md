---
type: Decision
title: Cross-Cutting Runtime Risk Policies
timestamp: 2026-07-09T00:00:00Z
tags: nara,architecture,diagnostics,security,performance,quality
related_plan: docs/plans/2026-07-09-006-refactor-engine-lifecycle-contracts-plan.md
---

# Decision

The second-pass architecture audit identified cross-cutting risks that should be documented before implementation continues.
The repository now records them as ADRs 0048 through 0055 instead of folding all of them into the current implementation plan.

# Context

The active implementation plan already covers plugin metadata, prelude layering, component schema capabilities, app time/window event lifecycle, asset watch diagnostics, render resource lifetime, editor workspace state, and viewport-aware UI routing.
A follow-up read-only audit added broader risks around runtime observability, untrusted file input, asset-root containment, persistent file envelopes, task backpressure, tilemap scale, GPU upload budgets, and quality matrix automation.

# Alternatives

- Implement all new findings in the active plan. Rejected because it would turn one already broad lifecycle-hardening plan into an unbounded engine rewrite.
- Leave the findings in chat only. Rejected because future agents would likely rediscover or miss them after compaction.
- Record ADRs now and let the active plan implement only overlapping parts. Chosen because it keeps the current goal executable while making the next foundations durable.

# Consequences

- ADR 0048 defines the runtime diagnostics and observability bus.
- ADR 0049 defines untrusted project input and parse/decode budget policy.
- ADR 0050 defines asset-root symlink, junction, and package trust policy.
- ADR 0051 defines persistent file envelopes, migrations, patch field-path migration, and golden fixtures.
- ADR 0052 defines task backpressure, cancellation, and long-running diagnostics.
- ADR 0053 defines visibility, culling, and tilemap render cache policy.
- ADR 0054 defines GPU upload budget and buffer allocation policy.
- ADR 0055 defines the local feature matrix, dependency boundary checks, and CI-ready fixture policy.
- The active plan cites these policies as constraints and follow-up boundaries; it does not require implementing their full code surface in the current goal.

# Citations

- [Engine Lifecycle Contracts Plan](../../../plans/2026-07-09-006-refactor-engine-lifecycle-contracts-plan.md)
- [ADR 0048](../../../architecture/adr/0048-runtime-diagnostics-and-observability-bus.md)
- [ADR 0049](../../../architecture/adr/0049-untrusted-project-input-and-parse-budget-policy.md)
- [ADR 0050](../../../architecture/adr/0050-asset-root-symlink-junction-and-package-trust-policy.md)
- [ADR 0051](../../../architecture/adr/0051-persistent-file-envelope-migration-and-golden-fixtures.md)
- [ADR 0052](../../../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
- [ADR 0053](../../../architecture/adr/0053-visibility-culling-and-tilemap-render-cache.md)
- [ADR 0054](../../../architecture/adr/0054-gpu-upload-budget-and-buffer-allocation-policy.md)
- [ADR 0055](../../../architecture/adr/0055-feature-matrix-boundary-checks-and-compatibility-fixtures.md)

