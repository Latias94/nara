---
type: "Verification Evidence"
title: "Finite reference shape certificate and clean-source public native qualification"
description: "Qualifies finite typed-reference certification, direct zero-preflight observations, restored-source comparisons and affected serial coverage; full R2 remains open."
timestamp: 2026-10-25T04:13:46Z
record_id: "d2673296c2bb413c9fd2b05bdf78a0c4"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "0a05c6c97b301c7b71e6e555333ac99f65a09d3b"
---

# Verification

This qualifies the finite reference-shape successor on `0a05c6c` plus the two frozen Rust files,
not full R2, delivery wave 1 or the foundation Goal. The
[comparison and source/context manifest](../../../../benchmarks/2026-10-25-reference-shape-certificate.md)
retain the actual scope, raw observations and disturbance limits. No public API, file version,
admission policy, Schema, classification or history authority changes.

# Result

- The private accept-only certificate now supports both typed EntityReference variants, including
  nested values and declared owners. Unproved shapes retain complete fallback; exact real JSON/RON
  encoder byte checks and the independent depth cutoff remain unchanged.
- The counted public experiment observes 576/576 certified, locally classified owner calls, both
  encoders once per call and both parser preflights zero times. Timer-only evidence is separately
  labeled. Certification also avoids the existing writer's encoded-buffer retention; these two
  savings are not experimentally separated.
- All seven temporary originals were hash-checked and restored byte-for-byte. The sole owned probe
  module was hash-checked and removed. Frozen candidate hashes were verified before/after cleanup
  and at final manual inspection; the archived instrumentation patch forward-checks afterward.
- Both uninstrumented candidate runs meet the selected populated-case comparison target. Debug P50
  is about 7-8 ms for this workload, but drifting controls and other-project builds prevent a fixed
  causal speedup, release/native latency or CPU/allocation claim. Full traversal and dual encoding
  remain whole-document work.

# Evidence

## Proof And Independent Review

The new positive reference regression failed at the original declined SceneLocal certificate in
run `0b204230-b276-436e-80f0-75b29e4063d7`, then all seven publication-shape tests passed in
`326218a3-31f0-4664-b2d9-11b5a52b32f0`. An earlier fixture compile error from a non-exported
namespace constant was corrected; it is not red-before-fix behavior evidence.

The force-false oracle remains independent of the new certificate. Four reference shapes cross
Scene/Prefab and owner presence, with individual shape-dimension sweeps, exact encoded-byte
boundaries and depth-cutoff/embedded-Prefab/patch fallback. The actual canonical serializers are
unchanged; do not describe differential admission tests as independent canonical-byte snapshots.

Four actual read-only Codex sidecars cover wire accounting, frozen-source correctness/security,
test/spec proof and standards/maintainability. No actionable finding was confirmed. They are
separate bounded reviews, not a completed multi-persona CE receipt. Final manual diff inspection
rechecks tag/string/UUID accounting, cutoff/fallback, checked accumulation, unchanged encoders and
public consumers, and absence of all probe hooks. Reuse of the existing counters and differential
helper avoids a new serializer framework, cache or profiling facility.

Code review: skipped (ce-code-review unavailable) - the attempted isolated full CE invocation
terminated without nested generic-agent spawn/wait/close capability. It launched no reviewer or
finish leaf and produced a failed, not complete, receipt. The authorized unavailable-path manual
diff scan and the four bounded reviews above are recorded instead; no CE-complete, eight/nine-lens
or cross-model claim follows. External model routes were not used. The attempted review and the
next-unit sidecar were collected and closed; neither left descendants. Raw artifacts remain in
the per-user temporary `nara-reference-certificate-mvm623kl` directory.

## Final Clean-Source Serial Execution

Explicit Rust 1.95.0, root `target`, locked dependencies, `CARGO_BUILD_JOBS=1`. Cargo commands in
this checkout were strictly serial. Ordinary tests use two threads; public/native tests use one.

| Scope | Actual result |
| --- | --- |
| Scene/Tooling, all features, libraries and integrations, `9ce2502f-c56e-4bcf-976b-a1f72692e60e` | 405 passed, 4 skipped |
| Root/Editor, all-feature libraries, `ff36496f-ea1a-49cb-bcf6-dbdfac04d45a` | 597 passed, 28 skipped |
| Independent prepared-authoring consumer, all five including diagnostic, `33ed1ffd-861b-4343-82ca-8a76e6d8a30a` | 5 passed |
| Selected actual Windows/DX12, `9e9a9b49-e787-4180-b993-fd9dd2cf9c2d` | 3 passed, 220 excluded |
| Scene/Tooling/root/Editor all-feature, all-target correctness Clippy | Exit 0; existing non-correctness warnings remain |
| Scene `--no-default-features` check | Exit 0; existing dead-code warning remains |
| Root and independent-consumer `cargo +1.95.0 fmt --all -- --check` | Both exit 0 |

The native cases actually execute production hidden-window DX12 workflows:

- `native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen`
- `native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`

This is affected-existing-workflow coverage, not a new native panel-widget or complete critical
platform group, Linux/macOS, monitor scanout, OS IME/accessibility or release-performance claim.
No production source changed after these checks. Root workspace/portable wasm checks from earlier
units were not rerun or relabeled as current evidence; the shared owner, root, public, minimal and
affected-native checks cover this private wire-proof change. Documentation closure needs only its
own link/memory/diff checks, not another native cycle.

Logs and exact argument arrays are in the temporary run's `verification-results.json`; the public,
counter and timing logs plus source hashes are durably compressed with the comparison report.

# Follow-up

The read-only wave-1 audit distinguishes already implemented stable selectors and nested writes
from a genuine remaining read seam: `EditorRootTarget` can request only a root. An admitted nested
subvalue is currently copied only as part of its complete root. The next concrete workflow should
read one keyed-row score on a non-active Scene/Prefab, compute a change from the returned value,
then reuse the same prepared candidate/inverse/history/persistence chain through Session and a
registered panel. Root reads over budget while a one-scalar selected observation fits is a useful
discriminating consumer; it must not introduce a second document or inverse store.

Inspect `crates/nara_scene/src/authoring_decode/read.rs`,
`crates/nara_reflect/src/property.rs`, `crates/nara_tooling/src/authoring_query.rs` and
`crates/nara_tooling/src/workspace/authoring_query.rs` before choosing the replacement API.
Present/Defaulted/Missing provenance, OptionalAbsent, missing keys and retired members remain
distinct. The existing Reflect resolver validates/scans complete inputs, including keyed-list
lookup: a small output budget is not a scan-work bound or O(depth) guarantee. Deepen the owning
resolution seam rather than materializing a full effective component or duplicating selector
logic in Tooling. A new contract needs its owning ADR/design reconciliation before implementation.

The sidecar compares pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc`
(`repo-ref/bevy/crates/bevy_reflect/src/path/mod.rs:278`) and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` (`repo-ref/godot/core/object/object.cpp:534`).
Borrow directed access and explicit validity, not ECS-as-document authority or tool-owned Undo.
This is read-only design evidence, not qualified implementation or a selected public API.

Full R1/R2 and remaining R3-R12 stay open. In particular finite child-process proposals,
inline/installable tool placement, durable Prefab overlays, execution scopes, Behavior, products,
render composition and final production integration are not completed by this optimization.
No push, PR, new migration guide, unrelated deletion or vendor change occurs.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Finite comparison](../../../../benchmarks/2026-10-25-reference-shape-certificate.md)
- [Prior attribution](../../../../benchmarks/2026-10-25-reference-preparation-phases.md)
- [Read projection design](../../../../architecture/authoring-read-projection-design.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
