---
type: "Verification Evidence"
title: "CDPF-U2 exact directory publication and terminal outcomes"
description: "Verifies exact Windows no-replace publication, Linux cooperative no-replace compilation, truthful begin/publish failure terminality, and bounded public filesystem authority."
timestamp: 2026-08-14T04:33:54Z
record_id: "b27082ccee9b484bb298d4d00901400c"
resource: "nara-fs-directory-publication"
tags: ["filesystem", "cdpf-u2", "directory-publication", "no-replace", "terminality"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e3a5e7b"
verified_by: "cargo-nextest-doctest-clippy-linux-cross-target-root-check-format-static-review"
---

# Verification

CDPF-U2 is implemented by `eede23a` and its begin-terminality correction `e3a5e7b`. The proving
tree exposes one move-only `DirectoryPublicationTransaction` with exact `publish` and `abort`
terminal operations. `DirectoryCapability::begin_directory_publication` now distinguishes a true
pre-mutation `BeforeStaging` rejection from a created staging object that was either removed exactly
or returned as a recoverable orphan.

The Windows adapter uses the no-replace `FileRenameInformationEx` form against the live staging and
parent handles. The Linux adapter compiles the same-parent `renameat2(RENAME_NOREPLACE)` path under
the explicit `TrustedLocal` and `NameBound` source tier. This Windows session supplies live local
NTFS execution evidence. Linux/ext4 remains unverified until CDPF-U3 runs on a hosted ext4 worker;
cross-target compilation and injected mapping tests do not broaden that claim.

# Result

- `Published` carries exact parent, candidate and published identities plus independent source,
  topology, conflict, atomicity and durability facts.
- `RejectedClean` is emitted only after exact handle-bound empty-directory removal succeeds.
- `Orphaned` retains every available staging authority and observation without inventing a
  capability or identity for a created-but-unopened object.
- Destination collision never overwrites the existing child, and no failure path falls back to
  check-then-rename, recursive deletion or ambient paths.
- The public Interface remains filesystem-only: it contains no project, starter, export, repair or
  package policy.
- CDPF-U2 is complete. CDPF-U3 is the next unit; the Fresh Project product plan remains inactive.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_fs` — 56 passed, 3 skipped. Run ID
  `1ee1b7b4-9b53-40a8-a9d9-9c14814c083e`.
- `CARGO_BUILD_JOBS=1 cargo test -p nara_fs --doc` — 2 compile-fail doctests passed.
- `CARGO_BUILD_JOBS=1 cargo clippy -p nara_fs --all-targets -- -D warnings` — passed.
- `CARGO_BUILD_JOBS=1 cargo check -p nara_fs --target x86_64-unknown-linux-gnu --tests` — passed;
  compile-only evidence, not hosted ext4 evidence.
- `CARGO_BUILD_JOBS=1 cargo check -p nara --all-features` — passed.
- `cargo fmt --package nara_fs` and `git diff --check` — passed on the implementation tree.
- Windows owner-local and public-contract tests exercised live successful publication, collision,
  exact cleanup, identity drift, post-create validation failure and created-but-unopened orphan
  terminality in the current checkout.

# Review

Final static standards/spec review found no unresolved P0 or P1. The review specifically checked
no-replace flags, proof binding, mount/volume drift, post-mutation begin failures, exact cleanup,
receipt identity, orphan authority and the absence of a destructive `Drop` fallback.

# Follow-up

CDPF-U3 must consume only the public `nara_fs` Interface from an external fixture, publish and
reopen a bounded nested tree, prove collision and unproven-parent behavior, audit the public
inventory, and publish the Directory Publication Conformance Baseline. Linux/ext4 remains
explicitly unverified until that hosted evidence exists.

# Citations

- `crates/nara_fs/src/directory_publication.rs`
- `crates/nara_fs/src/capability.rs`
- `crates/nara_fs/src/platform/windows.rs`
- `crates/nara_fs/src/platform/unix.rs`
- `crates/nara_fs/tests/filesystem_contract.rs`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md`
