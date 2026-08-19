---
type: "Verification Evidence"
title: "EPB-U1 bounded handle directory enumeration"
description: "Verifies bounded independent Windows directory enumeration and truthful unsupported platform behavior."
timestamp: 2026-08-19T17:43:20Z
record_id: "e4b844f6b2764741a2bff0a731925a80"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "04914e1"
verified_by: "codex-root"
---

# Verification

Verified implementation commit `04914e1` against EPB-U1 of the Editor Project Browser and Source
Index v1 plan. The proving scope is the shared `nara_fs` primitive only: one bounded, move-only,
direct-child enumeration session over an independently opened exact-self Windows directory handle.

# Result

EPB-U1 passes. Windows x86-64 now reports native directory enumeration availability only after each
retained handle proves local NTFS semantics. The session derives an independent exact-self handle
with empty-name `NtOpenFile`, streams fixed-buffer `NtQueryDirectoryFileEx` records, excludes dot
entries, and attaches Nara's scoped `FileIdentity` only after the existing child-open validation.

Every batch is caller-bounded; cumulative entry, name-byte, and rejected-fact limits terminate the
session on overflow. Reparse, special, changed, cross-volume, unproven, and unportable children are
represented by bounded rejection facts rather than followed. Unix and unsupported adapters remain
explicitly Unsupported, and production code contains no ambient `std::fs::read_dir` fallback.

# Evidence

- Red phase: the focused enumeration contract initially failed because the new public types and
  method were absent.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_fs --test filesystem_contract -E
  'test(handle_bound_directory_enumeration_is_bounded_and_cursor_independent) |
  test(handle_bound_directory_enumeration_stays_on_open_object_after_name_replacement) |
  test(directory_enumeration_limit_failure_is_terminal)' --test-threads=1` passed 3/3 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_fs --test filesystem_contract --test-threads=1`
  passed 27/27 executed tests; three pre-existing privileged reparse tests remained intentionally
  skipped.
- The contract proves independent repeated cursors, multi-batch root and child enumeration, exact
  identity observations, path-name replacement resistance, and terminal cumulative-budget failure.
- `cargo fmt --all`, `git diff --check`, and the scoped ambient-enumeration search passed.
- A diagnostic experiment established that `ReOpenFile` returns `ERROR_ACCESS_DENIED` for directory
  objects on the supported host while file objects reopen successfully. The production adapter
  therefore uses capability-relative empty-name `NtOpenFile`; the plan records this evidence-backed
  correction instead of assuming unsupported directory behavior.

# Follow-up

EPB-U2 is the sole next active unit. It owns the Host-private bounded scan transaction, manifest-root
normalization, deterministic source classification, immutable last-good snapshot, pending-work
signal, and bounded final directory-lineage revalidation. It must not couple the index to Runtime
content closure, Editor UI state, async work, watchers, importers, or closure-external document Open.

# Citations

- `04914e1` (`feat(fs): add bounded handle directory enumeration`)
- `docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
