---
type: "Verification Evidence"
title: "CDPF-U1 capability-owned directory staging"
description: "Verifies move-only same-parent staging, explicit file write authority, runtime filesystem-tier admission, and fail-closed pre-publication behavior."
timestamp: 2026-08-14T02:43:37Z
record_id: "44a2fdb822d94c268972bf50f7fa7168"
tags: ["filesystem", "cdpf-u1", "directory-publication", "capability", "staging"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e345687"
verified_by: "cargo-nextest-doctest-clippy-cross-target-check-root-check-format-static-review"
---

# Verification

Commit `e345687` implements CDPF-U1 without adding project, template, export, recovery or recursive
file-manager semantics to `nara_fs`. One read-write parent capability begins a move-only
`DirectoryPublicationTransaction`, retains the live parent and staging identities, and exposes only
a shared child capability for filling a known finite staging tree.

# Result

- `ReplaceSourceBinding` is removed in favor of the domain-correct
  `PublicationSourceBinding`; no compatibility alias remains.
- Direct-child directory and regular-file creation are exclusive and capability-relative. Files
  opened from existing content remain read-only; only newly created files expose borrowed write
  authority through `CapabilityWriter`.
- The transaction retains the parent session/generation, live staging identity, destination and
  exact source-binding tier without exposing a native path or terminal publication authority.
- Windows admission requires x86-64 Windows 10 version 1809 or later and a runtime-proven local
  NTFS parent before staging. Linux admission requires x86-64, strict `openat2` resolution, a
  runtime-proven ext4 mount and `TrustedLocal`; its source binding remains explicitly `NameBound`.
- Unsupported or unproven combinations reject before staging. Invalid post-create directory facts
  report a recoverable exclusive-child orphan instead of claiming rollback.
- Existing replacement and persistence consumers compile against the renamed proof type, and new
  filesystem operations have stable root diagnostic identifiers.

# Evidence

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1` and reused the workspace target.

- `cargo nextest run -p nara_fs` — **41/41 passed**, with 3 platform-inapplicable tests skipped.
- `cargo test -p nara_fs --doc` — **1/1 compile-fail doctest passed**, proving the transaction is
  not cloneable by a public consumer.
- `cargo clippy -p nara_fs --all-targets -- -D warnings` — **passed**.
- `cargo check -p nara_fs --target x86_64-unknown-linux-gnu` — **passed**. This is a cross-target
  compile gate only; it does not claim a hosted ext4 conformance result from this Windows session.
- `cargo check -p nara --all-features` — **passed**.
- `cargo fmt --all -- --check` and `git diff --check` — **passed**.

The focused tests cover success, read-only authority, destination/staging conflict, existing
destination, exclusive staging collision, unsupported and unproven parents, cooperative
name-bound admission, wrong object kind, reparse facts, cross-volume facts, explicit file-write
rights and capability lineage. A broad root all-features nextest run is not cited: an earlier
attempt timed out during linking, so closure relies on the focused behavioral gates and the root
all-features type gate above.

# Follow-up

CDPF-U2 is active. It must consume the staging transaction into exact no-replace publication and
truthful `Published`, `RejectedClean` or `Orphaned` terminal outcomes. Linux/ext4 hosted proof and
the external public consumer remain CDPF-U3 responsibilities. The Fresh Project product plan stays
inactive until the CDPF conformance baseline is published and a later registration activates it.

# Citations

- `docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `crates/nara_fs/src/directory_publication.rs`
- `crates/nara_fs/src/capability.rs`
- `crates/nara_fs/src/file.rs`
- `crates/nara_fs/src/platform/windows.rs`
- `crates/nara_fs/src/platform/unix.rs`
- `crates/nara_fs/tests/filesystem_contract.rs`
