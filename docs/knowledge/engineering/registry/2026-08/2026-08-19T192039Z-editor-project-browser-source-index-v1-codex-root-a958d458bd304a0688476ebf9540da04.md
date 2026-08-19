---
type: "Work Registration"
title: "Editor Project Browser and Source Index v1: EPB-U4 active"
description: "Activates the UI-neutral browser projection and egui Project Browser after indexed Authoring document admission was verified."
timestamp: 2026-08-19T19:20:39Z
record_id: "a958d458bd304a0688476ebf9540da04"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fe0fa55"
supersedes: "ae4a33089c7b41fd86c94ad7a95e6206"
registration_id: "editor-project-browser-source-index-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
---

# Scope

Execute EPB-U4 only: project immutable Host source-index facts into UI-neutral tooling state, add frame-bound Refresh/Open actions, and render a bounded egui Project Browser.

# Current Claim

EPB-U3 is implemented and verified at fe0fa55 with evidence 684770f357d14a0184e3cdbec4751355. EPB-U4 is the sole active unit; EPB-U5 and unrelated Editor successors remain inactive.

# Latest Links

- docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md
# Handoff

Add one Arc-backed browser view to the existing Editor project frame, keep selection/filter/expansion/query state in nara_tooling, route only Refresh/Open through the exact EditorFrameRevision, and keep filesystem authority and document admission in the Host. The egui Adapter may own only widget transients; platform wiring consumes one unified pending-work signal.

# Citations
