---
type: "Work Registration"
title: "Ordered UI kernel correction: NUI-U12 widgets and semantics active"
description: "Records NUI-U11 normalized input timeline closure and activates headless widgets, presented-frame routing, focus, modal, navigation, and semantics."
timestamp: 2026-08-05T13:29:14Z
record_id: "6b2d1b94dc86416ab2e62cb943afeaed"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c54d3667367f91dec1ad66118b2943e2922ce196"
supersedes: "3c1ccdb07e28445fb62b0e0c68176d20"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "verification/2026-08/2026-08-05T132051Z-nui-u11-normalized-input-timeline-and-pause-safe-routing-aba5a9f9a3664dfdaf3093313a43989f.md"
---

# Scope

NUI-U12 controlled widgets, exact presented-frame routing, focus, modal, navigation, and semantic accessibility facts

# Current Claim

NUI-U11 is complete: one bounded global normalized event timeline preserves exact ingress route facts, deterministic lifecycle cancellation, independent observation/UI/action progress, and pause-safe UI-before-gameplay consumption without UI or renderer dependencies.

# Latest Links

- verification/2026-08/2026-08-05T132051Z-nui-u11-normalized-input-timeline-and-pause-safe-routing-aba5a9f9a3664dfdaf3093313a43989f.md

# Handoff

Implement NUI-U12 proof-first with a new nara_ui_widgets crate and only consumer-driven kernel extensions. Resolve pending UI events against the exact retained UiFrame and UiPresentationReceipt selected by the opaque input route stamp. Keep geometry-coupled capture, target/bubble routing, modal focus scopes, navigation, restoration, semantics, and stale-frame rejection in nara_ui_kernel; keep controlled Button reducer state and bounded typed intents in nara_ui_widgets. Keep World, Host, Editor, Winit, AccessKit, and callbacks out of widget/kernel APIs. Add an independently locked widget consumer, headless semantic snapshots, and focused input/kernel/accessibility tests. Preserve concurrent user-owned files and keep Cargo serial.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u12-build-headless-widgets-focus-and-semantics`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
