---
type: "Work Registration"
title: "Readable runtime UI: RUI-U2 active"
description: "Closes SRT successor selection, verifies the readable product gap, and activates deterministic UI stack ordering."
timestamp: 2026-08-03T23:24:30Z
record_id: "f7194c79ff664e0dbe3f607cb1744820"
tags: ["rui-u1", "rui-u2", "runtime-ui", "text", "active"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8bcec9f"
supersedes: "18c82e290d4a4399b8f341e7ad95a3ce"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-03T232430Z-rui-u1-readable-runtime-ui-activation-c7fea22cd092467fb326065112af5619.md"
---

# Scope

Execute the bounded readable runtime-UI and deterministic-text product slice without reopening
scene lifecycle or admitting a general text, importer, widget, or render backend.

# Current Claim

SRT-U6 remains verified. RUI-U1 is complete at activation baseline `8bcec9f`. RUI-U2 is the sole
active implementation unit and owns explicit persistent UI sibling order, one validated computed
stack generation, and removal of runtime entity IDs from painter and hit-test semantics.

# Latest Links

- docs/knowledge/engineering/verification/2026-08/2026-08-03T232430Z-rui-u1-readable-runtime-ui-activation-c7fea22cd092467fb326065112af5619.md

# Handoff

Implement RUI-U2 proof-first. Update the existing UI and UI-render tests before production code,
validate duplicate root/sibling order without partial publication, make draw and hit testing share
one stack index, update the `UiNode` codec/schema honestly, run Cargo serially, and do not add font
dependencies until the ordering unit is reviewed and committed.

# Citations

- `docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md#rui-u2-publish-one-deterministic-ui-stack`
- `crates/nara_ui/src/layout.rs`
- `crates/nara_ui/src/interaction.rs`
- `crates/nara_ui_render/src/queue.rs`
