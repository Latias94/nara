---
type: "Work Registration"
title: "Reference-game delivery: standing execution authority and U11 ready"
description: "Supersedes per-action chat authorization gates with repository-owner standing authority while preserving technical delivery gates."
timestamp: 2026-07-30T04:24:24Z
record_id: "77fb3107d88e42e4bd7367b0a2daa49b"
tags: ["rgd-u10", "rgd-u11", "delivery", "standing-authority", "active"]
status: "active"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "17081a37c2b8bc9e312bb9ed3a1048231d5aa993"
supersedes: "97da745387c9468d98777ad1535b9bb1"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-30T034418Z-rgd-u10-refreshed-standalone-candidate-completion-verification-ecd70107315740eab6c580f18eca4dd0.md"
---

# Scope

Current delivery execution policy and the dependency-valid transition from completed RGD-U10 to RGD-U11.

# Current Claim

Active: governance commit `17081a37c2b8bc9e312bb9ed3a1048231d5aa993` grants standing authority for commits, pushes, fresh workflow dispatches, tags, environment-gated actions, and Releases. RGD-U10 remains complete at source `fafc9497f7101f0c271751f2ea3dea85b3eb9101` through run `30510353046`; RGD-U11 has not yet run and may proceed after exact technical preflight without another chat confirmation.

# Latest Links

- docs/knowledge/engineering/verification/2026-07/2026-07-30T034418Z-rgd-u10-refreshed-standalone-candidate-completion-verification-ecd70107315740eab6c580f18eca4dd0.md
# Handoff

Preserve the U10 source, workflow, artifact, digest, and retention identities. Audit the U11 entry gate and dispatch a fresh evidence-ingest run when its exact source and candidate preflight passes. Do not treat historical one-shot-authorization wording as current authority; do not bypass dependency, identity, least-privilege, environment, or replay gates.

# Citations

- `AGENTS.md#repository-execution-authority`
- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#requirements`
- Commit `17081a37c2b8bc9e312bb9ed3a1048231d5aa993`
- GitHub Actions run `30510353046`
