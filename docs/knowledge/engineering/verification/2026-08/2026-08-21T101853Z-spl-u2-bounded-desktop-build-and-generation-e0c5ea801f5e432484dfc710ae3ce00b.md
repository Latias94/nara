---
type: "Verification Evidence"
title: "SPL-U2 bounded desktop build and immutable generation verification"
description: "Verifies exact Cargo metadata and artifact selection, bounded build cancellation, and immutable Editor-owned executable generations."
timestamp: 2026-08-21T10:18:53Z
record_id: "e0c5ea801f5e432484dfc710ae3ce00b"
status: "verified"
producer_id: "codex-root"
run_id: "spl-u2-build-generation-20260821"
related_plan: "docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ab211bc"
verified_by: "codex-root"
supersedes: "9b5f75ac859c466ba7f94deb74851778"
---

# Scope

This record verifies SPL-U2 only: bounded Cargo facts and JSON selection, one exact desktop build
attempt, and an Editor-owned immutable executable-generation store. It does not claim child READY
attestation, ordinary Editor Play integration, external-window lifecycle effects, or the final
Reference Game trajectory; those remain SPL-U3 through SPL-U5 gates.

# Result

SPL-U2 is complete at `ab211bc`. The implementation is deliberately split into deep private
modules rather than accumulating another monolithic Editor file:

- `src/project_workflow/cargo_facts.rs` owns the pure, feature-gated Cargo metadata/artifact fact
  grammar and bounded incremental JSON decoder. It has no process, filesystem, or capability
  authority and is shared with Export only through hidden facts.
- `editor/src/supervised_play/build.rs` owns the build-attempt lifecycle; `build/source.rs`
  owns source-revision facts and `build/cargo_driver.rs` owns tool discovery, exact command facts,
  and bounded process-output decoding.
- `editor/src/supervised_play/generation.rs` is a small facade over `generation/format.rs`,
  `generation/platform.rs`, and `generation/store.rs`. The format module is pure, the platform
  module is the narrow handle/executable adapter, and the store owns the coupled publication,
  retention, owner-marker, and close transaction.

The unit now proves:

- real Cargo metadata version-field admission (`version`, not the non-existent `format_version`),
  unique root/package/desktop-target facts, exact manifest/profile/feature binding, and bounded
  diagnostics;
- `compiler-artifact` plus `build-finished` selection with foreign artifacts ignored, `fresh: false`
  accepted, duplicate/wrong/non-executable/malformed results rejected, and build failure/cancel
  producing no generation;
- manifest/lock drift supersedes an attempt before publication;
- executable copy digest revalidation and canonical launch-manifest framing;
- no-replace generation publication, active/last-good/candidate retention bounds, owner markers,
  conservative scavenging, and close-time cleanup;
- raw Cargo output/loss is consumed through the SPL-U1 process owner, so cancellation is terminal
  only after process-tree cleanup rather than `Drop`.

# Commands

All Cargo commands were run serially in PowerShell with the existing `target` directory. No Bash,
WSL, or parallel Cargo invocation was used.

1. `cargo check --offline -p nara-editor --lib`
   - Result: passed (warnings only; the remaining warnings are expected future U3/U4 consumers).
2. `cargo nextest run --offline -p nara --lib -F project-workflow-cargo-facts,tooling -E 'test(project_workflow::cargo_facts::)' --test-threads=1 --no-fail-fast`
   - Result: 20/20 passed.
3. `cargo nextest run --offline -p nara-editor --lib -E 'test(supervised_play::build::tests::)' --test-threads=1 --no-fail-fast`
   - Result: 5/5 passed.
4. `cargo nextest run --offline -p nara-editor --lib -E 'test(supervised_play::generation::store::tests::)' --test-threads=1 --no-fail-fast`
   - Result: 8/8 passed.
5. `cargo nextest run --offline -p nara --no-default-features --features project-workflow-cli --bin nara -E 'test(exact_headless_and_desktop_artifacts_form_one_selection) | test(an_additional_project_binary_is_rejected)' --test-threads=1 --no-fail-fast`
   - Result: 2/2 passed; Export retains its exact headless+desktop policy.
6. `cargo fmt --all` and `git diff --check`
   - Result: passed; Git emitted only existing LF-to-CRLF checkout notices.

# Follow-up

SPL-U3 is next. It must add the profile-scoped `desktop-supervised-play/v1` declaration, the
capability-free launch/source receipt, the inherited bootstrap handshake, and the one-shot
first-present READY observer. The child protocol remains value-only: no process handle, path,
capability, Winit object, or general bidirectional transport crosses the Host boundary.

# Citations

- `ab211bc` (`feat(editor): add bounded supervised desktop build and generations (U2)`)
- `docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md`
- `docs/architecture/adr/0116-standalone-supervised-editor-play.md`
- `9b5f75ac859c466ba7f94deb74851778` (SPL-U1 process-owner evidence)
