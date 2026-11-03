---
type: "Work Progress"
title: "Encoded extraction saves and selective saved guards with classifier admission correction in progress"
description: "Actual codec-output association and selective saved guards are integrated; independent review requires complete contextual-default admission before final qualification."
timestamp: 2026-11-02T20:07:07Z
record_id: "e2a8b2523cd9412695a8239dc24cc33e"
producer_id: "codex-foundation-main"
run_id: "extraction-saved-20261103"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "88fd728"
---

# Summary

The complete foundation Goal remains active. Main and `refactor/foundation` remain `88fd728`.
All typed-root/extraction changes remain unstaged and uncommitted. This continuation adds actual
immutable persistence captures, codec-output association, selective saved guards and a real
single-leaf ProjectHost consumer. Full CP2 Project capture, Q/sidecar publication, Pending/Ready,
fresh-process recovery and native/public operation qualification remain unfinished.

# Details

## Ownership And Actual Output

Scene/Prefab persistence content now shares the actual Session document Arc. The private
`capture_shared` helper issues a distinct `EditorPersistenceCaptureIdentity`; actual version and
nominal capture are separate facts. The checkpoint owner itself issues
`EncodedEditorPersistenceCheckpoint` through the existing canonical codec. Callers cannot attach
arbitrary bytes/digest. Publication adoption compares both written and verified digests against
that output; refusal returns original output and receipt. `from_observed_file` verifies fixed
output through an actual file, with configured parent/name proof still owned by the concrete Host.
Extension output reuses the actual captured domain byte allocation without another clone.

`AuthoringDocumentReadmission` now shares the real classification Arc and history policy.
Save preservation validates candidates directly without creating a Session/history just to check
output. Ordinary Reopen retains its actual new working-owner semantics. Native/Published captures
retain their prior source through Edit, Undo and Reopen. Scene's JSON/RON source/prefab/patch
encoders use one pre-growth bounded writer with explicit-limit and admitted APIs; every new target
capacity is charged before reserve. The enclosing source owner separately admits canonical copies
and serializer/decode input. No second codec or compatibility reader was added.

## Selective Saved Stage And Host

`PreparedEditorWorkspaceExtractionSources::prepare_persistence` selects mandatory ReplaceSource,
explicit unchanged SaveCurrent and untouched Guard. The fixed table retains actual output Arc,
nominal capture, binding, revision/content and basis. Encoding failure restores the same capture;
foreign/equal-value recaptures and missing/duplicate/wrong-source commits refuse. Complete source,
Session and saved/reload destinations bind before installation. Release returns original fixed
slots and cumulative Work. Conflict remains Conflict, with ordinary Save's other states becoming
Clean. The lower source-only guard shares the same installation mechanics.

`PersistenceLeafCommit`, its validation cost and `bind_published_leaf` replace Schema-specific
names for the existing neutral leaf mechanism; actual CP1 consumers/tests were updated, without
aliases. The new private `persistence/prefab_extraction.rs` publisher takes the actual saved stage.
It requires the outstanding exact capture before filesystem effects, charges every real validation
retry and the complete file publication IO/metadata custody, and leaves incumbent Host adoption
until all other guards bind. Failures retain the encoded owner. Actual Root tests use temporary
files and combine ReplaceSource, SaveCurrent, Guard, all Host/Workspace guards, release/rebind and
later Undo/Redo. This is not complete Project Ready or interruption recovery.

## Verification And Review Corrections

Serial Rust1.95/jobs1/root target results so far:

- Focused encoding/readmission/capture/Workspace extraction:45/45,
  `7f5d18ca-e850-4178-8375-4917f6847948`.
- Root persistence and authored-upgrade cohort:98/98,
  `7e57248c-46c3-4843-93a2-3a9102098de3`. This precedes later admitted classifier corrections and
  the new Root negative/default controls, so it is not final current-head evidence.
- Complete Scene/Tooling libraries plus both independent extraction consumers:925/926, no skips,
  `7e5aad64-2bdf-4ad0-8eae-1fba046618b1`,53.056s. The one failure is the old loader-restriction
  conversion fixture expecting classification at7MiB; new full buffer-capacity admission correctly
  refuses earlier at publication-buffer. Parent now keeps that ceiling and adds a certified shallow
  2MiB payload control at3MiB to retain independent classification-refusal coverage. Not yet rerun.
- Initial builds caught one new `&String` binding fixture and old tests accessing removed
  Readmission fields; both were migrated. The latter now asserts actual classification Arc and
  history policy preservation, rather than rebuilding the former representation.

Boole's first static pass found no scoped P1/P2 but did not execute Cargo. Halley's next pass found
omitted non-asset scalar defaults missing from raw/classification multiplier estimates. The fix
adds `preserves_*_candidate_with_admission`, reusing actual bounded classification and the same
operation callback. Ordinary APIs remain shared FileLimits paths; actual decoded output does not
require another wire format to fit. Three core controls passed in the926 run above. Parent removed
the misleading retained-classification multiplier from source-copy admission and added real Root
1MiB-default exact/one-short, foreign-capture/no-IO and TargetChanged retry controls; these Root
additions have not run yet.

Meitner's independent follow-up found a second real bypass: an explicit empty list can suppress
materialization of a canonical List<Optional<EntityRef>> default while contextual-record validation
still scans all8192 Null defaults through an always-success callback. This identifies the owning
mechanism problem: complete contextual/default/reference validation must carry the same admission,
not merely the selected effective-value path. Meitner is the sole active writer now, assigned
`authoring_decode/{reclassification.rs,complete_value.rs}`, `document.rs`, bounded overlay calls if
needed, and their focused tests. No Cargo/fmt/commit is authorized for that worker. Parent must wait
for source-ready before final fmt/serial checks. No Nara Cargo is live at capture; session72187 is
terminal100. An observed Cargo/rustc belongs to the separate baozi checkout; do not stop it.

## Mature References And Next Project Pressure

The new pinned save/snapshot study records actual Bevy/Godot ownership, output-before-IO and
history acknowledgement, with limits. Neither reference qualifies Nara's stronger multi-file
protocol. Euler/Nash/format/Fermat/Halley and first reviewer Boole are closed. Bacon's read-only
next-capture study is also complete and closed.

Bacon identifies reusable all-profile physical inventory/custody, actual open observations and
existing bounded decoders. CP2 must keep original disk and frozen working variants distinct for
one physical member, observe Q/sidecar absence at exact retained parent/name, and return original
inputs on refusal. It must not inherit CP1's fixed membership, all-open source replacement,
force-Clean, bundle-last publication or Schema-successor identity. Raw decoder parameters currently
coupled to RecoveryMember should narrow to actual kind/encoding/bytes/limits/Host budget.

There is an important next pressure case to investigate before wiring Project capture:
`ConfigurationPolicy` is currently inventory scan scratch, while `CorpusLookup` assumes globally
unique aliases. Full physical membership does not justify flattening different profiles' local
aliases into one table or applying the selected profile's resolver everywhere. Preserve actual
profile resolution contexts; qualify the case where one physical Scene is used by different
profiles with differing Prefab roots. Reconcile physical document identity versus compilation
environment and consistent before/after proofs before selecting the owning model change. Do not
silently omit profiles or weaken alias ambiguity checks.

# Next Action

Finish and independently verify contextual/default/reference admission, run the corrected
conversion control and complete Scene/Tooling cohort, then new Root tests and affected ordinary
Save/CP1/extension/integration checks, minimal/all-feature libraries and correctness Clippy.
Update actual ledger evidence afterward. Keep all source/saved/Host scopes honest and no complete
CP2 claim or commit/ff yet. Then implement complete Project capture with actual profiles, separate
disk/working variants and observed-absent fixed Q/sidecar, continuing the original Host budget and
extraction Work through all downstream preparation/publication/recovery owners.

# Citations

- [Complete CP2 unit](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Pinned save comparison](../../../../research/2026-11-03-immutable-save-checkpoints-and-publication.md)
- [Checkpoint output owner](../../../../../crates/nara_tooling/src/workspace/persistence_checkpoint.rs)
- [Selective saved stage](../../../../../crates/nara_tooling/src/workspace/prefab_extraction/persistence.rs)
- [Actual file publisher](../../../../../src/project_host/persistence/prefab_extraction.rs)
- [Root consumers](../../../../../src/project_host/persistence/tests/prefab_extraction.rs)
- [Previous source-only guard progress](2026-11-02T190731Z-selective-extraction-source-checkpoints-bind-all-workspace-owners-before-installation-9696f7bec4a64642872d06fa5a3cfe29.md)
