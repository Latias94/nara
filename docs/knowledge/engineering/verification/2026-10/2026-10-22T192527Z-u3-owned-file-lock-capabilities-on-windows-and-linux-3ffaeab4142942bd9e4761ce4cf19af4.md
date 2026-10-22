---
type: "Verification Evidence"
title: "U3 owned file lock capabilities on Windows and Linux"
description: "Unique release owner and borrowed positional readers pass native FS suites plus root persistence and repair consumers."
timestamp: 2026-10-22T19:25:27Z
record_id: "3ffaeab4142942bd9e4761ce4cf19af4"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "6ca126f"
---

# Verification

Windows 11 / Rust 1.99 and Ubuntu-26.04 WSL2 / Rust 1.97.1. All Cargo runs are serial with
`CARGO_BUILD_JOBS=1`. Linux uses the existing root target with its native artifact separation.
Linux temp-root lock tests execute on the WSL filesystem; this is not a remote-filesystem claim.

# Result

File acquisition consumes the unlocked capability and returns one non-cloneable locked owner.
Rejection retains the original file; failed explicit release retains the locked owner for retry.
Failure-only boxing keeps these ownership-returning errors small. Readers borrow the exact file
with independent positional cursors; they do not duplicate open descriptions. Imported or exported
native handle authority cannot subsequently create another Rust lock owner. Native Windows/Unix
guarantee classifications are unchanged. Generation markers store the owner, removing forgotten guards.

# Evidence

- Windows `cargo nextest run --locked -p nara_fs --test-threads=1`: 71/71 pass, three existing
  platform skips, run `3f4d50b9-057e-4528-9edf-4efa01cf7cd6`. Includes acquisition/release/Drop,
  retained release failure, shared contention, native aliases, reader cursors and publication.
- `cargo test --locked -p nara_fs --doc`: 7/7 compile-fail tests pass, including no second lock,
  clone/export or release while a reader remains live. Strict all-target Clippy passes without
  exemptions after boxing failure-owned capabilities and removing redundant must_use annotations.
- Linux command through `wsl_run.py`, named Ubuntu-26.04, native `cargo nextest run --locked
  -p nara_fs --test-threads=1`: 77/77 pass in `2015aa28-72ca-42ed-a1fd-fb1b6eb9a641`. The
  `original_same_owner_dual_guard_primitive_releases_both_logical_locks` probe reproduces the
  underlying native mechanism; the new API makes that owner duplication unavailable. Reimport,
  contention, release and read behavior pass. An initial offline resolution lacked dear-imgui-rs;
  the same locked command with index access populated missing packages and then passed.
- Root persistence/Scene/Browser preparation cohort: 32/32 pass (204 unrelated filtered), selected
  library command with `tooling,presentation-2d,serde,project-workflow`. Publication locks before
  revalidating digest/identity, and failed-write/reopen semantics pass.
- Root Check/content/Workspace Play: 43/43 pass in `90026916-6653-44ba-8dfd-4fc941f0c303`, including
  stale repair refusal and one explicit startup-scene publication.
- Independent lock review found no blocker after checking pre-export/reimport aliases, success
  Drop behavior, boxed error transfers, retained generation markers and migrated exports.

# Follow-up

Editor generation/native Save and two-level save consumer qualification remain part of U11.
Release failure injection proves rejection before native unlock, not recoverability of every OS error.
No lock upgrade, reentrancy, stronger compare-and-swap, remote FS or media durability guarantee is added.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Lock owner](../../../../../crates/nara_fs/src/lock.rs)
- [File and readers](../../../../../crates/nara_fs/src/file.rs)
- [Filesystem regressions](../../../../../crates/nara_fs/tests/filesystem_contract.rs)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
