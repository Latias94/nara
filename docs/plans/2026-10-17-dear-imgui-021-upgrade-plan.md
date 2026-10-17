---
title: Dear ImGui 0.21 Upgrade
date: 2026-10-17
type: refactor
status: completed
execution: code
---

# Dear ImGui 0.21 Upgrade

## Scope and authority

Upgrade the selected frontend and its Winit/WGPU backends to the published 0.21.0 family under
Accepted ADR 0118. The registry packages and read-only reference checkout share upstream commit
`58a44069ee923c7378ec1663873655df8694c49f`. Existing foundation plans remain completed.
Implementation, serial local verification and precise local commits are authorized.

## Requirements

- R1. Resolve the Dear family from crates.io at 0.21.0, including independent consumers; remove
  the obsolete tracked renderer fork and its package-specific workspace/CI machinery.
- R2. Forward native events to the owning upstream window registration and use upstream core IME
  composition. Remove the duplicate preedit state/overlay while preserving Game input ownership
  and balanced press/release delivery.
- R3. Use explicit prepare, record/submit/present and main-record stages. Preserve partial
  secondary success on errors. Recording, skip or discard alone never proves a present; require
  `PresentationRequested`. A clear-only presented window grants no sampled-image composite.
- R4. Join the observed external texture registration and binding revision to the actual registered
  Nara TextureView and current image receipt under the same device owner. Binding revisions do not
  replace content, target, source or device generations.
- R5. Associate live native windows by stable registration instance, independently of successful
  presentation. Delegated receipts describe the observed native registration and UI frame, not an
  unobserved internal surface configuration. Primary Nara surfaces retain their exact generation.
- R6. Requalify the affected Windows/DX12 product paths and update current documentation without
  extending historical 0.20 evidence to the new release.

## Implementation decisions

The Host remains the event-loop, device and recovery owner. A scoped adapter created from Nara's
device scope owns the toolkit renderer and its actual external texture registrations; arbitrary
caller-supplied observations cannot grant an image receipt. Secondary work completes and joins
synchronously before primary acquisition. Nara producers are already submitted on that queue,
so no additional command scheduler is introduced.

Upstream native registration identity is deliberately distinct from internal surface lifetime or
configuration. Use an explicit delegated target identity in the private Editor receipt rather than
inventing a surface generation. Upstream still uses configured secondary extents internally; this
upgrade makes no acquired-extent or scanout claim for delegated surfaces. Nara primary acquisition
continues to use its actual texture extent.

## Execution units and ownership

1. U1: Root updates dependency/lock cohorts, removes tracked vendor files, and updates CI/current
   authority documentation. Renderer worker owns `nara_render_wgpu`'s Editor adapter/receipts and
   `editor/src/platform/imgui.rs`. Input worker owns `editor/src/frontend/imgui/` and
   `editor/src/platform/imgui/input.rs`. Workers do not run Cargo or format the checkout.
2. U2: Root integrates, runs focused behavior and native gates, obtains independent correctness
   review, records fresh evidence, closes this plan/registration and creates local commits.

## Verification

- Serial `cargo fmt --all --check` and workspace all-feature/all-target check using Rust 1.95.0.
- Focused nextest coverage of the Editor, WGPU adapter, CI policy and independent panel consumer.
- Real Windows/DX12 binding/window identity, detached view retirement, Game input/resume and IME
  composition regressions. Explicitly distinguish injected backend events from manual OS IME input.
- Review source, dependency graph and final diff; preserve unrelated/untracked local files.

Progress, exact commands, results and remaining platform limits belong in engineering memory and
the implementation ledger, not in this decision artifact.
