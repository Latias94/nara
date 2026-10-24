---
type: "Verification Evidence"
title: "Exact stored spelling through filesystem capabilities"
description: "Bounded exact descendant-name opens preserve capability authority and reject native lookup aliases."
timestamp: 2026-10-24T09:54:18Z
record_id: "79d51b36d2944aa0bc70384eb2d11b86"
producer_id: "codex"
run_id: "session-01a1629b-source-admission"
---

# Verification

The source-admission review found that Windows case-insensitive lookup could assign two different
location keys to the same native file. A filesystem-owned exact-name primitive fixes that boundary;
lowercasing in UI, deriving semantic identity from a file handle, or merely clearing Windows'
case-insensitive lookup flag are not equivalent fixes.

# Result

`open_file_exact` and `open_directory_exact` verify every descendant component while retaining
the same object-kind, no-reparse, volume and strict link checks as ordinary capability opens.
Ordinary opens keep their existing lookup semantics. Windows NTFS/ReFS uses one fixed 64 KiB
normalized-name query per component. Linux uses a native entry scan capped at 65,536 entries per
component, then joins that exact native entry's inode and no-follow `statat` device/inode to
the opened handle. No sibling list,
normalization, ambient path authority or path reopen is introduced.

This is a per-component observation; it does not freeze concurrent renames or source bytes.
The Linux scan is bounded linear sibling work and can reject large directories. It is not a
performance qualification or proof of casefold-volume behavior. Other platforms
remain explicitly unsupported for exact-name opens.

# Evidence

- `cargo nextest run -p nara_fs --locked -j 1 --no-fail-fast --failure-output immediate`:
  83 passed, 3 skipped on Windows. This includes exact nested success, parent/leaf aliases,
  raw Unicode spelling, same-handle identity, strict multi-link and cross-volume/reparse refusal.
- Final `cargo nextest run -p nara_scene -p nara_fs --all-features --locked -j 1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`: 381 passed,
  4 skipped on Windows, run `3f35acfd-8985-416e-82bf-ecea30082b47`.
- `cargo check -p nara_fs --tests --target x86_64-unknown-linux-gnu --locked` passed.
  This checks the Linux implementation and public tests, not native execution.
- WSL Ubuntu-26.04, Linux 6.18.33.2, Cargo 1.97.1, serial offline `cargo nextest run -p nara_fs
  --locked --offline -j 1 --no-fail-fast --failure-output immediate`: 89 passed, 0 skipped.
  The final run `0526c880-3060-4f2f-bdcf-cc7dfce32b08` includes a deterministic entry-replacement
  identity regression, exact scan-limit/absence distinction and actual three-descriptor observation.
  Native fixtures use `/tmp`; the checkout and shared target remain on `/mnt/f`. No casefold
  filesystem was mounted or claimed. WSL networking reported a mirrored-mode failure and fell
  back without network; the run used available offline dependencies and changed no networking.
- `cargo fmt --all` passed. The scoped independent review is recorded with the final source
  admission checkpoint; no worker ran Cargo concurrently.

# Follow-up

Casefold-volume execution and measured large-directory cost remain separate qualification.
The final descriptor regression counts only its fixture's handles, avoiding unrelated process
activity. Do not replace the stored-name evidence with a compile-target assumption.

# Citations

- [Filesystem API](../../../../../crates/nara_fs/src/capability.rs)
- [Platform mechanisms and limits](../../../../../crates/nara_fs/README.md#exact-stored-spelling)
- [Public contract tests](../../../../../crates/nara_fs/tests/filesystem_contract.rs)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
