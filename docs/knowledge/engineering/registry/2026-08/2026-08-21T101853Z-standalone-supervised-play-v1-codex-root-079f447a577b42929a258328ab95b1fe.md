---
type: "Work Registration"
title: "Standalone Supervised Play v1 active after SPL-U2"
description: "Advances the standalone supervised desktop Play lane through bounded build and immutable generation publication."
timestamp: 2026-08-21T10:18:53Z
record_id: "079f447a577b42929a258328ab95b1fe"
status: "active"
producer_id: "codex-root"
run_id: "spl-u2-build-generation-20260821"
related_plan: "docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ab211bc"
supersedes: "a84f194e998448498b8d9ef4fdd03a6f"
registration_id: "standalone-supervised-play-v1-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-21T101853Z-spl-u2-bounded-desktop-build-and-generation-e0c5ea801f5e432484dfc710ae3ce00b.md"
---

# Scope

SPL-U0 through SPL-U5: one local supervised desktop child process, explicit launch context,
first-present READY, lifecycle integration, and one generic plus one native proof.

# Current Claim

SPL-U0 accepted ADR 0116 and SPL-U1 supplied the private process-tree owner, bounded output, and
cleanup receipt. SPL-U2 now supplies exact Cargo metadata/artifact facts, a bounded build attempt,
and an Editor-owned immutable executable-generation store. Child READY, Host admission, ordinary
Editor lifecycle effects, and the final product trajectories remain unclaimed.

# Latest Links

- ADR: `docs/architecture/adr/0116-standalone-supervised-editor-play.md`
- SPL-U2 verification: `docs/knowledge/engineering/verification/2026-08/2026-08-21T101853Z-spl-u2-bounded-desktop-build-and-generation-e0c5ea801f5e432484dfc710ae3ce00b.md`
- Build facade: `editor/src/supervised_play/build.rs`
- Generation facade: `editor/src/supervised_play/generation.rs`
- Pure Cargo facts: `src/project_workflow/cargo_facts.rs`

# Handoff

Start SPL-U3 with the profile execution declaration and capability-free source receipt. Preserve
the private process and generation owners; do not introduce a general transport, watcher, plugin
host, remote execution, or live-edit protocol. Keep `.codex-research/` untracked.

# Citations

- `ab211bc` (SPL-U2 implementation)
- `e0c5ea801f5e432484dfc710ae3ce00b` (SPL-U2 verification)
