---
type: "Work Registration"
title: "Readable runtime UI: corrected RUI-U2 active"
description: "Retains deterministic UI stack ordering as the active unit after correcting the successor plan's input, font, render, and product boundaries."
timestamp: 2026-08-04T02:57:25Z
record_id: "4c6e8c1857574fb29851d3ffc470f509"
tags: ["rui-u1", "rui-u2", "runtime-ui", "text", "corrected", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2fb81c8"
supersedes: "f7194c79ff664e0dbe3f607cb1744820"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-04T025725Z-rui-u1-post-activation-contract-correction-2732c5f909c84103b8d3425cbe911cf5.md"
---

# Scope

Execute the corrected bounded readable runtime-UI product slice without reopening scene lifecycle
or admitting a general text, importer, widget, accessibility, or render backend.

# Current Claim

SRT-U6 remains verified. RUI-U1 is complete after its post-activation contract correction. RUI-U2
is the sole active implementation unit and owns explicit persistent UI sibling order, one validated
computed stack generation, removal of runtime entity IDs from painter and hit-test semantics, and
same-unit migration of current first-party UI consumers.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-04T025725Z-rui-u1-post-activation-contract-correction-2732c5f909c84103b8d3425cbe911cf5.md

# Handoff

Implement RUI-U2 proof-first. Keep the current red UI/UI-render tests, migrate the reference-game
root orders in the same unit, validate duplicate root/sibling order without partial publication,
make draw and hit testing share one stack index, update the `UiNode` codec/schema honestly, run
Cargo serially after other local Cargo work is idle, and do not add font or input dependencies yet.

# Citations

- `docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md#rui-u2-publish-one-deterministic-ui-stack`
- `docs/knowledge/engineering/verification/2026-08/2026-08-04T025725Z-rui-u1-post-activation-contract-correction-2732c5f909c84103b8d3425cbe911cf5.md`
- `crates/nara_ui/src/layout.rs`
- `crates/nara_ui/src/interaction.rs`
- `crates/nara_ui_render/src/queue.rs`
- `reference-game/src/ui.rs`
