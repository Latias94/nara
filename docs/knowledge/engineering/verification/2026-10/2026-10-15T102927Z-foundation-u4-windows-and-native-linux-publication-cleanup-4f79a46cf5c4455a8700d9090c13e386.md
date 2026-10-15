---
type: "Verification Evidence"
title: "Foundation U4 Windows and native Linux publication cleanup"
description: "Instance preflight and retained cleanup pass on Windows and native WSL2 Linux; real repair CLI failure exit is verified."
timestamp: 2026-10-15T10:29:27Z
record_id: "4f79a46cf5c4455a8700d9090c13e386"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bc71449"
---

# Verification

U4 checks actual directory capability rights/trust/platform publication support before creating a
candidate. Temporary-file creation validation, write/sync, replacement, and cleanup failures retain
an identity-bound cleanup owner. The owner cannot write/read/publish. Unknown creation identity is
revalidated using the original trust contract before cleanup; Unix does not gain unsafe name unlink.

# Result

Serial commands with CARGO_BUILD_JOBS=1 passed using the existing target:

- Windows: `cargo nextest run -p nara_fs --lib --test filesystem_contract --locked --offline
  --no-fail-fast --test-threads=1`: 59 passed, 3 skipped; run
  `32181f04-8fc3-4c26-85c2-b689647b4414`.
- Native Linux under Ubuntu-26.04 WSL2: the same package/library/contract selection with explicit
  `--target x86_64-unknown-linux-gnu`, 57 passed, no skips. This is execution on the Linux kernel,
  not a Windows cross-target check. Cargo 1.97.1 / Nextest 0.9.115; shared worktree under /mnt/f.
- Windows Host cleanup: six focused publication, retained candidate, Image metadata, Scene save,
  and Prefab lifecycle tests passed, run `dbe4b6ba-d3e8-456c-8f18-9d034ee8dedd`.
- Windows repair CLI and workflow: `cargo nextest run -p nara --bin nara --test
  project_workflow_check --features project-workflow-cli,runtime-ui,tooling --locked --offline
  --no-fail-fast --test-threads=1`: 12 passed, one existing trajectory skip; run
  `4e8b87b9-5b45-49f1-9c2b-7c51c51f6a39`. The real repair terminal handler removes the failed
  replacement candidate and preserves the external target.
- Windows Reference Game: `cargo nextest run --manifest-path reference-game/Cargo.toml --bin
  authoring_bundle --locked --offline --no-fail-fast --test-threads=1`, shared target: 2 passed;
  run `2e978136-1e9c-4ae0-ab1f-a574fb16d76a`. This executes the actual bundle failure-exit
  cleanup and the existing declared-bundle write/check trajectory.

The Linux offline metadata attempt initially lacked parley. Retrying with the authorized local
HTTP proxy populated Cargo's existing cache and completed successfully; no dependency versions
or distribution configuration were changed. No native macOS execution is claimed.

# Evidence

Two old implementations failed the newly written replacement-ownership regressions before changes
(run `6540c024`, full run identifier in the session tool output). New Windows regressions also cover
readonly cleanup retry, creation-facts failure, and renaming/replacing the candidate name without
deleting the unrelated replacement. Native Linux exercises strict preflight before any candidate
effect and explicit unsupported cleanup retaining recoverable ownership.

Host tests verify one retained candidate blocks another allocation and blocks session Complete until
cleanup succeeds. Image metadata transfers its local publisher into the long-lived Host on failure.
Scene/Prefab document replacement retains the lane while cleanup is incomplete. One-shot commands
explicitly attempt cleanup at their terminal error handler; failed cleanup prints residual identity/name.

Independent read-only reviewers `/root/mods_plugins` and `/root/toolchain_positioning` found no
remaining U4 blocker after the CLI terminal ownership omission was corrected. No extra platform
deletion implementation, validator framework, or retry policy was introduced.

# Follow-up

U4 is implemented and Windows/native Linux contracts and both CLI failure exits are verified.
U5 is changing the Editor's task closure path; a broader
Host run under that new path still exposes normal-drop/quarantine lifecycle work and is not recorded
as passing here. Keep U4 files that overlap pre-existing SPL filesystem/tooling edits uncommitted
until their own hunks can be isolated without staging another author's changes. Full U1-U13 review
and consumer closure remain outstanding.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U4
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- crates/nara_fs/src/capability.rs
- crates/nara_fs/src/replace.rs
- src/project_host/persistence/file_publication.rs
- src/project_host/runtime/editor/image_import/transaction.rs
- src/bin/nara/main.rs
