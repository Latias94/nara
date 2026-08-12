---
type: "Work Registration"
title: "Product composition contribution correction: PCC-U1 active"
description: "Pauses RUP-U3 after the all-features requested-subset reproduction and activates the focused WGPU/Winit product-contribution correction."
timestamp: 2026-08-12T11:14:03Z
record_id: "ded726f9f9554c45a401fef2f5249576"
tags: ["product-composition", "wgpu", "winit", "product-recipe", "rup-u3", "pcc-u1", "active"]
status: "active"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "459bdf2"
supersedes: "9835c1a1f6a74d0589b95f66b149fc1c"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-12T110921Z-product-composition-requested-subset-reproduction-4f2f32ff95af4504a2e224d0868ec7a0.md"
---

# Scope

PCC-U1 only: make base WGPU feature-invariant and install concrete Sprite/UI contributions only from requested product intersections.

# Current Claim

RUP-U3 is paused. Under --all-features, a render-wgpu-only request fails because compiled UI integration changes WgpuRenderPlugin requirements; compiled Winit accessibility likewise changes unrequested native behavior.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-12T110921Z-product-composition-requested-subset-reproduction-4f2f32ff95af4504a2e224d0868ec7a0.md
# Handoff

Implement the closed first-party WGPU contribution split, preserve exact UI attempt completion, run the named requested-subset matrix serially, then proceed to PCC-U2 without activating RUP.

# Citations
