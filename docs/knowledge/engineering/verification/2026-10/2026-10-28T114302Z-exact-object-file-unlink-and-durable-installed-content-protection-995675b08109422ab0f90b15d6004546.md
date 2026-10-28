---
type: "Verification Evidence"
title: "Exact-object file unlink and durable installed-content protection"
description: "Qualifies generic Windows unlink separately from bounded Session management/adoption, explicit protection repair and real managed/public/native consumers; complete reclamation remains open."
timestamp: 2026-10-28T11:43:02Z
record_id: "995675b08109422ab0f90b15d6004546"
producer_id: "codex-root"
run_id: "protected-source-tool-content-2026-10-28"
related_plan: "docs/plans/2026-10-28-protected-source-tool-content-plan.md"
git_branch: "feat/protected-source-tool-content"
verified_by: "01a1823f-6f4c-7992-b63a-694b869b65eb"
---

# Verification

Continue the full active foundation Goal from `main = cd760d6`. This successor qualifies bounded
installed-content management/adoption and explicit generic unlink, not the complete reclamation
workflow. Windows 11 build 26300, Rust `+1.95.0`, native local NTFS (temporary C: and checkout F:),
serial Cargo, one job and the existing root `target`. No push/PR or vendor changes. The containing
commit binds this source state. The earlier
[integration record](../../progress/2026-10/2026-10-28T112021Z-protected-installed-content-integration-and-explicit-unlink-authority-correction-10774d33b8b844fcb33ed785e034d011.md)
retains the initial failure and the authority correction.

# Result

The concrete Session owns a separate `.nara/source-tools/content.json` record, bounded to 2 MiB,
32 installations and 66 declared members each. Withdrawn installations retain protection. Native
IDs and executable approval never enter this record. Management consumes actual live installation
custody through an Arc, checks the project-relative directory and observes finite members without
requiring an unchanged whole executable release. Repeated registration cannot rebind immutable
facts or reset flags. Explicit Content adoption and observed modification are independent sticky
facts; baseline-equal bytes cannot clear them. Unknown siblings are untouched.

Read-only preparation produces one move-only original-basis successor with exact encoded bytes
and the complete live-creation-custody projection. Commit rechecks basis/project association and
publishes through existing Tasks/FilePublicationHost custody. Source editing, inverse/history,
selection, graph relationships, metadata adoption and process execution retain their owners.
Physical task return and cleanup are observed separately from a terminal publication. Session
Close waits for the actual IO task. Fresh Open reads protection/recovery data without recovering
created-object provenance, enabling code or deleting files.

Reconciliation preserves stronger admitted protection and its current disk Host. A rolled-back
record reports NeedsProtectionPublication/RequiresReconcile until an explicit prepared successor
republishes the retained facts; an in-memory OR is not durable success. Corrupt/unavailable records
do not become empty overwrite candidates. Actual fixed metadata objects and case variants remain
reserved independently of parsing/Host admission. The same content view enters ordinary frame
change/pending observations and Application forwarding; no parallel result bus or executor exists.

`nara_fs::DirectoryCapability::unlink_file(&path, &observed)` replaces the bare-ID Unsupported
placeholder with concrete explicitly authorized exact-object deletion. A writable parent supplies
authority; the borrowed original and current deleting handles supply actual object continuity.
Strict traversal and live identity/kind/link validation precede same-handle POSIX disposition.
Different same-name objects refuse. The Windows x86-64/1809+/local-NTFS implementation requests
attributes plus DELETE, permits read/write sharing and excludes delete/rename sharing. Read-only
attributes are honored. Its receipt distinguishes name removal, retained old readers, unsupported
parent-directory sync and physical storage release. Other adapters preserve with Unsupported.

Generic unlink authorizes the complete file, including alternate streams; it claims no unchanged
bytes or installation ownership. Private actual Windows probes demonstrate that even stronger
default-stream read/delete custody permits alternate-stream creation/modification. That prevents
claiming safe automatic whole-file reclamation from a default-stream digest, not providing the
independently useful generic primitive. FileCapability::same_object supplies fresh live comparison
across independent capability sessions without merging rights, generations or content authority.

# Evidence

- Final Fs library/public filesystem contract: **98 passed, 3 explicit privileged skips**, run
  `de4c97e0-5b5d-4ae5-9eff-0a508e70cfdf`; build 3.34 s/run 2.342 s. Actual unlink covers a retained
  original reader, different same-name replacement, read-only capability/attribute refusal,
  conflicting sharing, hardlinks and whole-object deletion with an allowed writer still alive.
  The writer can continue modifying the old unlinked object without changing the name receipt.
  Private probes cover writable views after file/section handle closure, ordinary sharing,
  readonly and the ADS counterexamples. The three skipped tests require live symlink/junction
  privileges; this is not their execution evidence.
- Focused production content: **30 passed**, run `da8807a9-15fd-4e32-bbbd-06ec1df735df`;
  build 1 m 01 s/run 6.000 s. Parser/model/store tests and actual Session candidates, adoption,
  modification/restore, stale basis, corrupt/missing/rollback recovery, Save/physical Close/Open,
  missing-Host aliases and held IO run together. The initial directory case failed because Windows
  erased STATUS_FILE_IS_A_DIRECTORY into Win32 AccessDenied. The native adapter now preserves
  FsError::NotRegularFile; its direct public contract passes without weakening the store assertion.
- Complete root/Tooling all-feature library nextest: **887 passed, 4 existing skipped**, run
  `e001eda7-c5a6-480d-af9e-a47c9b36fcbe`; build 6.87 s/run 276.068 s.
- Complete Editor all-feature library nextest: **246 passed, 34 explicitly skipped**, run
  `f8dd3bbf-c894-4872-bd9a-1da23849ab41`; build 1 m 59 s/run 43.617 s. Managed/native prerequisite
  cases are separately selected below; default skips are not inferred successes.
- `python tests/fixtures/behavior-languages/run.py --native-editor` passes with actual .NET builds
  and captured-image execution: **29 independent language/tool consumers**, run
  `de6347c4-046f-4240-9e87-e1b65f343497`; build 49.81 s/run 30.961 s. The new public content test
  passes in 1.163 s and joins actual same-release installations, live custody, baseline adoption,
  observed modification/restore, an unknown sibling, real source proposal/review/acceptance,
  Undo/Redo/Save, graph withdrawal and fresh Open. Saved source/protection persists; per-session
  history is not claimed to be serialized. No fixture-owned executor or ledger reader substitutes
  for production behavior.
- The same runner passes actual managed cancellation, run `361177c9-6a21-4a98-b4e0-38eec82baee9`,
  0.308 s; actual C# Application, `db50a8df-262e-4224-b90d-f12b0f638b34`, 1.738 s; and selected
  Windows/DX12 native source-tool/Inspector/panel/history/persistence,
  `3b719dc9-3e13-41e1-86f1-b957647b252c`, 5.859 s. This retains the incumbent frontend's selected
  behavior; no new graphical installer or content-management controls are claimed.
- Root minimal `--no-default-features --features runtime-2d,serde,tooling` library check passes,
  8.12 s. Fs/root/Tooling/Editor all-feature/all-target Clippy with `-D clippy::correctness`
  passes in 1 m 58 s. Non-correctness warnings remain, including private content IO result-size
  and argument-count suggestions; this is not an all-warnings-clean claim. Root and independent
  fixture formatting pass.
- Initial independent parent review found three P2s (durable rollback, pre-admission aliases,
  missing frame pending) and preview inconsistency; their production fixes and adversarial tests
  are included above. Final independent review `01a1823f-6f4c-7992-b63a-694b869b65eb` found an
  incorrect whole-view comparison in the new C# test. It now updates only explicitly expected
  content while comparing all execution/review/selection fields; the real test passes. Its
  targeted final review reports no remaining confirmed P1/P2 in the bounded content scope.
  Independent native review `01a18200-dcc9-7572-a2ea-9d0666e5de2b` reports no confirmed P1/P2
  in generic unlink, separately from stronger reclamation. Source reviews are not new executed
  platform evidence. All eight subagents are completed and closed.

# Follow-up

Keep the full foundation Goal active. Complete document-edit protection at its owning mutation
boundaries, fresh explicit reclamation review after graph withdrawal and actual physical return,
pre-deletion intent publication, interrupted/partial result recovery, and the stronger whole-file
content proof. No production content reclamation is exposed by this unit. Generic unlink, graph
withdrawal, an equal digest or a receipt never supplies those policy decisions automatically.

Preparation currently performs bounded synchronous code-free IO against the Session. A full
owned handoff/locality and interactive-latency qualification remains separate; byte-work bounds
are not RSS, OS latency or allocator isolation. Linux/ReFS unlink execution, native Linux/macOS,
the official language SDK, gameplay publication/migration, source-free resource delivery,
composable rendering and Nara-owned custom-drawn UI remain outside this qualification.

# Citations

- [Protected content plan](../../../../plans/2026-10-28-protected-source-tool-content-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Filesystem ADR](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Session ADR](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [Editor ADR](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Guide](../../../../guides/finite-source-tools.md)
- [Content owner](../../../../../src/project_host/runtime/editor/source_tools/content.rs)
- [Private record](../../../../../src/project_host/runtime/editor/source_tools/content/record.rs)
- [Public unlink contract](../../../../../crates/nara_fs/tests/filesystem_contract.rs)
- [Independent consumer](../../../../../tests/fixtures/behavior-languages/tests/support/installed_graph.rs)
