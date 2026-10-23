---
type: "Verification Evidence"
title: "Post-boundary U8 explicit Bevy ECS entry qualification"
description: "Verification Evidence for Post-boundary U8 explicit Bevy ECS entry qualification."
timestamp: 2026-10-23T07:51:51Z
record_id: "b54c09bf3ae54a5dbc48b382336ea785"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "17f292a"
---

# Verification

U8/E2 replaces the complete public `upstream` dependency alias with `nara_ecs::bevy_ecs` and
`nara::ecs::bevy_ecs`. It preserves the ordinary prelude, original runtime types, six adapted
derives, hidden generated support and the `nara_ecs_derive/upstream/` provenance directory.
Production, tests, examples, Reference Game, samples and independent renamed/module consumers use
the explicit dependency name. Accepted ADRs 0002/0044 and the existing active migration example
describe the current path; historical evidence and the old migration row retain their original
observed name. No new migration guide or compatibility alias is added.

All Cargo commands were serial with Rust 1.95.0, `--locked`, one build job and the root target:

- ECS/derive nextest: 23 passed, none skipped, run `2e2cfcee-088a-43de-9e7e-d1b9f71d4f0f`.
- ECS rustdoc: one negative import test passed, proving the old public alias is unavailable.
- `cargo +1.95.0 nextest run -p nara --test derive_dependency_fixtures --locked
  -E 'test(renamed_dependencies_resolve_derive_crates)' --test-threads=1`: the one harness passed,
  run `d5d8bcfb-e07f-420e-bd79-2e0c228ecc12`. It actually runs both renamed-root and renamed-ECS
  fixture tests, including their shared six-derive contract and original native system semantics.
- `cargo +1.95.0 check --workspace --all-targets --all-features --locked`: passed with existing
  warnings. This compiles every enabled root/Editor/example/test target, not only the ECS crate.

Tracked-source search finds no old public imports except the intentional negative rustdoc. The
independent derive implementation module is unrelated and deliberately retained. No changes exist
in `crates/nara_ecs_derive` for this naming unit. Final bounded independent read-only review found
no blockers after inspecting the facade/export, six-derive consumer and ADR boundary plus tracked
legacy-path search. It confirmed original type interoperability and unchanged private/provenance
support; it made no test execution claim. Final combined qualification remains separate.

# Result

The direct and renamed authoring contracts compile and execute through the explicit full-crate
name. Naming does not claim backend independence or adapt every Bevy macro. Final combined owner,
independent consumer and native evidence remains separate from this API unit.
