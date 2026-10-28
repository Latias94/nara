---
title: Protected installed content and explicit source-tool reclamation
date: 2026-10-28
status: in-progress
related_plan: 2026-10-28-local-source-tool-release-plan.md
---

# Protected Installed Content And Explicit Reclamation

## Complete Consumer

Continue the same independently built EnemyStats tool from exact installed graph withdrawal to
content protection, explicit adoption and reviewed file reclamation. Modify one shipped sample,
adopt another, leave an unknown sibling, and retain an unchanged managed member. The modified,
adopted and unknown files survive. A disabled dependent still blocks ordinary withdrawal. A held
child or publication prevents reclamation even after the graph reports Withdrawn. Save and actual
Close retain source/protection publication facts; fresh Open rereads source and management/recovery
data without recovering prior-session history. Open never starts code or resumes deletion. This
is the successor to the qualified installed-graph unit, not a new package resolver, executor or
document store.

## Authority And Durable Facts

An installation receipt proves a selected artifact observation. It does not prove that the current
file object was created by the installer or that its contents remain unchanged. Installation
publication already retains continuously live created-member handles. Preserve that stronger
evidence until it is explicitly relinquished; never reconstruct it from a serialized native ID,
an equal receipt, a file name or an equal digest after Close.

Bounded management records retain the installation, declared member baseline, known protection
and interrupted reclamation facts. They cannot authorize deletion. Observed modification and
explicit adoption are sticky: later baseline-equal bytes do not clear them. An imported project
copy is project-owned and is never followed through its release origin. Unknown siblings and
directories remain outside reclamation. Do not claim knowledge of changes that happened and were
reverted while no observation existed.

Fresh Open can read management facts without recovering historical creation custody. If removal
after that boundary is offered, a fresh move-only review must expose that missing provenance and
the exact currently held candidate objects. Its explicit confirmation grants new, bounded removal
authority; the record does not grant it. Corrupt, missing or ambiguous management data preserves
files. A dirty or otherwise open document-owned source is protected before disk equality can make
it a removal candidate. Source/history arbitration remains with the existing document owners.

Graph withdrawal and file reclamation have independent outcomes. Reclamation requires the selected
installation to be absent from the admitted graph and all relevant child, pipe, selection IO,
deferred promotion and publication-cleanup obligations to have physically returned. The first
implementation may conservatively fence the whole finite tool lane. It must not treat route
withdrawal, cancellation request or a task result as physical return.

## Accepted Source-Edit Contract And Qualification

ADRs 0047/0118 own this contract. The Session captures the exact
bound project-relative source and owner content identity at five actual acceptance seams:
Workspace commands, prepared edit commit, prepared construction commit, final Prefab-conversion
publication and extension actions. An accepted content change immediately marks matching managed
members `modified`. Undo/Redo that changes content participates; no-op, refusal, query, navigation,
preview and reclassification do not. Undo, baseline restoration, Save file replacement, Reopen
and Discard cannot clear protection. Existing persistence lanes retain private accepted-touch
facts through initial asynchronous reads and new management without another document store/history.
ASCII-case path matching is conservative protection, not exact-object proof or imported-origin
ownership.

Explicit management/adoption/observation confirmation admits known facts before IO, retaining them
on failure. Additional protection invalidates the content basis without changing immutable pending bytes;
automatic successors coalesce later facts. An accepted publication settles only facts its bytes
cover. Reads retain their start-basis protection: actual weaker disk requires explicit
reconcile/republication, while an older in-flight read missing only newer document facts can
automatically publish their successor. Session/Application
`prepare_source_tool_content_protection(&basis)` prepares the exact already-known repair after
reconciliation; an accepted explicit repair publication reports `Protected`. Automatic metadata
work must preserve the explicit `content.operation` outcome. Protection publication debt and
physical work are separate observations.
Keep bounded provenance for confirmed registrations not yet admitted by disk evidence.
Their absence after a failed first publication permits explicit repair; disappearance of an
already admitted installation remains a conflict. Once physical Close begins, subsequent polls
must continue that retirement rather than reapply the preparation gate to a retired file Host.

Save, Reopen and new document Close guard the owning source before writes, replacement or release
with `Busy`/`RequiresReconcile`. Whole physical Close advances protection before irreversible
closing or Host/task shutdown, so failure leaves the Session repairable. Application keeps
inspection and original-basis repair available on the retained Session during Close/replacement
waiting, including replacement opening; ordinary authoring stays gated. Already-entered Save IO
keeps its actual receipt; no cross-file transaction is promised. Source acceptance is memory
protection only. Only an accepted file-publication receipt supplies disk evidence; a crash before
publication may lose facts. Windows directory synchronization remains unsupported, without a
power-failure guarantee.

The [source-edit qualification](../knowledge/engineering/verification/2026-10/2026-10-28T132217Z-accepted-document-protection-and-repairable-installed-content-close-qualification-8a03235372c74fcbb4a02f7ee23eaca2.md)
records actual owner, public Application, independent managed and selected native results, with
source-routing review and unexecuted platform/locality scope kept separate. Its qualification
matrix is:

| Boundary | Required qualification |
|---|---|
| Five acceptance seams | Changed content marks immediately; no-op/refusal/query/navigation/preview/reclassification do not |
| Sticky lifetime | Undo/Redo, baseline restore, Save replacement, Reopen and Discard retain protection; imports gain no origin ownership |
| Initial read and management | Accepted touches survive an older read and first registration, including conservative ASCII-case matches |
| Unavailable initial record | Clean Undo retains its unclassified touch; Save/Reopen/Close require a successful read before releasing it |
| Publication overlap | Exact pending bytes stay fixed; later edits invalidate candidates and coalesce; success settles only covered facts |
| Reconcile and failure | Weaker disk refuses automatic repair; explicit `Protected` repair retains facts on failure and preserves explicit operation results |
| Save/Reopen/Close | Owning-source guards precede effects; failed whole Close remains repairable; entered Save receipts remain honest |
| Public and native consumers | Session/Application inspection and original-basis repair during Close/replacement waiting, including replacement opening; relevant existing native workflows |

## Filesystem Contract

Generic explicitly authorized exact-object unlink already landed at `2f032e7`, replacing the
unsupported identity-only entry point. A writable directory capability supplies mutation
authority. Strict relative traversal and the original live observation bind a freshly opened
deleting handle to the current leaf. Identity mismatch refuses before mutation; the successful
operation consumes that same deleting handle. It supplies no installation ownership, unchanged
content or user adoption policy. Authorization includes the complete file and its alternate
streams. Keep this useful low-level primitive independent of the stronger installer policy.

The generic Windows x86-64/1809+/local-NTFS adapter uses attributes plus DELETE access, permits
read/write sharing, excludes competing delete/rename opens and uses handle-bound POSIX disposition.
Reject hardlinks, reparse points, unavailable identity, read-only authority and different live
objects. Do not ignore a file's read-only attribute. A receipt reports the native namespace
disposition and actual guarantee, not immediate physical storage reclamation, a permanently absent
path or crash-durable parent synchronization. Unix/other unsupported adapters preserve the object
with an explicit Unsupported result; no stat/name/unlink fallback is permitted.

The first actual local NTFS probe exposes an additional requirement: a restrictive default-stream
read/delete handle still permits creating a named alternate data stream. Sharing and hashing the
default stream do not protect all user bytes removed by whole-file disposition. Qualify existing
writable mappings whose original writer handles have closed separately; byte-range locks do not
exclude mapped writes. Do not qualify generic unlink or a default-stream check as the planned
protected-content reclamation. Continue management/adoption independently while the complete
reclamation contract is investigated.

## Ordered Implementation And Evidence

1. Reuse the qualified generic exact-object unlink and ADR 0070 contract. Keep the unresolved
   all-stream safe reclamation proof separate; no default-stream check closes that requirement.
2. Extend bounded management/protection and same-original-basis review with the adopted
   source-edit contract above, then qualify its matrix through concrete Session/Application
   consumers. Reuse Tasks and FilePublicationHost custody, with no second source validator or
   execution owner. Preserve the predecessor's project association, cancellation without writes,
   namespace-drift and original-basis refusal contracts.
3. Before the first deletion, publish the reviewed pending/protection facts. Retain actual partial
   results if a later member or record publication fails; do not report rollback. Persisted Pending
   or a missing file after restart is not invented evidence of successful deletion. Fresh Open
   reports recovery and requires another explicit review.
4. Join the real managed tool, graph withdrawal, held physical work, modified/adopted samples,
   unknown files, dirty source and Save/Close/Open through public Session and Application APIs.
   Independent review must challenge creation provenance, metadata tampering, equal-byte object
   replacement and protection loss. Run affected Cargo checks serially with one job/root target,
   independent consumers and the relevant selected native workflow.
5. Record the actual source/platform scope, amend owning ADRs/guides and create precise local
   commits. Keep the full foundation Goal active. Existing-owner Schema migration, executable
   gameplay publications, resources/delivery/rendering and Nara-owned UI remain separate work.

## Qualified Predecessor And Remaining Scope

The [unlink/content verification](../knowledge/engineering/verification/2026-10/2026-10-28T114302Z-exact-object-file-unlink-and-durable-installed-content-protection-995675b08109422ab0f90b15d6004546.md)
qualifies explicit generic unlink and bounded management/adoption/observation, original-basis
protection repair, actual held-task Close and public real managed consumers. The same content
facts enter ordinary frame pending/change observations. The source-edit successor above adds
accepted-change protection and repairable persistence/Close within its linked evidence.
Reviewed reclamation/partial recovery, all-stream content proof, preparation locality and
other platform qualification remain open. No production reclaimer or new graphical installer
controls are claimed. This plan and the full foundation Goal remain in progress.

## References

- [Local lifecycle](2026-10-28-local-source-tool-release-plan.md)
- [Qualified installed graph](2026-10-28-installed-source-tool-graph-plan.md)
- [Foundation plan](2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Filesystem authority](../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Session ownership](../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [Editor/tool lifecycle](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Microsoft CreateFileW sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
