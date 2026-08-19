---
type: "Work Registration"
title: "Editor Project Browser and Source Index v1: EPB-U1 active"
description: "Activates the bounded handle-relative directory enumeration unit after the durable contract was committed."
timestamp: 2026-08-19T17:02:52Z
record_id: "120048e91d4c4bd0a3fbfbce78564634"
status: "active"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4275d93"
supersedes: "45ef05245c1940fea17fa0ce398fb8a7"
registration_id: "editor-project-browser-source-index-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
---

# Scope

Execute EPB-U1 only: implement bounded handle-relative direct-child enumeration in nara_fs for Windows x86-64 local NTFS, with explicit unsupported behavior elsewhere.

# Current Claim

EPB-U0 is implemented at 4275d93. EPB-U1 is the sole active unit; EPB-U2 through EPB-U5 and all unrelated successor Editor lanes remain inactive.

# Latest Links

- docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md
# Handoff

Add red filesystem contract tests first, then implement the move-only bounded session and Windows platform adapter. Do not add ambient read_dir, recursive scanning, async work, watcher policy, or Host classification.

# Citations
