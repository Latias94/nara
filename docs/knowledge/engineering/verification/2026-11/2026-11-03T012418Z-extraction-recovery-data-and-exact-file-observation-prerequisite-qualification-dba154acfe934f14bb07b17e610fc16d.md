---
type: "Verification Evidence"
title: "Extraction recovery data and exact file observation prerequisite qualification"
description: "Actual reviewed working encodings, immutable recovery data, pre-admitted source decoding and independent physical leaf observations retain original owners and strict native admission."
timestamp: 2026-11-03T01:24:18Z
record_id: "dba154acfe934f14bb07b17e610fc16d"
producer_id: "codex-foundation-main"
run_id: "extraction-recovery-data-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd7289c0d680481e0f964561650de05f90cbad"
---

# Verification

The complete Prefab extraction unit now has immutable bounded recovery data issued from the real
Review and fixed outputs, plus neutral exact-file observations for partially created destinations.
This qualifies those prerequisites only. It does not implement package/Pending transport, complete
mixed-generation recovery, Session/Application confirmation, multi-file publication, final Ready
or native extraction. The full foundation Goal stays active. Main and `refactor/foundation` remain
`88fd7289c0d680481e0f964561650de05f90cbad`, with the large authorized WIP uncommitted and unstaged.

# Result

## Actual working and output owners

Core `PreparedPrefabExtraction::encode_original_source` borrows the actual original working corpus
and uses the shared bounded canonical codec. Its returned Vec is recovery data, not an issued
publication output. Tooling exposes the narrow `encode_reviewed_working_source` delegation without
taking a saved checkpoint. The sealed output/checkpoint encoders retain their original association.

`outputs/recovery.rs` consumes the complete Project fixed-output owner. It retains working encodings
and output allocations across refusal/retry, builds admitted linear physical-index maps, and issues
the package from actual disk bytes, actual reviewed working data and already fixed outputs. Guard,
SaveCurrent, ReplaceSource and closed rewrite keep distinct dispositions. Cancellation drops actual
package storage before refunding its exact Host reservation; source Work remains cumulative, and
original output/checkpoint cancellation remains with its prior owner. Partial owners expose no
package as a successful result. No filesystem write occurs in this preparation.

`recovery_data.rs` owns the canonical-v1 code-free package, strict input/count/shape/aggregate
admission, fixed blob commitments and immutable encoded bytes/digest. It retains the complete
original source table, selection/profile, old/final Ready configuration, unchanged bundle, new
Q/sidecar, metadata guards, roots and missing sidecars. Source/output identity, configuration,
canonical-output and SaveCurrent checks are data admission only; complete physical membership and
semantic stored-pair admission remain external. Q and sidecar classify absence as NotCreated;
missing original members classify separately. Native identity, nominal Schema/history and historical
replacement receipts are never serialized.

## Owning corrections from review and execution

Independent review reported two P2s. The initial package decoder sorted and checked source structure
before its ordering charge, allowing invalid structure to skip that cost. Core now provides
`decode_bytes_with_admission`: after bounded wire parsing, the callback sees the raw source before
domain traversal and canonical ordering. The shared Project `source_decode` admits ordering and
later local structure checks there, retaining their cumulative cost on failure. The package removes
its duplicate candidate pipeline and reuses that owner; canonical byte comparison has its own
prepaid work. A separate reviewer closed the assigned P2 with no remaining scoped P1/P2.

The second P2 allowed an impossible Missing root by checking only its full path. Validation now
derives the first missing component from its exact existing ancestor and compares it with present
roots, file parents and other Missing roots' existing ancestors. Three regressions cover the
original counterexample, dependent contradictions and valid exact/independent/Unicode paths. The
final separate review closed this P2.

Actual raw-observation tests then found a role omission missed by the first static review: original
`claim(path)` rejected an observed source but not an observed required `path.meta`. It now rejects
both before inserting the original claim. The sidecar search compares virtual suffix bytes under
preadmitted work without allocating another path. The focused rereview corrected its earlier
overstrong verdict and found no remaining scoped P1/P2.

Two fixture corrections preserve the underlying product contracts. A Scene that only instances P
inherits its effective change without rewriting its own declarations; the actual closed-rewrite
control is a desktop-only Prefab storing a suppression inside the cut. A shared Scene is interpreted
in both profiles and cannot pin the desktop source's base while the same alias selects the mobile
Prefab. Also, Windows denied renaming an ancestor containing a held child directory. The negative
leaf test now actually replaces its exact empty parent, retains the old complete custody and
requires SourceChanged; it does not ignore a failed mutation or skip the assertion.

## Physical observations and interpretation

Stable-metadata recovery may observe Q while Q.meta is absent. `SourceInventory::capture_admitted`
therefore accepts sorted finite `ObservedFileClaim { path, maximum_bytes }` entries. Their Present
or Missing facts have their own table, exact parent/ancestor custody, namespace/alias/identity
checks, equality, comparison cost and positive/negative revalidation. They are excluded from
original native participants and metadata. Original/open/config-required source and sidecar role
overlap refuses. Ordinary Ready capture and CP1 pass an empty list; the scanner gains no Pending
or semantic transition authority. Five actual-file tests cover all four Q/sidecar existence states,
additional sources, role conflicts, content/identity/presence changes, refusal settlement and exact
parent replacement outside configured roots.

Environment capture now accepts the real root, selected profile and borrowed destination
key/record/authored-path facts. It no longer requires a Ready receipt or an absent-destination
wrapper to construct actual per-configuration before/after catalogues. Live capture still supplies
the same actual facts. Concrete recovery can later reuse this path without fabricating a Ready
receipt or native identity for a partially published Q.

# Evidence

All Cargo was serial, Rust `+1.95.0`, jobs1 and root `target`. No build artifacts were deleted.
General/WIP warnings remain; correctness Clippy was the enforced lint scope.

- Original working/output encoder public controls: 2 passed,
  `bfa6da41-1a6c-4b87-b27f-d6f805c88bc5`.
- Raw decoder admission controls: 2 passed,
  `3aa4ded4-da96-48d1-a834-db969ca99cfd`.
- Final Scene full library and complete saved-generation integration: **785 passed, 0 skipped**,
  `fcd9bcc1-5866-4246-8cdc-a978c024b360`,47.204s after15.24s build. These files were unchanged
  by subsequent Root role/test corrections.
- Initial package/Project cohort:14 passed/1 fixture failure,
  `741aaf1c-8170-4dc4-b209-a0ae5ea0badf`; all13 initial package tests passed. The next shared-Scene
  fixture was correctly rejected for a source-base mismatch,
  `3290bf7e-2ffb-421a-8723-294165dcad8a`. The subsequent197 cohort had196 passes and one remaining
  mistaken inherited-Scene disposition expectation, `dd095d44-7295-40b9-9784-a96aae885226`.
  These fixture expectations were corrected without weakening product admission.
- First final207 cohort had205 passes and the sidecar-role/Windows mutation failures described
  above, `a5be25e2-24f9-41f1-9b26-7d1fc0fd3256`.
- Corrected actual observation controls: **5 passed**,
  `03644a70-0ae2-4270-b482-2a2010d49f1e`,0.566s.
- Final complete affected Root cohort: **207 passed**,1231 outside-filter skips,
  `332b730d-086c-4726-a3e2-b7b58becb03a`,33.350s after1.01s build. This includes all extraction,
  package, shared decode, neutral inventory, old upgrade inventory/preparation/adoption/recovery,
  Session authored-upgrade and actual encoded checkpoint/extraction leaf controls.
- Scene/Tooling all-feature/all-target correctness Clippy passed50.36s; their minimal libraries
  passed8.72s. Final Root/Editor all-feature libraries passed23.90s; minimal Root passed0.34s;
  Root all-feature library correctness Clippy passed36.80s, terminal session34014 exit0.

TEMP logs are `nara-extraction-recovery-original-encoding-tests.log`,
`nara-extraction-source-decoder-admission-tests.log`, `nara-extraction-recovery-scene-tests.log`,
`nara-extraction-recovery-package-tests.log`, `nara-extraction-recovery-package-consumer-tests.log`,
`nara-extraction-recovery-package-root-cohort.log`, `nara-extraction-recovery-final-root-tests.log`,
`nara-extraction-observed-files-corrections.log`, `nara-extraction-recovery-qualified-root-tests.log`,
and domain/product/minimal/Root Clippy logs with the same recovery prefix. Raw logs remain outside
tracked memory. Final format/memory/whitespace checks follow this capture.

# Follow-up

No live agents, Cargo or cleanup helpers remain. All six participating/review contexts are closed.
The fixed recovery data and neutral observation prerequisites are coherent; continue implementation
of the complete unit rather than reporting CP2 or foundation complete.

1. Factor only code-free package/manifest transport from `source_upgrade_storage.rs`: exact current
   Pending reads and package commitment, retained native custody/cleanup, package-before-Pending,
   and fixed Ready publication. Do not reuse Schema-successor or field-upgrade policy for extraction.
2. Implement the concrete extraction recovered-inventory owner under actual Pending authority.
   Originals must be original/fixed output, with complete membership and exact metadata/root/missing
   guards. Pass exactly Q/Q.meta as raw observation claims; each can be absent/fixed output. Build
   before configuration catalogues without Q metadata and after catalogues with its fixed metadata.
   Capture actual unchanged bundle/Adapter authority; do not manufacture a Ready receipt from the
   package's old manifest or fake native admission before Q's sidecar exists.
3. Decode original disk, reviewed working and fixed outputs in one physical index order under that
   new actual nominal basis. Use existing `PrefabExtractionRequest::admit_outputs` and independent
   `prepare_saved_generation`, preserving Q IDs/input names and Guard/SaveCurrent semantics.
4. After all output publication, capture an ordinary complete future native inventory with no raw
   observation exemptions, including Q. Then bind future Project, persistence, lanes and every
   source/saved guard before Ready and infallible installation. Complete Session/Application
   Review/Confirm and physical tool/Play retirement, actual independent fresh-process interruption,
   Check/Play and affected native qualification before the complete-unit local commit/ff main.

# Citations

- [Previous fixed-output/stored-admission qualification](2026-11-03T004438Z-fixed-extraction-outputs-and-borrowed-stored-generation-admission-qualification-03bf5d78ca3f406abeb2e28d78c557cd.md).
- [Complete extraction plan](../../../../plans/2026-11-02-prefab-extraction-plan.md).
- [Pinned source/context and recovery comparison](../../../../research/2026-11-03-profile-resolution-and-saved-generations.md#stored-extraction-recovery).
- [Pinned immutable checkpoint/publication comparison](../../../../research/2026-11-03-immutable-save-checkpoints-and-publication.md).
