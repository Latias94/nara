---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U13 runtime adapter active"
description: "Records NUI-U12 presented-frame widget, focus, semantics, and accessibility closure and activates the ECS Runtime UI replacement."
timestamp: 2026-08-05T16:25:00Z
record_id: "b0316a9f09424bc2bd5b15270bd28e19"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f9abd9b4bbbc57e0da9fe08d015073797118308c"
supersedes: "6b2d1b94dc86416ab2e62cb943afeaed"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-05T162240Z-nui-u12-presented-frame-widgets-focus-and-semantics-a83546b63b6a4c87b986b762d8ba7e77.md"
---

# Scope

NUI-U13 ECS and Scene Runtime UI authoring adapter replacement

# Current Claim

NUI-U12 is complete: exact submitted-frame interaction, controlled Button intents, modal focus and capture, complete semantic state, and the optional bounded AccessKit schema bridge are verified without ECS, renderer, or callback authority in kernel/widgets.

# Latest Links

- verification/2026-08/2026-08-05T162240Z-nui-u12-presented-frame-widgets-focus-and-semantics-a83546b63b6a4c87b986b762d8ba7e77.md

# Handoff

Implement NUI-U13 as a destructive replacement of the legacy nara_ui execution tree. Preserve ordered hierarchy and exact projected identity; project canonical version-1 Scene/ECS authoring components into UiAuthoringSnapshot; compile per surface; route pending normalized input through UiFrameJournal and UiInteractionSession; reduce widgets and publish typed Runtime actions. Delete ComputedUiLayouts, ECS hit testing, UiPointerRoute, Entity/query-order fallbacks, renderer-bound root/panel fields, and legacy codec shapes. Keep kernel compiler/session internals out of ordinary preludes, preserve concurrent user-owned WIP, and keep Cargo serial.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `docs/architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-05T162240Z-nui-u12-presented-frame-widgets-focus-and-semantics-a83546b63b6a4c87b986b762d8ba7e77.md`
