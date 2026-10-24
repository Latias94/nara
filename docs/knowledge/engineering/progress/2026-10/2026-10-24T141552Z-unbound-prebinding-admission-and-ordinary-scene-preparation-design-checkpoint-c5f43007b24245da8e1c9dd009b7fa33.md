---
type: "Work Progress"
title: "Unbound prebinding admission and ordinary Scene preparation design checkpoint"
description: "Reflect revalidates retained unbound native values before target allocation; ordinary Scene prepare remains the next owning implementation."
timestamp: 2026-10-24T14:15:52Z
record_id: "c5f43007b24245da8e1c9dd009b7fa33"
producer_id: "codex"
run_id: "session-01a1629b-prebind-admission"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_base: "e988712"
status: "qualified-prerequisite-scene-prepare-not-implemented"
---

# Summary

Reflect's `UnboundComponentApplyBatch::validate_fresh_targets` rechecks its original World,
touched asset catalogue and fresh native binding/lifecycle/support eligibility after the last
allowed flush, without binding targets, rerunning preparation or publishing values/receipts.
Native values imply targets; a Scene with targets but no native values still admits receipt
support. Existing staged targets and bound publication retain their later full validation.
The private batch/value ownership stays unchanged. There is no new global revision or registry.

ADR 0081 adopts this narrow primitive. The selected next Scene design uses a move-only ordinary
token at this existing unbound stage and one split kernel for ordinary and advanced flows.
First placement declares an occurrence; replacement retains its exact predecessor. Unbound
tokens reserve neither a slot nor a numeric runtime instance. Their actual public API, managed
Schema association and complete consumer qualification are not implemented by this prerequisite.

# Details

- Read-only Reflect investigation `01a16e21-6f99-75b0-974b-44865ff7c4d4` established the reusable
  native-value ownership and missing prebind validator. It is closed.
- The five new regression tests first failed to compile because the missing method did not exist.
  A deliberately narrow baseline delegate to `ComponentApplyBatch::validate_commit` then produced
  red run `d76c2e44-a504-4c5c-b295-be46d80a94e6`: 0 passed, 5 failed. Four failures demonstrated
  missing late hooks, asset/flush, binding-authority and empty-value receipt admission. The fifth
  exposed a fixture's incorrect zero-entity assumption after Bevy resource metadata work.
- The complete validator's first run `5022ffb0-6070-4557-b25e-bb014b7776d2` passed three and failed
  two fixture roster assertions. Bevy hooks/resources can create internal entities; controls now
  perform the same permitted work and compare the allocator or unchanged roster. No production
  guard was weakened. The final five-test run passes:
  `64ff05c5-da3b-4d8c-9af9-f780d69f221b`. Both stable/runtime conflict directions are exercised.
- Final `cargo +1.95.0 nextest run -p nara_reflect -p nara_scene --all-features --locked -j 1
  --no-fail-fast --failure-output immediate --status-level fail --final-status-level fail` passes
  570 tests, 1 ignored allocation measurement, run `7b375d7c-e0ab-4b85-8f27-87a867246948`.
  Reflect no-default-feature/all-target `cargo check`, all-feature/all-target Clippy with
  `-D clippy::correctness`, and root `cargo fmt --all -- --check` pass on Rust 1.95.0.
  Existing non-blocking warnings remain. All Cargo work is serial with `CARGO_BUILD_JOBS=1`.
- Read-only implementation review `01a16e3f-070c-7d43-a208-cb8f740fec3f` found no supported
  defect in the bounded method/tests; it is closed. Read-only design review
  `01a16e36-76e4-7c23-adae-fa7bb7731cd6` found no blocking design gap and is closed. It specifically
  requires opposite-order two-token commit in an initially domain-free World, exact replacement
  after individual member retirement, touched/untouched asset pairs and retained/managed Schema
  pairs in the subsequent public Scene implementation. Neither review executed tests.
- The main owner scanned the diff for unnecessary abstractions: the new method reuses existing
  support and receipt validators; it preserves each fresh binding rather than deduplicating
  conflicting values. Protected regression tests remain unchanged after their fixture corrections.
  No native window path or existing materialization implementation changes in this prerequisite;
  a new native qualification is not claimed or required for the currently unused primitive.

Code review: skipped (ce-code-review unavailable)

The actual portable invocation `01a16e47-3553-7491-a7f1-26ade8d869f9` returned terminal `failed`
after two unsuccessful discoveries of native child-agent launch/collection tools. It is closed;
no portable artifact or completed receipt is claimed. The scoped native independent reviews
and explicit main-owner final scan above are separate evidence.

# Next Action

Implement the ordinary public Scene prepare/new/replace/one-shot commit operations by splitting
the existing kernel immediately before target allocation. Retain exact frozen definition and
source/occurrence facts, validate current eligibility before binding, and acquire target-bound
Identity proofs only against the current authority. Keep standalone retained definitions usable
after the caller registry drops; do not permit a managed registry mismatch. Use the selected
design and complete public/independent consumer acceptance before qualification. Then continue
Runtime/Canvas complete-key lowering and the other foundation waves. The full goal stays active.

# Citations

- [Selected next Scene boundary](../../../../architecture/scene-source-and-occurrence-identity-design.md#ordinary-preparation-selected-next-boundary)
- [Native binding ADR](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Unbound batch](../../../../../crates/nara_reflect/src/persistent_apply.rs)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
