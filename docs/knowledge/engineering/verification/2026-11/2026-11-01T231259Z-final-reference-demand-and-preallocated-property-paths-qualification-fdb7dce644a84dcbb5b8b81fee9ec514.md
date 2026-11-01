---
type: "Verification Evidence"
title: "Final reference demand and preallocated property paths qualification"
description: "Qualified shared compiler replacement after overwrite and retained-path-budget regressions, with lexical input scope selected for the next CP2 slice."
timestamp: 2026-11-01T23:12:59Z
record_id: "fdb7dce644a84dcbb5b8b81fee9ec514"
producer_id: "codex-foundation-main"
run_id: "prefab-final-reference-demand-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
git_commit: "74d5fc76f659ff6b2b2aa0dab07c3a132f917236"
supersedes: "a31a71bcf62c4deb8a38131a3a44907e"
---

# Verification

Qualified source commit `74d5fc76f659ff6b2b2aa0dab07c3a132f917236`, following the separate
construction-test correction `e4b08d4d354dd9b33ef724cd9fc08ac52e22fb62`. Tests ran on the same
source tree before its code-only commit. Windows 11, Rust 1.95.0, serial Cargo and one build job
reused the checkout's root target, including the independent workspace. No push or PR.

This closes the final-reference-demand prerequisite. CP2 extraction, the new inline-reference
format and the full foundation Goal remain active.

# Result

The previous production compiler rejected an inherited missing local reference before an outer
property pin could replace it. The actual red test was
`later_property_pin_replaces_an_unresolved_inherited_reference_before_target_lookup`, run
`0da9be78-6b4a-46b3-ba2c-f8005d25f508`. Program-wide binding discovery could also open a missing
nested source for a reference replaced by a later property suffix.

The shared compiler now keeps unpublished normalized values and transient per-leaf declaration
origins. Existing Reflect selectors identify each replaced storage subtree. Only final surviving
references demand targets/sources. Opening an occurrence discards the current candidate and
recloses structure/programs; discarded materialization counters and its migration flag reset,
while cumulative scratch, work and source acquisition remain charged. Final lowering uses real
surviving IDs and exact value/generated-ID costs. The eager binder, value-ID bridge and raw
program-wide binding scan are removed; there is no second selector evaluator or fake-ID encoding.

Independent adversarial review found a further P2: scalar property pins retain complete storage
paths even when no reference origins exist. A short selector does not bound its Schema wire
prefix. The public 512-pin/64-KiB-prefix control wrongly succeeded under a declared 40-MiB working
allowance, red run `e1af8cfd-7493-4153-b24a-e6695ad62ec1`. Reflect now admits root path copies,
storage-vector growth and selected wire strings before allocation; Scene directly charges its
single compiler reservation. Root/Absent path clones follow the same rule. The final rereview
reported no assigned P1/P2. Limits remain unchanged and model logical work/storage, not RSS.

Two full-suite failures were stale construction assertions, not a production limit regression.
ADR0113 and the existing owner controls distinguish the shared 256-operation construction
allowance from ordinary eight-write edits. Commit e4b08d4 updates exact/one-over raw-write and
declaration boundaries, rejected-correction preservation, immutable preparation and one-step
Undo/Redo. The construction production implementation is unchanged.

# Evidence

| Cohort | Actual result | Run |
| --- | --- | --- |
| Reflect and Scene, all features, all ordinary test binaries | 1277 passed; 3 existing skipped, across 71 binaries | `eeee1e42-8168-41a3-97d1-b2a607e855b0` |
| Root source loading, captured authoring and Prefab conversion | 172 passed; 1182 excluded by filter | `0e4917e5-2335-4dac-8d0d-63b7f815f236` |
| Independent module consumer: prefab_sources, prefab_conversion, durable_identity, source_context_trial | 18 passed; 1 existing skipped | `57b2ff82-5939-4981-b581-226e901f461a` |
| Actual Windows/DX12 complete Scene and Prefab conversion | 1 selected test passed; 319 excluded | `4413d88a-e5df-476d-82fd-98e0358ceb74` |
| Reflect and Scene doctests | 7 plus 12 passed, including compile-fail ownership controls | Cargo doctest log |

The separate prepared-authoring binary passes 20 tests with two existing skipped cases in run
`6e187bfe-42d3-4c4b-8fc7-db14591a54a8`; these are included in the full owner result above.

The owner command selected `nara_reflect` and `nara_scene` with `--all-features --locked` and
`--test-threads=1`. Root used `--no-default-features --features
project-workflow-cli,tooling,presentation-2d --lib` with filter
`test(project_content::) | test(capture::) | test(prefab_conversion::)`.
The independent workspace used `CARGO_TARGET_DIR=<checkout>/target`.

Additional checks passed: Reflect/Scene minimal-feature library builds, Root all-feature library
build, owning all-feature/all-target correctness Clippy and `cargo fmt --all -- --check`.
Clippy still reports warning classes such as the existing large failure carrier reused by the
new methods; this is a correctness-lint result, not a warning-free claim.

Controls cover repaired and uncorrected references, zero reads of overwritten bad dependencies,
canonical-base and missing-terminal failures, keyed paths and repeated namespaces, Persistent
and Null values, mixed default/reference origins, a one-byte final local ID and exact/one-short
value/generated-ID allowances. Storage controls cover complete root/member/map strings, denial
before vector reserve, repeated scalar prefixes and exact non-Clone caller errors.
The real-file Root control verifies the effective repaired target, unchanged saved dependency
bytes and released loader reservations; the uncorrected source still refuses activation.

Native qualification explicitly ran
`native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab` with DX12,
fallback disabled and the required adapter flag. It uses the documented hidden native windows,
actual Inspector conversion, source/history/persistence and composite receipts. This is the
selected conversion trajectory, not framebuffer readback, the complete native matrix,
Linux/macOS evidence or qualification of the future Nara-drawn Editor.

Raw logs remain in TEMP under `nara-final-reference-demand-*`,
`nara-final-reference-storage-*` and `nara-construction-current-limits-tests.log`.

# Follow-up

Independent interface mapping found a real instance-override counterexample: P must assign a
Q-internal property to P:b without changing reusable Q. A selected-base-only input environment
cannot express that. The CP2 plan and ADR0038 now select lexical payload/argument scope while
structural targets retain selected-base scope. Qualified input endpoints provide explicit nested
access; current-chain cycle detection uses actual occurrence plus input ID. Root binding preserves
contributor environments. This is a selected future contract, not implemented by this correction.

Continue the ordered inline-reference/input, contextual-default, shared flat-output and actual
Native admission replacements. Source publish/patch/classification must retain their source role;
Native registry, complete Scene preparation, processed admission/cook and direct data setters
need after-defaults gates before codecs or mutation. Then complete root-bound/forest and authored
placement replacement, concrete extraction correspondence, new-member publication/recovery and
the independent full CP2 consumer. No compatibility reader or fabricated Schema successor is
required. Do not close CP2, R5 or the foundation Goal from this prerequisite.

# Citations

- [Complete CP2 plan](../../../../plans/2026-11-02-prefab-extraction-plan.md)
- [Shared compiler design](../../../../architecture/prefab-overlay-design.md#selected-staged-replacement)
- [Source/provenance contract](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Root saved-file control](../../../../../src/project_content/tests/final_reference_demand.rs)
- [Predecessor WIP](../../progress/2026-11/2026-11-01T221729Z-final-reference-demand-compiler-replacement-in-progress-after-production-overwrite-regression-a31a71bcf62c4deb8a38131a3a44907e.md)
