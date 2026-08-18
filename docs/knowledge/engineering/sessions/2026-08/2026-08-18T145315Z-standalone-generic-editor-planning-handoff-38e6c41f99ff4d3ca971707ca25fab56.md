---
type: "Session Handoff"
title: "Standalone generic editor planning handoff"
description: "Reviewed handoff for the accepted standalone generic Editor boundary and implementation-ready Generic Open/Recovery plan; no implementation unit is active."
timestamp: 2026-08-18T14:53:15Z
record_id: "38e6c41f99ff4d3ca971707ca25fab56"
producer_id: "codex-root"
run_id: "sge-planning-20260818"
related_plan: "docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Summary

Nara's ordinary Editor product is one installed `nara-editor`, not an `nara` subcommand and not a
project-generated Editor binary. Generic Open and source authoring are separated from executable
Runtime composition so missing project providers or broken project Rust can enter bounded
Authoring Degraded/Recovery states instead of terminating the Editor process.

# Verified State

- ADR 0114 accepts the standalone product boundary, the non-executable Authoring Project
  transaction, data-only owner-aware schema bundle, lossless degraded records, and supervised
  child-process direction for future ordinary Play/Run.
- ADR 0090 is accepted and refined to require equality of a canonical unavailable-record manifest,
  not only aggregate semantic digests.
- The SGE plan is implementation-ready after coherence, feasibility, product, design, security,
  scope, and adversarial review. It does not activate itself.
- The first slice keeps ordinary Play/Run explicitly unavailable, retains the current in-process
  Game View only as a trusted advanced Adapter, and defers directory enumeration, Asset Browser,
  external transport, extension hosting, animation, and shader tooling.
- This planning pass changed documentation only. No Cargo command ran. `git diff --check` and the
  changed-document relative-link check passed.

# Open Threads

- No implementation unit is active. SGE-U1 must be registered separately before code work starts.
- OQ-039 owns the exact supervised child-process protocol; SGE does not implement it.
- A project-wide Asset Browser requires a focused handle-bound `nara_fs` directory-enumeration
  contract. SGE v1 opens only the manifest-selected startup source and explicit dependency closure.
- General Editor packages/extensions, animation, shader/material authoring, docking, and final Nara
  UI toolkit convergence remain successor work.

# Next Action

When implementation is authorized, register SGE-U1 and implement only the bounded authoring schema
bundle plus root-private Authoring Project admission and receipt. Do not start the shell refactor or
degraded document path before the U1 identity, lineage, capability, budget, and Adapter-receipt
contracts are proven.

# Citations

- [ADR 0114](../../../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [Standalone Generic Editor plan](../../../../plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md)
- [Implementation status](../../../../architecture/adr/implementation-status.md)
- [Product-model research](../../standalone-generic-editor-product-model-research.md)
