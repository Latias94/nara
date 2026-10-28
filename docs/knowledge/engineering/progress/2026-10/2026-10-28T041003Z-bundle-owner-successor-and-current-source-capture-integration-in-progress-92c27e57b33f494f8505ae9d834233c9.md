---
type: "Work Progress"
title: "Bundle owner successor and current source capture integration in progress"
description: "Records snapshot-preserving composition, live accepted leaf handoff, nominal current-source capture and remaining historical and Session adoption work."
timestamp: 2026-10-28T04:10:03Z
record_id: "92c27e57b33f494f8505ae9d834233c9"
producer_id: "codex-root"
run_id: "authoring-owner-adoption-2026-10-28"
related_plan: "docs/plans/2026-10-28-authoring-owner-adoption-plan.md"
git_branch: "feat/authoring-schema-owner-adoption"
supersedes: "becb85b221e241aebeb4845c692a7379"
---

# Summary

Continue the complete active foundation Goal. Local `main = 26ce7a2`, ahead of origin by 172,
contains the qualified source/history predecessor. Current branch is
`feat/authoring-schema-owner-adoption`; this unit is uncommitted. `vendor/` remains untouched.
No remote publication is authorized. All Cargo runs are serial, Rust `+1.95.0`, one job, root target.

# Details

The current unit adds data-only owner composition and accepted bundle-leaf handoff, plus actual
current-source capture against the unique prepared successor basis. It does not yet implement
the full project successor, historical actual-demand proof, whole Workspace/Session adoption,
PublishedWithoutAdoption/reconciliation or installed-owner UI.

Reflect `AuthoringSchemaBundle::from_owner_snapshots` shares admitted owner current/predecessor
catalogs, preserving their Arc identity. It shares composition/fingerprint validation with raw
admission, enforces owner count before each push and keeps full predecessor/tombstone claims.
Scene hidden prepared-source borrows reuse commit's exact revision/previous-basis guard and
share original document Arcs. They do not publish sources or replace live history.

Root `authoring/bundle/owner.rs` prepares one absent generation-one owner, engine-owned Adapters
and one basis; AlreadyAdmitted requires original manifest/bundle observations still agree.
The shared file publisher binds while the incumbent leaf lives. After actual publication,
attachment transfers its accepted capability directly into a successor metadata receipt and moves
the prepared basis rather than readmitting. Wrong publication or exhausted cumulative budget
returns the exact prepared input plus actual accepted evidence for reconciliation.

The manifest's admitted leaf is now shared by Arc between predecessor and successor, retaining
its physical object continuously. Paths, settings and successor digest are prepared before
publication. Receipt metadata baseline and complete operation usage are distinct; failed captures
remain charged and cannot authorize later validation/attachment after exhaustion.

`project/capture/schema_owner.rs` invokes existing actual-demand capture from validated prepared
Scene and open dirty Prefab sources. Physical authority remains incumbent during preparation;
only readiness/diagnostics/failure are exposed for review. A matching nominal successor with an
actual accepted leaf can attach the full capture, after which normal source validation uses
the successor manifest/bundle observation. Source sessions and their history remain untouched.

Independent review corrected over-budget reuse and the overly broad capture review accessor,
and confirmed preserved owner/basis identity and actual leaf authority. Source/library compile
passed after replacing three now-invalid Arc-backed const getters. Focused Reflect snapshot
and Scene view tests pass 15/15 (`2970482e-b309-49bf-b237-e7ff46b647da`). The first root test build
used a nonexistent Prefab convenience decoder; it now uses the real composition-source candidate.
The first root run passed 2/11; most failures came from a fixture importing a new capability
session for each FileIdentity comparison. The fixture now retains one root. The second run
passes 10/11 (`bc710cb2-ce8c-41b1-ac76-2e2dbe8b30e7`); remaining Windows parent rename refusal
must be qualified as retained custody, while successful namespace replacement still requires
stale admission refusal. Those results do not yet qualify the final revised code.

# Next Action

Finish the root fixture's explicit Windows retained-parent case, freeze all writers, fmt, then
run root owner/current-capture tests and complete affected Authoring/file-publication/schema
cohorts. Root production budget split and preprepared path/settings changes require final rerun.
Add/keep wrong nominal capture attachment and baseline-versus-operation usage assertions.
Run relevant full Reflect/Scene, feature-minimal root and correctness Clippy checks, independently
review final scope, then record immutable successor evidence and precise local commits/ff main.

After this unit, implement the actual project receipt using the same basis/current working
sources; extend the existing Scene history kernel for bounded streamed effective validation.
Retain actual historical file/lookup evidence separately from current assets. Preserve lawful
outer repair/discard rather than eagerly publishing every closed dependency as a standalone
source. Prepare the complete open set and Session publication/Close/recovery before claiming
fresh-project installed metadata adoption. Never substitute this internal helper for full Goal
completion or that workflow's native qualification.

Active write ownership at capture: parent owns root production/current-capture tests and ADR/docs;
Confucius owns only new `authoring/bundle/owner/tests.rs` and is finishing the Windows fixture.
Reflect and Scene workers are frozen/source-ready. No Cargo is known running at this capture.

# Citations

- [Owner-adoption plan](../../../../plans/2026-10-28-authoring-owner-adoption-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source/history predecessor](../../verification/2026-10/2026-10-28T032419Z-source-owned-schema-reclassification-with-exact-retained-history-and-bounded-interpretation-becb85b221e241aebeb4845c692a7379.md)
- [Host seam research](../../subagents/2026-10/2026-10-28T033218Z-host-schema-adoption-seams-after-source-history-qualification-656e522708f64f469f6623e660467e9a.md)
