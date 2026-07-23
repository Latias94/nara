---
type: "Verification Evidence"
title: "RGD-U10 local standalone candidate preparation verification"
description: "Verifies the local packaging, checkout-free consumer, smoke, policy, and candidate-workflow preparation without claiming hosted candidate execution."
timestamp: 2026-07-23T04:34:28Z
record_id: "668ca80db5e7483f80a3f25fe4ee0301"
tags: ["rgd-u10", "release-candidate", "packaging", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "412002eba1a41448aa6622f0f78dca522e1fd968"
verified_by: "Focused nextest, local candidate smoke, Python parsing, formatting, Clippy, diff validation, and exact staged-scope review"
---

# Scope

This evidence verifies only RGD-U10's locally admissible preparation at commit
`412002eba1a41448aa6622f0f78dca522e1fd968`: an allowlisted candidate layout, bounded package and
transport tooling, a checkout-free verifier/consumer path, policy tests, licenses, and a manual
candidate workflow. It does not close U10 or claim that GitHub-hosted Windows/Linux candidate jobs
have passed.

# Implemented Contract

- `reference-game/packaging/package-layout-v1.json` fixes the candidate root, payload allowlist,
  executable/data modes, file and byte budgets, and the package schema.
- `package.py` performs bounded allowlisted staging, canonical manifest/digest generation, atomic
  no-overwrite publication, and a no-checkout transport bundle containing the exact verifier
  scripts and layout.
- `smoke_artifact.py` verifies archive and bundle structure before extraction, rejects traversal,
  aliases, case collisions, links/special entries, unsafe modes, digest/provenance mismatches, and
  budget excess, then runs bounded headless and desktop probes from a randomized external work
  root.
- Extraction preserves executable bits on Linux, and candidate processes run with a sanitized
  system-only `PATH`; the consumer therefore does not inherit Cargo or the source checkout.
- The shared project-root resolver distinguishes packaged sibling `project/` content from Cargo
  development execution and rejects malformed packaged layouts rather than silently falling back.
- `NARA_WGPU_FORCE_FALLBACK=1` is a private local smoke control that exercises software fallback
  without adding a public renderer error or backend API.
- The manual workflow is protected-main-only, builds the Ubuntu and Windows matrix, verifies
  before extraction, keeps the consumer job checkout/toolchain-free, and records artifact retention
  and identity fields without publication credentials.

# Verification

- `cargo nextest run --locked -p nara --test artifact_package_policy --build-jobs 1
  --test-threads=1`: passed, 6/6 (`2e173c47-944f-45d7-b2aa-a2b51ba3f9be`).
- `cargo nextest run --locked -p nara --test ci_policy --build-jobs 1 --test-threads=1`: passed,
  10/10 (`ff0ef7f0-87c0-41bf-b5a7-12d8089e1731`).
- `cargo nextest run --locked -p nara_render_wgpu --build-jobs 1 --test-threads=1`: passed,
  20/20 (`08be598a-f7f5-4ef4-8ffc-a717f38ace9d`).
- Reference-game binary tests for `headless`, `desktop`, and `desktop_render_probe`: passed,
  21/21 (`0c1dd315-7b9f-4be6-a1ca-0856a86f4bc9`).
- `cargo build --manifest-path reference-game/Cargo.toml --locked --release --features desktop
  --bin headless --bin desktop --bin desktop_render_probe -j 1`: passed. The build emitted only
  existing root dead-code warnings and took approximately 15m53s.
- Focused Clippy passed for `nara_render_wgpu`, the two root policy tests, and the three
  reference-game binaries with one build job. The documented pre-existing allowances were
  limited to `result_large_err`, `collapsible_if`, `double_must_use`, `too_many_arguments`, and
  `derivable_impls`; the reference-game run retained three existing `dead_code` warnings from
  the root library.
- `cargo fmt --package nara --package nara_render_wgpu -- --check` and
  `rustfmt --edition 2024 --check reference-game/src/bin/headless.rs
  reference-game/src/bin/support/project_root.rs`: passed.
- Python `--help` entry points and AST parsing for all three new/changed Python files passed.
- A local reviewed Windows bundle smoke passed from an external randomized work root with the
  sanitized path and fallback GPU control. Its verifier reported:
  `sha256=6de844537f3dd90dcbba1ce2c8b1e50ef88f69c0461da3dc350cc88617955c15`, encoded archive
  size `19,634,820` bytes, expanded size `55,369,727` bytes, and `17` payload files; the desktop
  probe completed and the headless summary used schema `nara-reference-game.wave-summary-v1`,
  platform `windows-x86_64`, version `0.1.0`.
- The local archive receipt intentionally recorded pre-commit source revision
  `4f47c76ca0c48492c33e32cadf6fdc940b8aa5a4`. It is therefore mechanics-only smoke evidence,
  not a candidate artifact for commit `412002e...`; the hosted workflow must rebuild and bind a
  fresh archive to the final revision.
- `git diff --cached --check` and the exact staged-path audit passed before the code commit. The
  commit contains exactly the 16 U10 implementation/policy/documentation files and no concurrent
  architecture or memory edits.
- `architecture_docs` was intentionally not run under the user's instruction; U10 does not alter
  architecture-governance authority.

# Review Corrections

The focused staged review found and corrected Linux executable-bit loss during extraction,
unbounded JSON and staging reads, path-alias acceptance, unbounded candidate output capture,
inherited toolchain `PATH`, weak transport/archive budget checks, unsafe manifest mode handling,
and an unnecessary public renderer error surface. These corrections are included in the cited
commit and covered by the policy or package tests above.

# Remaining Boundary

RGD-U10 local preparation is complete, but U10 execution and closure remain blocked on U8's final
hosted Windows/Linux matrix. No hosted workflow run, artifact identity/retention record, public
candidate, publication credential, or release claim is recorded here. The local temporary smoke
root was not removed because the environment rejected the validated recursive cleanup operation;
it is disposable preparation output, not repository state.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u10-build-and-consume-standalone-release-candidates`
- Commit `412002eba1a41448aa6622f0f78dca522e1fd968`
