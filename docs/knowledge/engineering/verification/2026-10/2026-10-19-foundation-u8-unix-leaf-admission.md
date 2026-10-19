---
type: Verification Evidence
title: Foundation U8 Unix FIFO leaf admission
description: Nonblocking leaf open rejects FIFOs through the same handle in both Linux trust modes.
timestamp: 2026-10-19T08:40:38Z
record_id: 1f72e8907c9f42af8e0277032080f824
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Result

Unix `platform::open_file` uses `O_NONBLOCK` with the existing read-only, no-follow and close-on-exec
flags. Both ordinary `openat` and strict Linux `openat2` receive those flags. The capability owner
then inspects and rejects non-regular types on that same opened handle. No path-based stat/open
precheck, timeout wrapper in production, or consumer-specific workaround was introduced.

The Linux regression spawns a child process for each TrustedLocal/Untrusted and relative/direct-child
combination. A FIFO has no writer. The child must return `NotRegularFile`, still read a regular file,
and reject a symlink. A parent deadline kills and joins a blocked child, so a regression does not
hang the test process. The test is Linux-only because other Unix strict-resolution proof is not
admitted by this adapter.

# Native Evidence

Environment: Ubuntu-26.04 on WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`, x86-64, Rust 1.95.0,
nextest 0.9.115. Tests created their live filesystem fixtures under Linux `/tmp`, not on DrvFS.
All Cargo commands were serial, used `CARGO_BUILD_JOBS=1`, and reused the repository target tree.

| Run | Result |
| --- | --- |
| `4615806f-80ae-4d87-943f-5d5b0a3b9bcd` before the flag change | Failed as expected: TrustedLocal relative FIFO open waited beyond the parent's 5-second deadline. |
| `4f5a761a-c0c8-4c54-8532-99b36ad93fdc` after the change | All 64 filesystem library, filesystem-contract and path-contract tests passed. The FIFO parent exercised all four combinations, including strict `openat2`. |

An independent static review found no actionable correctness issue. The existing CI already runs
the `nara_fs` package in its Linux workspace suites; the new regression is discovered there without
an extra process or platform workflow.

The locked Windows crate cohort also passed after formatting: run
`63f38e0a-65fb-49c4-a8c6-df65e95fb6e7`, 64 passed and three platform-dependent tests skipped.
This guards the shared capability surface; it does not substitute for the Linux FIFO trajectory.

# Verification Boundary

WSL networking fell back to None, and its offline registry lacked the unrelated `dear-imgui-wgpu`
entry required to resolve the whole workspace. The native run therefore used a temporary standalone
manifest pointing directly to the current `nara_fs` source and both test files, with its same direct
dependency versions (`blake3 1.8.5`, `thiserror 2.0.18`, `rustix 1.1.4` with `fs`). No copied source
or mocked Unix implementation was tested. Offline resolution chose cached transitive dependencies;
this is native crate behavior evidence, not the root locked-workspace Linux qualification.

The temporary harness was an execution artifact under `target/foundation_u8_native.py`; it adds no
production script or alternate build system. On a fully provisioned Linux host the owning command
is `cargo +1.95.0 nextest run -p nara_fs --locked --test-threads=1`.
`O_NONBLOCK` addresses FIFO peer waiting; it is not a universal deadline for arbitrary filesystem
or device-driver operations. Other Unix platforms retain their existing unqualified scope.
