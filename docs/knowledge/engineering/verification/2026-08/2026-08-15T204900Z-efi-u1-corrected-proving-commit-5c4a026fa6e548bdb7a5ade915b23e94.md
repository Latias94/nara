---
type: "Verification Evidence"
title: "EFI-U1 corrected proving commit"
description: "Rebinds prior EFI-U1 evidence to commit 8f3cc19 after correcting unshared clock-skewed commit dates; behavioral evidence is unchanged."
timestamp: 2026-08-15T20:49:00Z
record_id: "5c4a026fa6e548bdb7a5ade915b23e94"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u1", "correction"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8f3cc19"
verified_by: "history-correction-and-existing-focused-gates"
supersedes: "b8d1206722cd47a1931e34c05258c221"
---

# Verification

The original EFI-U1 record named local commit `6f1444b`. That object was created while the local
clock was one day ahead of the repository's authoritative date and was never published to a remote
branch. The four affected local commits were rebuilt with dates on 2026-08-15. EFI-U1 is now
implemented by `8f3cc19`.

# Result

- `6f1444b` and `8f3cc19` have the same tree
  `8477cbd8c899fb4bfe4a144014ce423e2be81fc0` and the same commit subject.
- No source, test, ADR, or verification result changed during the correction.
- The prior immutable evidence remains historical; this successor is the current proving-commit
  binding for EFI-U1.

# Evidence

- `git show -s --format="%T %s" 6f1444b`
- `git show -s --format="%T %s" 8f3cc19`
- Prior behavioral evidence:
  `docs/knowledge/engineering/verification/2026-08/2026-08-15T185031Z-efi-u1-exact-bounded-authoring-conformance-b8d1206722cd47a1931e34c05258c221.md`

# Follow-up

Use `8f3cc19` and this successor record in mutable implementation ledgers and later registration
snapshots. Do not rewrite the superseded immutable shard.

# Citations

- Commit `8f3cc19`
- Superseded local commit object `6f1444b`
- `docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md`
