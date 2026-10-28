---
type: "Verification Evidence"
title: "Retained source objects and truthful source-tool physical cleanup qualification"
description: "Qualifies retained document and metadata aliases, superseded cleanup observation, and terminal process cleanup failure without claiming artifact reclamation."
timestamp: 2026-10-28T15:06:43Z
record_id: "13bc9f4599ef4e839fd608dcfd7f4aed"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/source-tool-ownership-boundaries"
related_plan: "docs/plans/2026-10-28-managed-source-tool-artifacts-plan.md"
---

# Scope And Result

Starting from `main@874bfc0`, implement the existing owners' retained-source alias and physical
observation corrections. Design commit `c411f43` selects the next artifact/project-source
ownership replacement separately. The complete foundation Goal remains active.

`FilePublicationHost::same_source_object` compares the actual retained leaf, including after
rename, unlink and uncertain publication. Scene/Prefab proxies and common source admission keep
that object protected independently of the current bound parent/name. Extension sources and the
actual retained selection/content metadata participate. Remove obsolete name-only metadata
accessors instead of keeping duplicate admission routes.

The existing HostCustody exposes a conservative nonblocking cleanup query, including superseded
Hosts. Source-tool maintenance advances that same custody; observations never perform cleanup.
`EditorSourceToolView::physical_work_pending` includes those obligations and deferred selection
work even after phase Idle. `process_cleanup_failed` remains sticky after process-task disposal
fails. `TaskCleanupFailed`, successful return and Pending remain distinct. Terminal failure does
not cause endless maintenance or become permission to reclaim files.

No new filesystem primitive, executor, source store, reclaimer, compatibility reader or graphical
control is added. Generic exact-object `unlink_file` remains present and unchanged.

## Verification

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1`, Rust 1.95.0 and the checkout's root
`target`. The independent fixture also selected that root target.

| Check | Actual result |
| --- | --- |
| Root/Tooling/Editor focused all-feature libraries, source admission/publication/tool/retirement filter | 182 executed: 181 passed, one new metadata test assertion failed; production code did not change afterward |
| Corrected metadata assertion, same package/feature union | 1 passed; the other 181 successful cases remained unaffected |
| Independent real language/tool consumer | 30 passed, no skips |
| Actual approved managed-child cancellation | 1 passed |
| Actual C# Application workflow | 1 passed |
| Selected Windows/DX12 native source-tool workflow | 1 passed |
| Root no-default `runtime-2d,serde,tooling` library check | Passed, 3.85 seconds; warnings remain |
| Root/Tooling/Editor all-feature/all-target Clippy, `-D clippy::correctness` | Passed, 1 minute 27 seconds; not a zero-warning claim |
| `cargo +1.95.0 fmt --all -- --check` | Passed |
| Initial affected Markdown link check | 9 files, 240 local file links passed; evidence/ledger integration has its final check separately |

The initial run was
`4e4d1ebf-5ce1-4a93-8c5b-964817b99dc1`, build 5 minutes 25 seconds, execution 107.658 seconds.
Its filter was:

```text
test(source_admission) | test(source_exclusion) | test(file_publication) |
test(source_tools) | test(source_tool) | test(document_hosts_compare_retained) |
test(persistence::retirement)
```

The new metadata test initially expected `AlreadyOpen` after creating a hardlinked replacement
of a reserved ledger. Strict metadata preflight correctly refuses that multi-link object with
`SourceRead` before alias classification. Correct only the test: first verify `AlreadyOpen` for
the unlinked retained object while the bound name is missing, then assert the exact `SourceRead`
refusal after the multi-link replacement. Do not weaken the production preflight.
The failed test process reported an abort after its assertion; this is not treated as evidence
of a production buffer-overflow defect.

The corrected single-test run was `7991f180-d3dd-4025-a041-f1c2ae30a2e6`, build 27.59 seconds,
execution 0.321 seconds. The separate root integration regression exercises Scene, Prefab and
extension owners with actual native rename, equal-byte unrelated files, namespace replacement,
uncertainty and unlinked retained objects. The new physical tests use actual held publication
cleanup and executor capture-disposal failure rather than fabricated pending booleans.

`python tests/fixtures/behavior-languages/run.py --native-editor` completed successfully:

- Independent consumer: `81cf8831-9386-48ea-bdca-6d2f9be98b02`, 30 passed in 34.145 seconds.
- Actual managed cancellation: `0cebf60b-b21d-42b2-ba88-d4e316fd332a`, one passed in 0.391 seconds.
- C# Application: `0d9d7f36-f3ca-4e69-89f6-9ae12b2ba049`, one passed in 1.890 seconds.
- Selected native: `0a90781d-dd95-484c-9cf1-16470258002a`, one passed in 6.874 seconds.

The native selection is
`native_source_tool_inline_property_and_panel_share_review_history_and_persistence`.
It qualifies the existing Dear behavior path, not Nara's future custom-drawn frontend or all
native release trajectories.

## Independent Review And Local State

Workers `01a182c8-2f12-7321-a968-69a07dd9c75d` and
`01a182c8-775c-78c2-b766-d3caf9851e90` owned disjoint implementation paths and ran no Cargo.
The parent integrated actual Session metadata/document regressions and owned all verification.
Independent reviewer `01a182ec-5dad-76e2-a7ad-3ca63953e47b` reviewed the design, production diff
and corrected assertion. No remaining blocking production correctness or architecture finding
was reported. All three agents were closed.

Existing `vendor/` remained untracked and untouched. No remote publication occurred. Cleanup of
the aborted test's exact temporary fixture was rejected by automatic approval with only
`blocked by policy`; do not retry through another deletion mechanism. That system-temp fixture
is left in place. This is not a source or verification blocker.

## Remaining Work

The current mixed installed-content protection implementation remains the production predecessor.
Follow the selected replacement in dependency order: reconcile owning ADRs, constrain managed
artifact admission, explicitly import project-owned templates, remove superseded mixed-content
APIs/record/interception, then join original-basis withdrawal with actual quiet, removal intent,
real per-member unlink and partial-result recovery.

Fresh Open must observe recovery without executing code or continuing deletion. A text-copy
template does not qualify native Scene/Prefab identity and reference remapping. Existing-owner
Schema evolution, executable gameplay publications, rendering/resources/delivery and the
custom-drawn UI remain separate foundation work. No full R1b or foundation completion, new
cross-platform unlink qualification, all-stream unchanged-content proof or performance gain
is claimed.

## Owning Sources

- [Artifact ownership design](../../../../architecture/source-tool-artifact-ownership-design.md)
- [Replacement plan](../../../../plans/2026-10-28-managed-source-tool-artifacts-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Finite-tool guide](../../../../guides/finite-source-tools.md)
- [Native qualification scope](../../../../verification/native-editor-qualification.md)
