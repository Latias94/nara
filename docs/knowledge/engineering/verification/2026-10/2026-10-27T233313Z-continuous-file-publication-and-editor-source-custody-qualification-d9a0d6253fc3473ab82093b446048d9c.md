---
type: "Verification Evidence"
title: "Continuous file publication and editor source custody qualification"
description: "Qualifies live leaf custody across publication, fresh authoring admission and pending image imports, with public, managed and native consumers."
timestamp: 2026-10-27T23:33:13Z
record_id: "d9a0d6253fc3473ab82093b446048d9c"
producer_id: "codex-root"
run_id: "file-publication-source-custody-2026-10-28"
related_plan: "docs/plans/2026-10-28-local-source-tool-release-plan.md"
git_branch: "refactor/file-publication-source-custody"
verified_by: "01a17f8f-acde-7d70-b677-8451cde37bb0, 01a17fa4-0714-7773-b033-8f2aee864eb9"
supersedes: "b852454a4fe841548bed563ca97d071e"
---

# Verification

Continue the complete active foundation rebaseline from `b680187`, on
`refactor/file-publication-source-custody`. This is a prerequisite correction for the local
source-tool lifecycle plan, not completion of durable selection or the foundation Goal. The
containing commit binds the tested source changes and owning documents. The original untracked
`vendor/` was preserved; no remote publication, separate Cargo target or concurrent Cargo builds.

# Result

The shared `FilePublicationHost` now owns an optional move-only `FilePublicationSource` containing
the accepted leaf capability and observed digest. Its binding is derived from that source rather
than duplicated as an authoritative ID/digest pair. `bind_existing` requires a live observed
`FileCapability`, verifies the current name and digest while that object is alive, and retains the
matching object. Reconciliation consumes the actual newly read capability. Uncertainty preserves
the prior object's custody while withholding an accepted observation.

Temporary publication opens and verifies a reader while the original writer is still alive,
then carries that reader across writer consumption and accepted-token handoff. Pre-publication
reader validation has a distinct `ValidateTemporary` failure stage. Once the name has published,
subsequent failure remains Uncertain with the real receipt, not a fabricated unpublished failure.
Existing replacement remains detect-only; read capabilities do not freeze content or strengthen
directory durability.

Scene, Prefab and extension Reopen carry their actual read handles through preparation and owner
admission. Ordinary authoring Scene/Prefab admission now carries its read leaf into the file Host.
Captured startup facts are historical: startup independently rereads and decodes under the retained
Schema basis before transferring this newly admitted leaf. Anchored authoring Prefab Open similarly
uses its existing Reopen admission. No complete source catalog is kept open for this purpose.

Image imports retain the concrete metadata Host through pending acceptance, including the
unchanged-settings branch. Completion reopens the current name for drift detection while the
accepted leaf remains alive. Cancellation, completion, catalog revocation and abandoned-owner
physical Close use the incumbent Host custody. Both Image and generic source-asset imports freshly
read and decode current metadata instead of treating a catalog's historical native ID as a live
object proof. Equal bytes on a different object may pass this new admission; changed displayed
content refuses. The generic catalog API now exposes `observed_digest`, not a purported live binding.

Removed the old bare-ID binding interfaces, split pending metadata fields, unused receipt cache,
unused Prefab test helpers, duplicate startup document transfer and its now single-field
`AuthoringProjectSessionParts` wrapper. `into_session_snapshot` returns the actual concrete result.
No compatibility alias, old reader or migration guide was added.

## Findings And Corrections

The first independent review found two real gaps beyond the publisher itself: pending Image
imports retired their accepted leaf too early, and catalog observations were being rebound as
though they retained object identity. Both were corrected through the real consumers. Integration
also found the corresponding authoring read-to-Host gap and corrected it. The final independent
review reported no new material finding in these paths; it was static review, not test execution.

The initial full library run passed 643 tests and aborted one fixture after Windows returned
ERROR_ACCESS_DENIED to its unconditional parent-directory rename. A retained accepted leaf can
prevent that native directory operation. The revised regression requires the Windows refusal,
preserves exact alias exclusion, and requires rename success after physical Close. Other platforms
retain the rename-success branch. No handle was dropped to recover the old test assumption.
New tests prove live-object custody by reading the retained object after rename/unlink and by
checking native open/close lifetime; they do not attempt brute-force native ID reuse.

# Evidence

Windows 11 x64 build 26300, Rust `+1.95.0`, .NET SDK 10.0.400 and selected framework 10.0.11.
Native Editor cases used actual DX12, hidden windows and owned Context input. All Cargo work was
serial with `CARGO_BUILD_JOBS=1` and the existing root target. Writers froze before formatting and
qualification. After the small single-field-wrapper removal, the complete library/public
persistence suite was rerun on the final source.

| Scope | Actual result |
| --- | --- |
| Final focused publisher, authoring handoff, pending metadata and source-import regressions | 31 passed, 627 excluded; `1476df07-24d6-45e4-80f9-606a442470a1`; 5.931 s |
| Final root all-feature library plus public `editor_persistence`, including linear-authority compile failures | 672 passed, 6 skipped, 1 slow; `5bd86af0-596c-4006-8840-152489eb3651`; 287.536 s |
| Rebuilt independent language, installed artifact, managed bootstrap and actual Session consumers | 22 passed, 0 skipped; `bdb65cb8-4ed6-45ed-ba18-f1e768032d2c`; 21.609 s |
| Explicit actual managed child cancellation/custody case | 1 passed, 657 excluded; `6649de06-e701-49cd-93c4-90aa719d7d8a`; 0.292 s |
| Actual C# Application consumer | 1 passed, 278 excluded; `cd53ed48-60a8-4e7c-8bc2-d7444967f4d2`; 0.811 s |
| Actual installed-image native Inspector/panel/history/Save/Close/fresh Open | 1 passed, 278 excluded; `3887d273-c5db-45c3-b92f-68983b62d45a`; 7.078 s |
| All-feature native Scene image reimport and ordinary Scene/Prefab widgets Save/Reopen | 2 passed, 277 excluded; `a715a87c-dd72-4e59-a60b-aac38b51ed6a`; 9.884 s |
| No-default `runtime-2d,serde,tooling` library | check passed; 3.30 s; existing feature-conditional warnings |
| Root and Editor all-feature/all-target correctness Clippy | passed with `-D clippy::correctness`; 1 m 20 s; existing warnings remain |

The actual consumer/native sequence was `python tests/fixtures/behavior-languages/run.py
--native-editor`, followed by the explicitly selected two ordinary native trajectories. The six
standard-suite skips are not counted as executed tests. The separately selected cases above have
their own outcomes; neither they nor the ordinary suite establish every ignored native trajectory.
Formatting, local links, diff checks and engineering-memory validation complete integration.

The source facts of other capture/index/cache pipelines remain historical observations; this
unit does not qualify continuous native-object identity for that entire system. Startup now has
one fresh decode at Session opening; its performance has not been separately measured. There is
no immutable-content, strong predecessor CAS, crash-durability, complete native-group, Linux or
macOS qualification claim from these results. Native image tests do not imply GPU framebuffer
readback or monitor scanout. Nara's custom-drawn Editor destination remains future work.

# Follow-up

Continue the local lifecycle plan's concrete durable selection unit. Split successful probing
into Ready and the single route-promotion helper; publish a bounded desired-selection record
before promotion, retain truthful disk V2 / active V1 outcomes and require explicit reconciliation
after uncertainty. Fresh Open only observes data and does not restore execution approval. The
plan records the project-local sidecar direction and the required ADR 0035 authority reconciliation
before implementation. Durable selection, installed dependency closure, metadata adoption/evolved
lineage, modified-content preservation, exact removal and complete Package UI remain unfinished.
Do not treat this shared prerequisite as R1b or foundation completion.

# Citations

- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Local lifecycle plan](../../../../plans/2026-10-28-local-source-tool-release-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [ADR 0115](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Captured managed predecessor](2026-10-27T221624Z-captured-managed-tool-images-through-the-session-and-exact-finite-child-custody-b852454a4fe841548bed563ca97d071e.md)
