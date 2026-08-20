---
type: "Work Registration"
title: "Editor Image Import and Reimport v1: EIR-U1/U2 atomic migration active"
description: "Reconciles the build-green boundary: typed metadata deletion and all strict consumers migrate atomically."
timestamp: 2026-08-20T03:06:24Z
record_id: "a8651338ca4940429199c29e478862be"
status: "active"
producer_id: "codex-root"
run_id: "eir-u12-atomic-20260820"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9efde9d"
supersedes: "8271913ae26044eda4c910646e49f6ea"
registration_id: "editor-image-import-reimport-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
---

# Scope

Execute EIR-U1 and the mechanically inseparable EIR-U2 strict-consumer migration as one build-green atomic unit; EIR-U3 through EIR-U6 remain inactive.

# Current Claim

EIR-U1 cannot delete the unreleased flat reader while EIR-U2 consumers and fixtures remain on that reader. The two units are therefore one atomic migration boundary without a compatibility layer.

# Latest Links

- docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md
# Handoff

Install the typed Image recipe, reset canonical asset_meta, migrate strict ProjectContent, Generic Authoring identity admission, runtime reload, examples, tests, and fixtures, then remove every obsolete flat/hash/global-settings path. Do not begin the Host transaction from EIR-U3.

# Citations
