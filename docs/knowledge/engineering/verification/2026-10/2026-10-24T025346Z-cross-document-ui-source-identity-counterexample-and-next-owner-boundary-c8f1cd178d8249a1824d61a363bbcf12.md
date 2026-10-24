---
type: "Verification Evidence"
title: "Cross-document UI source identity counterexample and next owner boundary"
description: "Public same-World two-Scene characterization confirms missing containing source identity; source context and logical occurrence replacement remains Proposed."
timestamp: 2026-10-24T02:53:46Z
record_id: "c8f1cd178d8249a1824d61a363bbcf12"
producer_id: "codex"
run_id: "session-01a1629b-source-identity"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "0fa93da"
verified_by: "public Scene/UI integration test and pinned-source static review"
---

# Verification

- `cargo nextest run --locked -p nara_ui --test source_identity --test-threads=1` passed the
  public same-World integration characterization (`51b8dcd2`). The complete Tooling/UI all-feature
  cohort subsequently passed 151 tests (`2b592f39`), including that integration binary.
- The test uses only ordinary public App, Scene and UI APIs. It is a separate Rust integration
  test, not an internal adapter harness or a separate Cargo workspace.
- Two different documents contain distinct marker entities and separate Scene instance receipts.
  Different local UI root IDs compile two roots. Equal local root IDs reject the entire candidate
  with `root-key-duplicate`. Runtime close completes in both cases.

# Result

The containing-source omission is a qualified runtime finding. It does not prove action misrouting:
the current projector refuses duplicate logical roots before candidate publication, consistent
with ADR 0109. Equal Prefab anchors/chains across containing Scenes and repeated complete-source
occurrences remain source-derived follow-up requirements, not executed cases in this test.

R5a was committed independently as `0fa93da`. No source/occurrence fix is included in that commit.
The new owning design is Proposed and requires a complete direct/file-backed identity trial and
ADR 0102 conformance refreeze before changing shared UI semantics.

# Evidence

Read-only sidecar `01a16bc1-1db8-7063-81a2-5a3a1a126ab2` traced source admission, candidate custody,
publication, UI/Canvas identity, and pinned Bevy/Godot mechanisms. It recommends keeping source
content separate from caller-owned logical occurrence and rejecting incomplete persistent-spawn
inputs rather than retaining a local-ID fallback. It ran no Cargo and changed no files; it was
closed after collection. Its report is design evidence, not execution of the proposed replacement.

An earlier review agent mistakenly added an internal reproduction despite its read-only brief.
Its duplicate hierarchy placement first failed with `AlreadyPlaced` and cannot support the identity
finding. Root corrected that setup, then replaced the redundant internal test with the public
integration characterization. No unexplained user change was reverted; agent authorship was
reconciled before editing it. All Cargo evidence cited here is root-owned and serial.

# Follow-up

- Resolve the file-authored source-universe carrier and exact source/product/named-origin equality
  in the disposable complete trial. Do not infer it from project name, path, file ID or session ID.
- Qualify source-bound candidate/resolver input, occurrence-aware publication and exact retirement,
  including empty Scenes and same-source simultaneous occurrences.
- Migrate direct App, file-backed loading, preview and Canvas together; preserve retained-source
  leases and runtime incarnation fences. Do not create a second Entity resolver or permanent
  occurrence-history ledger.
- Reopen/refreeze ADR 0102 only with the owning amendment, neutral conformance and independent
  consumer evidence; preserve ADR 0109's whole-candidate duplicate-key rejection.
- The broader foundation goal remains Active. ADR 0083 remains Proposed; no importer/package or
  native/product qualification is claimed.

# Citations

- [Public characterization](../../../../../crates/nara_ui/tests/source_identity.rs)
- [Proposed owning design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
