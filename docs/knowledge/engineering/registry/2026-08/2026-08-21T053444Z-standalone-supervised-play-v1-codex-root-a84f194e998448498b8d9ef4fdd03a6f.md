---
type: "Work Registration"
title: "Standalone Supervised Play v1 active"
description: "Activates the standalone supervised desktop Play successor under OQ-039."
timestamp: 2026-08-21T05:34:44Z
record_id: "a84f194e998448498b8d9ef4fdd03a6f"
status: "active"
producer_id: "codex-root"
run_id: "spl-u0-supervised-play-20260821"
related_plan: "docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "90d7545"
registration_id: "standalone-supervised-play-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
---

# Scope

SPL-U0 through SPL-U5: one local supervised desktop child process, explicit launch context, first-present READY, lifecycle integration, and one generic plus one native proof.

# Current Claim

SPL-U0 has accepted ADR 0116 and narrowed OQ-039 to one local external-window desktop placement.
SPL-U1 now supplies the private Windows process-tree owner, supported Linux x86_64 process-group
adapter, bounded cancellable output, spawn authority allowlist, and terminal cleanup receipt.
Windows focused tests and a Linux cross-target check pass; native supported-Unix process evidence,
Host admission from current saved documents, Cargo generations, READY, and lifecycle integration
remain unclaimed until their later gates pass.

# Latest Links

- ADR: `docs/architecture/adr/0116-standalone-supervised-editor-play.md`
- Focused code: `src/project_host/runtime/editor/external_play.rs`
- Process code: `editor/src/supervised_play/process/**`, `editor/src/supervised_play/log.rs`
- Focused tests: `editor/src/supervised_play/tests.rs`
- Verification: `docs/knowledge/engineering/verification/2026-08/2026-08-21T090132Z-spl-u1-private-supervised-process-substrate-9b5f75ac859c466ba7f94deb74851778.md`

# Handoff

Do not implement a general transport, watcher, plugin host, remote execution, or live-edit protocol. Keep .codex-research untracked and preserve existing in-process Game View.

# Citations
