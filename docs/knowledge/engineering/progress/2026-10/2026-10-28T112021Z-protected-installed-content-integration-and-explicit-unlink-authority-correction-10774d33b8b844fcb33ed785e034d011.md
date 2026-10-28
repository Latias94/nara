---
type: "Work Progress"
title: "Protected installed content integration and explicit unlink authority correction"
description: "Records native ADS counterexamples, concrete exact-object unlink, ongoing content protection integration and review corrections without claiming complete reclamation."
timestamp: 2026-10-28T11:20:21Z
record_id: "10774d33b8b844fcb33ed785e034d011"
producer_id: "codex-root"
run_id: "protected-source-tool-content-2026-10-28"
related_plan: "docs/plans/2026-10-28-protected-source-tool-content-plan.md"
git_branch: "feat/protected-source-tool-content"
---

# Summary

Continue the full active foundation Goal from local `main = cd760d6`; do not reduce it to this
content unit. The clean starting checkout contained only the original untracked `vendor/`.
All current other edits belong to this authorized branch. No push/PR, no new build directory,
serial Cargo with one job/root target. Windows 11 build 26300, Rust `+1.95.0`, actual test temp
volume C: NTFS and checkout F: NTFS. The full Goal remains active.

# Details

The successor plan separates installed selection from a bounded management/protection ledger.
Actual created installation Arcs transfer live member custody; fresh Open only reads data facts.
Manage, explicit Content adoption and finite member observations prepare without writes, then
confirm the original basis through existing Tasks/FilePublicationHost custody. Modified/adopted
facts are sticky; unknown siblings are untouched. No production content-removal path is exposed.
This is not a second process owner, package resolver, document store or schema interpreter.

An actual Python ctypes probe and private Rust Windows probes show that a restrictive default
stream read/DELETE handle excludes ordinary writers and a retained writable view after original
file/section handles close, but still permits creating and modifying named alternate streams.
Default-stream hash equality cannot prove the complete file has no new user data. Oplock or
quarantine alternatives are research, not qualified implementation.

The first response incorrectly tied the generic unlink axis to that stronger reclamation proof
and removed its unused Unsupported stub. The user explicitly challenged whether unlink was still
needed. Correct the layer boundary: `DirectoryCapability::unlink_file(&RelativePath,
&FileCapability)` now performs concrete explicitly authorized exact-object deletion. The writable
parent grants authority over the complete object, including streams; live expected/current handles
prevent same-name replacement and recycled-ID confusion. Generic unlink promises no unchanged
bytes, installation ownership or adoption policy. Windows x86-64/1809+/local NTFS uses attributes
plus DELETE, read/write sharing without delete sharing, same-handle POSIX disposition, and no
read-only override. Unix/other adapters preserve with explicit Unsupported. The receipt separates
namespace removal, retained old readers, unsupported parent sync and physical storage release.

Native qualification at this source: Fs library plus filesystem contract **98 passed, 3 explicit
privileged cases skipped**, run `1fe79abe-f43a-4ba9-9855-84d80b9548fc`; build 3.13 s/run 2.557 s.
This includes public actual unlink, stale observed object versus replacement, rights/writer/
read-only/hardlink refusal, live file comparison and the ADS counterexamples. A preceding
97-test cohort also passed; do not count it as independent platform coverage.

First root content-focused run `229c8541-5828-4158-80b4-868150da67b1`: **27 passed, 1 failed**.
Windows had collapsed `STATUS_FILE_IS_A_DIRECTORY` into Win32 AccessDenied, so the store observed
Write instead of the known-kind Changed refusal. Fix the owning NT adapter to preserve
`FsError::NotRegularFile`; its new direct native contract passes in the 98-test cohort. Keep the
store test intact. Root/Editor all-feature library checks passed earlier, before the later review
corrections; they do not qualify the final source.

Independent parent-integration review found three P2s: rollback protection restored only in memory;
reserved metadata aliases admitted before a decoded Host existed; content tasks absent from frame
pending/change observations. Corrections expose NeedsProtectionPublication/RequiresReconcile until
explicit repaired publication, reserve current metadata objects independently of decoding, and
project the same content view into existing frame polling. Prepared previews now retain complete
creation custody and exact encoded bytes. These need their final executed tests and re-review.
Actual held-IO Close and corrupt-record alias regressions are included in the root content tests.

Workers' write scopes: record/store and tests; content correction and Tooling content model;
Session tests; independent real C# consumer; native Fs implementation/probes. Completed native,
Session-test, frame/alias and independent-consumer workers are closed. At capture, remaining
record/store/content writers have been told to freeze; the native researcher is reviewing generic
unlink separately from all-stream reclamation. No worker runs Cargo or fmt. Root and independent
fixture fmt completed after inspecting the intended diff.

# Next Action

The final focused root content run is live as exec session `61818`, log
`%TEMP%/nara-protected-content-focused.log`; inspect that same handle, do not restart on observation
timeout. Then serial root/Tooling and Editor suites, real language consumer runner with selected
native case, minimal features, correctness Clippy and final diff/review. Record final evidence,
render memory, precise local commits and fast-forward main only after qualification. No commit
or merge from this content branch has occurred at capture.

Complete edited-document protection, fresh explicitly authorized reviewed reclamation after
physical return, interrupted-deletion recovery, and the stronger native all-stream proof remain
open. Generic unlink does not close them. Full foundation resources/delivery/rendering/gameplay
publication and Nara-owned custom drawing are unchanged outstanding work.

# Citations

- [Content successor](../../../../plans/2026-10-28-protected-source-tool-content-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Fs authority](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Session content owner](../../../../../src/project_host/runtime/editor/source_tools/content.rs)
- [Native probes](../../../../../crates/nara_fs/src/platform/windows/file_removal_tests.rs)
- [Generic unlink](../../../../../crates/nara_fs/src/capability.rs)
- [Actual independent consumer](../../../../../tests/fixtures/behavior-languages/tests/support/installed_graph.rs)
