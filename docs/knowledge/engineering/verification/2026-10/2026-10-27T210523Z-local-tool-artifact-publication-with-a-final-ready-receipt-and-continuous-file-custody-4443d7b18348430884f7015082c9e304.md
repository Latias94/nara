---
type: "Verification Evidence"
title: "Local tool artifact publication with a final ready receipt and continuous file custody"
description: "Records the Windows populated-directory rename counterexample, receipt-last artifact publication, incomplete and post-commit facts, continuous file custody and actual local C# consumers."
timestamp: 2026-10-27T21:05:23Z
record_id: "4443d7b18348430884f7015082c9e304"
producer_id: "codex-root"
run_id: "local-tool-installation-publication-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "refactor/local-tool-installation-publication"
verified_by: "01a17f0a-be7d-7023-b93e-617651a8942a"
supersedes: "1f12c7706bd44248adec1203402d167e"
---

# Verification

Continue the full active foundation Goal after `b7d676d` on
`refactor/local-tool-installation-publication`. Entry had only the original untracked `vendor/`;
the containing commit's other changes belong to this unit. Reuse the root `target`, run Cargo
serially with one build job, do not push or create a PR, and preserve unrelated/user changes.
The local lifecycle plan remains in progress; this unit implements artifact publication and
inspection, not installed graph selection, executable approval or full R1b completion.

# Result

`EditorSourceToolInstallationPlan` retains the exact captured release, caller-issued parent and
destination. Preview borrows those bytes; cancel returns them without writes. Confirm reserves an
absent empty candidate directory, exclusively creates/synchronizes declared members, verifies the
selected destination and complete inventory, and commits a bounded `.nara-installation.json` by
no-replace file publication last. The receipt is strict JSON format 1, at most 4096 bytes, with a
nonzero random installation identifier and selected manifest digest. It is data, not authority.

Each exact read capability is acquired and compared while its original created handle is still
alive. The receipt reader likewise spans its temporary-file publication, with identity and bounded
digest checks before commit. After receipt publication, the complete root/member/inventory check
runs again; the successful owner then releases writers while retaining continuous readers and
private release metadata. Payload bytes need not remain copied in the installed owner. Ordinary
read sharing does not freeze content. `verify()` observes current destination, identities, digests
and inventory through the original parent; it does not establish a filesystem snapshot.

`Failed` retains the original plan and actual pre-ready begin/publication error or native orphan.
`Incomplete` retains the plan, published unready root and acquired custody when this attempt did
not commit the ready receipt. Once receipt publication succeeds, later admission refusal is
`PublishedUnverified` with both publication facts. No failure claims rollback. A competing receipt,
unknown member, changed bytes or a different object with matching bytes cannot silently become
owned/ready content. The complete inventory has an expected-entry-derived aggregate work ceiling.

Fresh `EditorSourceToolInstallationInspection::read` requires strict source authority and a
caller-selected receipt digest, decodes the receipt, recaptures the declared release through the
existing owner and verifies the full inventory. It returns code-free data without recovering
creation custody, authorizing deletion, adopting project metadata or approving execution.
The existing capture and installation share private metadata and the strict-source helper;
there is no second Schema validator, executor, compatibility reader or migration guide.

## Native Counterexample And Removed Approach

The first design populated a sibling staging directory and then renamed the whole tree while
retaining created-member custody. Real Windows execution returned `PermissionDenied(5)` for
directory publication. Closing writable handles and retaining only read handles still failed.
Those experiments do not provide passing publication evidence. Closing every member handle and
later comparing bare native IDs would break continuous object provenance, so that alternative
was rejected. The production protocol instead publishes an empty unready root first and commits
the ready receipt last. It deliberately makes no whole-directory-tree atomicity claim.

An initial orphan retry path reused temporary-file discard. It was removed before qualification:
under the selected protocol these files are already visible and may have been changed or adopted.
Temporary creation alone does not prove unchanged content or deletion authority. The orphan owner
exposes observations and retained custody only. Drop closes handles and leaves bytes. Exact
stable-content reclamation and recovery/adoption policy remain work rather than a path-based
recursive-delete fallback. No obsolete public retry/compatibility operation remains.

Reviewer `01a17f0a-be7d-7023-b93e-617651a8942a` read the final actual source and found no material
correctness issue in this bounded publication/inspection unit. It checked terminal-state/custody
transfer, receipt handoff, destination/member/inventory checks, factoring and the actual external
C# consumer. The competing-receipt test rejects at pre-ready inventory; it does not separately
inject the final `replace_temp` failure branch. That branch was reviewed but is not claimed as a
newly executed fault-injection case. Reclaimer research was source analysis, not an implemented
primitive or deletion qualification.

# Evidence

Windows 11, Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, shared root `target`, serial Cargo. Writers froze
before formatting. The actual .NET artifacts were rebuilt by the independent language runner.

| Scope | Actual result |
| --- | --- |
| Initial populated-directory publication | 9 passed / 3 failed; `f7e559e0-f437-4305-8a95-fda9560aa681`; real directory rename refused |
| Read-handles-only populated-directory retry | failed; `33485ade-01b8-4968-b9ab-f05fbfed5d04`; same native rename refusal |
| Full source-tool selection before the final receipt precommit digest refinement | 63 passed, 578 excluded; `31657354-4227-40e9-b7bf-e8d648561003`; 48.484 s |
| Actual rebuilt language/Session consumers before that refinement | 15 passed, 0 skipped; `3a1f2e21-f0b7-4449-9ee9-c79e1a41c0fa`; 18.073 s |
| Actual C# Application consumer before that refinement | 1 passed, 278 excluded; `20285e21-5c81-4643-93c0-c4da456192a2`; 0.838 s |
| Final installation and receipt selection, including the precommit digest check | 18 passed, 623 excluded; `ce0a9fa4-88ec-48a7-8a98-30f5510a5c75`; 2.041 s |
| Final independent actual C# release selection | 3 passed, 12 excluded; `8c3a1596-041c-4872-89ed-ac0157ac860a`; 0.153 s |
| Final root no-default, tooling-only library | check passed |
| Final Editor all-feature library | check passed |
| Final root all-feature/all-target correctness Clippy | passed; existing warnings remain; 44.72 s |
| Final independent language all-target correctness Clippy | passed; dependency warnings remain; 3.80 s |

Native filesystem tests cover real publication and fresh inspection, no-write cancellation,
destination conflict, incomplete custody, identical-byte object replacement, receipt tampering,
undeclared content, reserved receipt paths and strict-source admission. The external consumer
captures the real C# DLL/deps/runtimeconfig/catalog/sample, mutates only its temporary source copy,
publishes the original frozen bytes, compares all seven installed files/roles/sync observations,
closes custody, reopens and rejects sample drift. The native directory-rename test records the
qualified Windows refusal; it does not claim root-replacement execution on another platform.

Per-file synchronization is achieved in these Windows tests. Parent-directory synchronization
remains Unsupported; root naming, ready-receipt naming and durability are separate facts. These
tests do not prove crash recovery, immutable namespaces, an RSS sandbox, a deadline or Linux
installation support. There is no UI/graphics change, so predecessor Windows/DX12 qualification
is neither rerun nor extended. Formatting, relative-link and memory checks validate integration
only and do not expand product scope.

# Follow-up

Continue the full local lifecycle plan with approved installed activation through the existing
Session owner. Define complete release and external-runtime custody, the durable selection versus
routing publication cut, candidate failure preserving V1 and observable post-switch physical
retirement debt. Reopened artifact receipts must not approve their own code. Connect fresh-project
metadata through the actual authoring bundle/readmission owner; generation-one capture does not
solve evolved catalog lineage or Schema migration.

Stable-content reclamation must have its own exact ownership/content proof and honest platform
tier; do not revive temporary discard or copy the private Play store's recursive path deletion.
Installed relationships, disabled required dependents, adoption, update/disable/remove and the
complete public/native UI trajectory remain in progress. The full foundation Goal stays active.

# Citations

- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [Finite source tools guide](../../../../guides/finite-source-tools.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Filesystem authority and platform tiers](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [Predecessor release capture](2026-10-27T201403Z-code-free-local-finite-tool-release-capture-and-lifecycle-owner-design-1f12c7706bd44248adec1203402d167e.md)
