---
type: "Verification Evidence"
title: "SPL-U1 private supervised process substrate verification"
description: "Verifies the private Windows process-tree owner, bounded cancellable output, spawn authority allowlist, cleanup receipt, and the supported Linux compile boundary."
timestamp: 2026-08-21T09:01:32Z
record_id: "9b5f75ac859c466ba7f94deb74851778"
status: "verified"
producer_id: "codex-root"
run_id: "fc9d45c8-4ec6-4be2-8449-82aa8f7012c5"
related_plan: "docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "07b590e"
verified_by: "codex-root"
---

# Scope

This record verifies the SPL-U1 process and log substrate only. It does not claim Cargo artifact
selection, immutable generation publication, child READY admission, Editor Play lifecycle wiring,
or a native Reference Game trajectory; those remain owned by SPL-U2 through SPL-U5.

# Evidence

The unit established:

- one private `SupervisedProcessOwner` with explicit running, terminating, force-killing,
  draining, and complete states;
- a terminal cleanup receipt requiring direct-child reap, empty containment, and stopped output
  readers before the owner becomes complete;
- Windows pre-execution Job and inherited-handle allowlists through `STARTUPINFOEXW`, kill-on-close
  containment, overlapped output pipes, and cancellable reads;
- the supported Linux x86_64 process-group, close-on-exec allowlist, nonblocking output, TERM/KILL,
  and direct-child reap implementation, with other Unix targets remaining fail-closed;
- bounded memory-only stdout/stderr retention with explicit loss/truncation facts and a raw-output
  observer for the later Cargo JSON owner;
- a private bootstrap endpoint and a sentinel authority canary proving that unrelated inherited
  file authority does not cross the spawn boundary.

# Commands

All Cargo commands were run serially in PowerShell. No Bash, WSL, or parallel Cargo invocation was
used.

1. `cargo check --offline -p nara-editor --lib`
   - Result: passed.
2. `cargo nextest run --offline -p nara-editor --lib -E 'test(supervised_play::)' --test-threads=1`
   - Result: 18/18 selected Windows tests passed; 13 target-gated tests were skipped.
3. `cargo check --target x86_64-unknown-linux-gnu -p nara-editor --lib`
   - Result: passed after Cargo fetched the missing Linux target dependencies.
4. The sentinel inheritance test was repeated five consecutive times after replacing a raw-handle
   number comparison with a disk-handle object-kind proof.
   - Result: 5/5 passed.
5. `cargo fmt --all` and `git diff --check`
   - Result: passed; Git emitted only existing LF-to-CRLF checkout notices.

# Platform boundary

Windows has native runtime process-tree evidence in this checkout. Linux x86_64 has compile-time
evidence only in this record because the verification host is Windows. The required supported-Unix
native process-group trajectory remains an SPL-U5 release gate and must not be inferred from the
cross-target check.

# Residual scope

SPL-U2 must consume the raw-output/loss seam to parse bounded Cargo JSON, publish an immutable
executable generation, and make cancellation terminal only after this owner's cleanup receipt.
The private owner is not yet connected to ordinary Editor Play.
