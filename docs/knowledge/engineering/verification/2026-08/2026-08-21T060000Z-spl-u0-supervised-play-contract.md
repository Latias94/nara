---
type: "Verification Evidence"
title: "SPL-U0 supervised Play contract and launch-context verification"
description: "Verifies the versioned execution declaration, capability-free source receipt, and explicit project-root contract before build or process effects."
timestamp: 2026-08-21T06:00:00Z
record_id: "8ffa9628a3e245e4c137df5a46196731"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-21-001-feat-standalone-supervised-play-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "90d7545"
verified_by: "codex-root"
---

# Scope

This record verifies the SPL-U0 value contracts only. It does not claim Cargo build,
executable publication, child-process supervision, READY admission, or Editor lifecycle
integration; those remain owned by SPL-U1 through SPL-U5.

# Evidence

The unit established:

- the profile-local, versioned `desktop-supervised-play/v1` execution declaration;
- forward-compatible preservation of unknown execution contract identifiers;
- a bounded, canonical, capability-free `EditorExternalPlaySourceReceipt` codec;
- explicit project-root selection with no relative-path or current-working-directory fallback;
- the focused ADR/OQ/implementation-ledger/plan registration for the local external-window
  placement, while generic transport, remote execution, embedded transport, live edit, and
  extension hosting remain open.

# Commands

All Cargo commands were run serially with `-j 1` in PowerShell. No Bash or WSL command was used.

1. `cargo nextest run -j 1 -p nara_project -E 'test(execution_contract_is_profile_local_exact_and_forward_compatible) | test(execution_contract_rejects_an_unbounded_or_non_identifier_value)'`
   - Result: 2/2 passed.
2. `cargo nextest run -j 1 -p nara --lib -F runtime-2d -F serde -F tooling -E 'test(source_receipt_roundtrips_canonically_without_host_authority) | test(source_receipt_identity_includes_profile_schema_content_and_launch_context) | test(source_receipt_rejects_absolute_and_noncanonical_source_paths) | test(development_launch_uses_only_the_explicit_project_root) | test(packaged_launch_uses_only_the_sibling_project_directory) | test(root_resolution_has_no_relative_or_cwd_fallback)'`
   - Result: 6/6 passed.
3. `cargo nextest run -j 1 -p nara --test project_workflow_creation -F project-workflow-cli -E 'test(desktop_2d_v1_is_one_closed_prevalidated_project_plan)'`
   - Result: 1/1 passed.
4. `cargo nextest run -j 1 --manifest-path reference-game/Cargo.toml -p nara-reference-game --test project_manifest_ingest -F desktop -E 'test(desktop_profile_requests_the_exact_content_presentation_and_adapter_set)'`
   - Result: 1/1 passed.

An earlier root integration invocation omitted the presentation feature required by an existing
integration test target. It was not counted as a product failure; the corrected `--lib` command
above is the authoritative root result.

# Residual scope

The Host still needs to construct this receipt from current saved Scene/Prefab facts and later
bind it to an immutable generation and child READY receipt. No process effect is admitted by this
record.
