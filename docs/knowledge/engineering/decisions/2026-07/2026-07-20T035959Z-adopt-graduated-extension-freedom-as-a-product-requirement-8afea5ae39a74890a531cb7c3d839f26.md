---
type: "Decision"
title: "Adopt graduated extension freedom as a product requirement"
description: "User-directed non-normative decision to preserve stable extension paths, explicit exact-version interop, and exclusive Host replacement as distinct ecosystem freedom levels."
timestamp: 2026-07-20T03:59:59Z
record_id: "8afea5ae39a74890a531cb7c3d839f26"
resource: "docs/knowledge/engineering/2026-07/2026-07-19T162113Z-plugin-freedom-and-ecosystem-ux-in-real-game-validated-engines-e91777acf3b246209026dda460554a91.md"
tags: ["plugin", "ecosystem", "interop", "host", "product-boundary"]
status: "discussed"
producer_id: "codex-root"
run_id: "20260719-deep-modules-dialogue"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "24b42dc15fa62314a7948735f62a16e89e95508b"
---

# Decision

Adopt graduated extension freedom as a Nara product requirement:

1. Ordinary game-owned Rust code and runtime plugins use stable public App, ECS, schedule, and
   domain APIs.
2. Reusable integrations prefer stable semantic contribution contracts owned by the applicable
   domain.
3. Integrations that cannot yet fit a portable contract may use an explicit exact-version,
   backend-specific interop contract with bounded authority and expected migration cost.
4. Integrations that require exclusive lifecycle or native ownership use a separately admitted Host
   replacement contract rather than escalating ordinary plugin authority.

(session-settled: user-directed - chosen over both stable-only extension seams and broadly exposing
backend internals: the selected model preserves deep ecosystem reachability without making ambient
backend authority the normal or stable plugin contract.)

This decision establishes a product requirement and evaluation model only. It does not accept a
cross-domain interop API, promote a Proposed design, authorize implementation, or prove that every
domain needs all four levels.

# Context

The discussion examined user workflows that are likely to determine whether Nara can sustain a
Rust-native ecosystem: replacing the default physics implementation, integrating Dear ImGui or
another toolkit, omitting Nara runtime UI in favor of a game-owned UI, and distributing a package
that spans runtime, editor, import, native, and content contributions.

Current evidence shows that ordinary runtime plugins retain substantial Bevy-like ECS composition
freedom. The material gaps are deeper roles: a plugin slot currently expects one concrete plugin
identity, the stock wgpu path exposes no public external rendering contribution or interop path,
normalized platform input is incomplete for a full external UI toolkit, and no clean-room package
lifecycle proves installation through safe removal.

The render-extension design and its inactive tracer plan already contain render-specific scoped
encoding, exact-version wgpu/native interop, and Render Host replacement candidates. That evidence
supports investigating the graduated model, but it cannot automatically define physics, UI,
platform, audio, or other domain contracts.

# Alternatives

- **Stable semantic contracts plus Host replacement only.** Rejected because ecosystem authors
  would have to wait for a permanent portable abstraction before proving the integration that could
  discover that abstraction. The likely fallback would be a source fork or private first-party hook.
- **Broad Bevy-style access to backend internals from ordinary plugins.** Rejected as the default
  because it would make incidental execution topology, native handles, queue ownership, and
  lifecycle authority part of the practical compatibility surface.
- **Graduated stable, exact-version, and exclusive-Host paths.** Selected because each integration
  can use the lowest authority that reaches the user outcome while retaining an honest escape hatch.

# Consequences

- Compatibility is judged by reachable user tasks, not by API-surface similarity to Bevy or another
  engine.
- Exact-version interop is explicitly advanced, backend-specific, migration-prone, and excluded from
  portable project data and normal gameplay preludes. Its contract must state capability timing,
  borrowing or retention rules, epoch identity, ordering, failure, and finite close semantics.
- Host replacement remains an exclusive authority selection with conformance requirements; it is
  not a privilege that an ordinary plugin can acquire during `Plugin::build` or `finish`.
- Stable and exact-version paths may coexist. A successful exact-version integration provides
  evidence for a future stable semantic seam but does not force one.
- Physics replacement and a primary-window Dear ImGui integration are the first named clean-room
  pressure cases. Package install, update, disable, removal, and last-good recovery are the product
  workflow gate around them.
- Each domain admits only the levels proven by production-shaped variation pressure and an
  independent implementation. The active reference-game plan remains the only implementation
  authority; the inactive render parity plan cannot execute until its stated rebaseline gate passes.

# Citations

- `STRATEGY.md`, especially Integrated modular platform and Delivery and ecosystem.
- `docs/architecture/adr/0016-extension-seams-for-backends-and-domain-modules.md`.
- `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`.
- `docs/architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md`.
- `docs/architecture/render-extension-capability-interface-design.md`.
- `docs/architecture/source-extension-package-interface-design.md`.
- `docs/plans/2026-07-15-001-feat-render-extension-parity-tracers-plan.md`, which remains
  `inactive-needs-rebaseline`.
- `docs/knowledge/engineering/2026-07/2026-07-19T162113Z-plugin-freedom-and-ecosystem-ux-in-real-game-validated-engines-e91777acf3b246209026dda460554a91.md`.
