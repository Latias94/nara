---
type: "Work Progress"
title: "Reviewed real artifact unlink integration and incomplete publication follow-up"
description: "Records real ready-artifact disposal, namespace and cleanup corrections, focused verification and the concrete incomplete-publication owner successor."
timestamp: 2026-10-28T18:25:59Z
record_id: "1de5c56631a146318b4b1098eb0fdfd8"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "feat/reviewed-artifact-reclamation"
related_plan: "docs/plans/2026-10-28-managed-source-tool-artifacts-plan.md"
---

# Summary

Continue from local `main@65e6a24`, without remote publication. The full foundation Goal remains
active. Preserve actual `DirectoryCapability::unlink_file`; the owner now uses it for registered
ready artifacts after exact graph withdrawal, current source/user checks and intent publication.
This is not completion of the artifact lifecycle or the full foundation refactor.

# Details

The move-only review captures the original artifact/selection bases, actual root/directory/leaves,
missing observations and namespace custody. Confirm revalidates these facts and fences conflicting
work. The existing artifact publisher must accept and physically return the bounded intent before
the existing Tasks owner admits per-member removal. Recheck actual selection binding/digest across
that wait. Retain each unlink receipt before another member; cancellation or first failure stops
later work. Durable Pending, Removed, Missing and Failed remain distinct. Fresh Open only observes
the actual disk record, never restores execution/deletion authority or automatically continues.

Results publication failure retains exact mixed results and publication debt. Explicit artifact
reconciliation plus retry republishes facts only. Close revokes new admission immediately, drains
owned results before shutting the publisher, and can physically finish with unpublished results in
its failure receipt. Application retains every failed Session receipt keyed by its actual entry
generation; the first-error getter remains the native exit summary. Repeated Close polling does
not duplicate the same Session.

Independent source review found unavailable graph mistaken for absence, new persistence/conversion
IO during removal, endless Close on terminal publication debt, loss of surviving creation custody,
loss of later Application receipts and root Tasks cleanup publication ordering. Owning fixes are in
the current tree. Root admission now publishes sticky cleanup failure before CleanupFailed becomes
visible; a deterministic gate holds the interval before running-accounting removal.

Actual tests exposed two filesystem ownership mistakes. Completed installation retained its
directory creation DELETE handle, preventing namespace lease admission. Exact current-directory
open plus same-object comparison while that original remains live now hands over to a read/traverse
handle; failed handover retains unverified publication and original custody. The old immutable
launch lease also blocked the caller's own child publication. A distinct opaque namespace lease
uses exact-self read/write sharing without delete sharing; launch sharing is unchanged. Other
platforms refuse this new lease rather than treating duplication as exclusion.

Focused evidence before final all-library qualification:

- Tasks: `9ba6037d-5bad-4670-b86d-3773e42d5ba6`, 97 passed, no skips.
- Filesystem: `192fe564-7553-4372-97ae-d3fd48db8334`, 47 passed, no skips. Includes actual directory
  namespace exclusion plus child publication/rename/unlink. A later receipt assertion was added.
- Root artifact/reclamation/installation: `5f0849d6-c4a5-4d17-895b-5e8a1c4f2134`, 67 passed,
  772 outside-selection skips, 12.649 seconds. Includes real locked-member partial failure,
  cancellation, failed intent/results, Close debt receipt, selection drift/unavailability,
  equal-byte replacement and surviving alias protection. A test expecting populated-directory
  rename after dropping one lease was corrected to inspect DELETE admission; retained descendant
  handles have independent Windows rename restrictions. The filesystem test retains actual rename.

Earlier failures are not qualified results: first test compile used private record internals;
first real runs exposed the two namespace issues above; one unsupported-format fixture omitted
the now-required reclamations array. Assertion-unwind aborts in directly retained Session tests
are not independently established native buffer overflows.

At this capture, root/Tooling/Editor/Filesystem/Tasks all-feature libraries are running serially;
no final result is yet claimed. All workers are frozen. Root/independent formatting completed.
Reviewer `01a183bc-bd18-72f3-ba60-495c5b823eca` is checking the final namespace/handover/Close changes.
Independent/native consumer validation, minimal checks, Clippy and final commit remain.

# Incomplete Publication Successor

Read-only researcher `01a183ae-b3c7-7542-ace3-5eab900d3a0a` identified a concrete next owner
replacement. Current registration/record require a ready installation; unready artifacts cannot
yet enter review. Do not claim complete reclamation or discard their retained cleanup custody.

Use one artifact-publication entry with Ready/Incomplete facts rather than a fictitious ready
receipt. Allocate correlation before publication; ready receipt can retain that same ID. A
publisher-constructed concrete incomplete owner must retain actual project/store/destination,
published-directory receipt, exact created members and cleanup-only members with their actual
paths/roles/intended baselines. Receipt temporary paths remain actual temporary names, never
fabricated ready receipt names. Empty/subset incomplete publications are legitimate bounded facts.

Cleanup-only members require an exact live-object reader handover before ordinary unlink review.
Retain original cleanup/error on any uncertainty. `TemporaryFileCleanup::discard` is not an
unlink receipt and cannot produce Removed. Cleanup's retained parent Arcs can keep creation
DELETE access alive even after replacing the orphan directory field. Alias exclusion must include
these original objects; a narrow same-object query may be necessary. Graph absence must exclude
the same actual directory or path as well as ID, including another ready identity at that location.

Replace the unreleased canonical format directly, retaining version 1, finite bounds and only one
reader. Reuse the same artifact publisher, review, removal task and result recovery. Do not add a
second reclaimer, recursive cleanup, compatibility aliases or a generic transaction framework.
Actual payload/sync/read-open failures, receipt-temp competition, unknown siblings, retained source
aliases, partial cancellation and fresh-Open controls must accompany this next replacement.
This is an implementation direction for the next bounded unit, not current implemented behavior.

# Next Action

Complete independent review and current ready-artifact consumer qualification, record actual
results and create precise local Conventional Commits/ff main. Then continue incomplete artifact
publication ownership, native template identity/remapping and the dependency-ordered foundation
plan. Never close the foundation Goal because this ready-artifact slice passes.

# Citations

- [Artifact plan](../../../../plans/2026-10-28-managed-source-tool-artifacts-plan.md)
- [Ownership design](../../../../architecture/source-tool-artifact-ownership-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Microsoft sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
