---
type: "Work Registration"
title: "Capability Directory Publication Foundation - CDPF-U2 active"
description: "Closes capability-owned directory staging and activates exact no-replace publication with truthful terminal outcomes."
timestamp: 2026-08-14T02:43:37Z
record_id: "61f5f8687540452dad45ad301f43fd1d"
resource: "nara-fs-directory-publication"
tags: ["filesystem", "project-workflow", "cdpf-u1", "cdpf-u2", "active"]
status: "active"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e345687"
supersedes: "de57fd940b3b4c889202e219f154cc55"
registration_id: "fresh-project-export-repair-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-14T024337Z-cdpf-u1-capability-owned-directory-staging-44a2fdb822d94c268972bf50f7fa7168.md"
---

# Scope

Implement CDPF-U2 exact same-parent no-replace directory publication and terminal outcomes without
adding project, export, recursive cleanup or recovery semantics to `nara_fs`.

# Current Claim

CDPF-U1 is complete at `e345687`. CDPF-U2 is the sole active implementation unit and owns native
destination revalidation, exact Windows handle-bound and Linux cooperative name-bound publication,
identity/durability receipts, and truthful `Published`, `RejectedClean` or `Orphaned` outcomes.
The Fresh Project successor remains inactive.

# Latest Links

- `docs/knowledge/engineering/verification/2026-08/2026-08-14T024337Z-cdpf-u1-capability-owned-directory-staging-44a2fdb822d94c268972bf50f7fa7168.md`

# Handoff

Implement the consuming terminal operation proof-first. Revalidate the destination immediately
before publication, use Windows handle-bound no-replace rename and Linux `renameat2`
`RENAME_NOREPLACE`, preserve every independent proof axis, and expose an orphan whenever exact
empty-directory cleanup cannot be proven. Do not add overwrite, recursive delete or product
workflow policy.

# Citations

- `docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md#cdpf-u2--exact-no-replace-publication-and-terminal-outcomes`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `crates/nara_fs/src/directory_publication.rs`
- `crates/nara_fs/src/platform/windows.rs`
- `crates/nara_fs/src/platform/unix.rs`
