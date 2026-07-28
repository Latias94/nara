---
type: "Verification Evidence"
title: "RGD-U11 verified editor persistence authority"
description: "Closes the receipt-forgery source gate with workspace-bound linear authority and post-publication content verification."
timestamp: 2026-07-28T02:59:34Z
record_id: "ecc563c521db4408a236fe8caa4a17fe"
tags: ["rgd-u11", "persistence", "nara-fs", "nara-tooling"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9dcc8cf"
---

# Verification

- Source revision: `9dcc8cf` (`fix(tooling): require verified persistence receipts`).
- Scope: `nara_fs`, `nara_tooling`, the root Editor persistence Host, public/prelude exposure,
  compile-contract fixtures, and real Save/Reopen integration.
- Review: independent adversarial and reliability re-reviews reported no remaining P0/P1/P2.
  The API-contract review's migration-record finding is closed by ADR 0047 and this evidence.

# Result

The public saved-state forgery route is closed. `EditorWorkspace::new` has no persistence writer.
The Host-only paired constructor yields one non-cloneable `EditorPersistenceAuthority`; its captured
checkpoint is non-cloneable and workspace-bound. A commit can only be created by consuming a
non-cloneable `ReplaceReceipt` whose prior/candidate identity, atomic publication, durability, and
post-publication content observation satisfy the editor contract.

`TemporaryFile` accounts the exact byte stream accepted through its capability. After replacement,
`DirectoryCapability` reopens the target, verifies the candidate identity, and computes a bounded
published digest. Written and published digests must match. Conflict, injected failure, external
temporary-file mutation, or any receipt mismatch leaves saved revision/digest unchanged and the
document dirty; ambiguous publication makes the Host sticky-uncertain until explicit Reopen.

# Evidence

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --locked`: passed.
- `cargo nextest run --locked -p nara_fs -p nara_tooling -p nara --features
  "tooling,runtime-2d,serde" --lib --test editor_persistence --test-threads=1`: 84/84 passed.
- `cargo clippy --locked -p nara_fs -p nara_tooling --all-targets -- -D warnings` with the
  repository's existing unrelated lint allowances: passed.
- `git diff --cached --check`: passed before the code commit.
- Public negative compile contracts reject cloned/constructed checkpoints and authorities,
  cloned replacement receipts, receiptless commits, and direct workspace saved-state mutation.
- Runtime regressions cover foreign authority/checkpoint rejection, external target conflict,
  same-length temporary-file mutation, failed/uncertain saves, retry blocking, and explicit Reopen.

# Follow-up

- `ConflictProtection::DetectOnly` does not prevent a non-cooperative writer from modifying the
  target after post-publication observation. External-change detection and explicit Reopen remain
  the truthful current policy; strong CAS/journaling stays under Proposed ADR 0091.
- RGD-U11 still has two source blockers: bounded terminal asset reload handling and paused-input
  transition retention. U2/U7/U8-U10 evidence remains invalidated until the final advancing
  revision is verified.
- The dependency-correction lane already exists in the active plan. After delivery evidence closes,
  execute the focused hierarchy/transform tracer and the reflect/asset deletion test before
  freezing a workspace normal-dependency allowlist; do not duplicate that decision in this record.

# Citations

- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/architecture/adr/0091-editor-persistence-recovery-and-concurrent-writer-policy.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/nara-foundation.md`
- `crates/nara_fs/src/replace.rs`
- `crates/nara_tooling/src/workspace.rs`
- `src/project_host/persistence.rs`
- `tests/editor_persistence.rs`
- Commit `9dcc8cf`
