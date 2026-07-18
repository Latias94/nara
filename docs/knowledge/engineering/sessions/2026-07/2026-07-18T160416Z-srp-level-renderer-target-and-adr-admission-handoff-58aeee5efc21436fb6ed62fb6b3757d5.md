---
type: "Session Handoff"
title: "SRP-level renderer target and ADR admission handoff"
description: "Captures the non-authoritative conclusion that SRP-level renderer replaceability is a product-goal candidate while graph and public extension mechanisms remain evidence-gated."
timestamp: 2026-07-18T16:04:16Z
record_id: "58aeee5efc21436fb6ed62fb6b3757d5"
tags: ["rendering", "architecture", "srp", "adr", "candidate"]
status: "open"
producer_id: "codex-root"
run_id: "2026-07-18-render-architecture-discussion"
related_plan: "docs/plans/2026-07-15-001-feat-render-extension-parity-tracers-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f90e1b9b3235a8c5a8544eca9ba442bb7b81fd9f"
---

# Summary

The architecture discussion converged on a narrower distinction than "Nara should adopt a render
graph": SRP-level complete logical-renderer replaceability is a plausible product goal, while a
logical resource graph is only one possible private execution mechanism. The candidate product
outcome is that an external Rust package can define materially different backend-neutral renderer
policy, be selected through one coherent product choice, and integrate without forking Nara or
implicitly acquiring device, target, submission, presentation, or recovery authority.

This shard records continuity evidence only. It does not accept Pipeline Family, Renderer Profile,
Pipeline Recipe, Render Feature, graph/compiler, interop, or replacement Render Host Interfaces and
does not authorize implementation.

# Verified State

- Accepted ADR 0094 supersedes the prematurely complete ADR 0077 taxonomy. It accepts the owned
  backend-neutral frame transfer, static phase planning, and one serialized wgpu execution authority,
  while requiring production-shaped tracers before higher-authority extension mechanisms enter a
  public Interface.
- The render pressure matrix and render-extension Interface workbench already describe an SRP-level
  candidate gradient: Material Technique, Render Feature, Pipeline Family, exact interop, and Render
  Host. Both documents are explicitly non-normative.
- The render-extension parity plan has `execution_state: inactive-needs-rebaseline`. It cannot begin
  until the active reference-game plan releases its registration or records an exact file handoff,
  followed by the required activation rebaseline.
- At repository revision `f90e1b9b3235a8c5a8544eca9ba442bb7b81fd9f`, the active reference-game
  plan remains the only implementation authority. No render-extension implementation or ADR change
  was made during this discussion.

# Open Threads

- Decide whether complete external logical-renderer replaceability is a non-negotiable product
  direction or remains a capability candidate selected only after tracer evidence.
- If it becomes a product direction, define the durable outcome without preselecting exact Rust
  traits, persistent recipe shape, public graph nodes, a second render World, or replacement Host.
- Prove the lowest useful authority first: a real post-process or overlay should test Render Feature;
  a materially different stylized renderer plus an ordinary author-selection workflow should test
  complete Pipeline Family replaceability.
- Determine whether the eventual durable record should refine ADR 0094 or be a focused successor
  decision. Avoid reviving ADR 0077's complete taxonomy from aspiration alone.

# Next Action

Do not create a new Accepted ADR during the active reference-game plan. At its release or exact
handoff, rebaseline the inactive render tracer plan against landed code and current Accepted ADRs.
Then run the focused Feature and independent renderer-policy tracers. If product ownership makes
SRP-level replaceability mandatory independently of the chosen mechanism, record that outcome in a
focused ADR while leaving Interface shape and internal graph admission evidence-gated.

# Citations

- [Nara Strategy](../../../../../STRATEGY.md)
- [Architecture document authority](../../../../architecture/README.md)
- [ADR 0094: Minimal Render Execution Boundary and Evidence-Gated Extensions](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md)
- [ADR 0077: Render Pipeline Recipes, Graph Compilation, and Backend Encoding (Superseded)](../../../../architecture/adr/0077-render-pipeline-recipes-graph-compilation-and-backend-encoding.md)
- [Render Capability Demand and Pressure Matrix](../../../../architecture/render-capability-pressure-matrix.md)
- [Render Extension Capability Interface Design](../../../../architecture/render-extension-capability-interface-design.md)
- [Inactive Render Extension Parity Tracers Plan](../../../../plans/2026-07-15-001-feat-render-extension-parity-tracers-plan.md)
- [Active Reference-Game-Driven Foundation Plan](../../../../plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md)
