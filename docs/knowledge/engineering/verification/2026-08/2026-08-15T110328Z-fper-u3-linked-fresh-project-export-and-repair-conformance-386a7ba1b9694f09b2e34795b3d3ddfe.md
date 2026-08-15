---
type: "Verification Evidence"
title: "FPER-U3 linked fresh-project export and repair conformance"
description: "Verifies the public create, check, source run, source-carrying export, explicit repair, rerun and re-export trajectory."
timestamp: 2026-08-15T11:03:28Z
record_id: "386a7ba1b9694f09b2e34795b3d3ddfe"
resource: "nara-first-party-project-workflow"
tags: ["project-workflow", "fresh-project", "fper-u3", "repair", "export", "conformance"]
status: "complete"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e37eb0f"
verified_by: "codex-root"
---

# Verification

FPER-U3 closes the first complete first-party project workflow on implementation commit `e37eb0f`:

1. create `desktop-2d/v1` through the public `nara new` command;
2. edit ordinary Scene data without changing Rust;
3. run the public check plus presentation-free headless and real Winit/WGPU desktop products;
4. publish and accept a source-carrying startup-closure export from an unrelated working directory;
5. move the selected startup Scene and observe the typed failure;
6. repair only the explicit scenes-root-relative replacement under the cooperative manifest lock;
7. run a fresh check, rerun both products, and publish a distinct successor export.

# Result

- Result: passed.
- Public commands observed by the linked trajectory: `11`.
- Final trajectory wall time with the warmed bounded Cargo cache: `141651 ms`.
- Before repair:
  - manifest: `e44368db0ed2d356dee786b23df8655b8eda778e7bc8e5c266e3cc1e0deb3173`
    (`707` bytes);
  - startup content: `ef802ee7bf91fb30c5b1ab893ae37dc6d41cf45f7c2fc4c17ec6f7b3c5e665ee`
    (`2305` bytes).
- The ordinary authored Scene edit and the explicit replacement both retain digest
  `944986a117fb5a115bce0ed1b30a13fba14c955a3eb49031ceded157b2f1bac8` (`2305` bytes).
- After repair:
  - manifest: `db3d481b10baddbee5c8fc616d1110155ace3fcff5f4773a7f691854e4409068`
    (`1651` bytes);
  - startup content: `969657c84a4e9343fbd6f3f2895c9a9ef360ecd49a5a64bb35025533bc48f5c3`
    (`2305` bytes).
- Every product observation binds its predecessor attempt, target/profile, manifest, startup
  content and the digest of the executable that the child process actually launched.
- The prior candidate remains byte-for-byte unchanged after repair; the successor candidate binds
  the new manifest and startup-content facts.

# Evidence

- Passed final product trajectory, Nextest run `a373d61b-155d-4cfc-aed0-3e636dde24bc`:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  $env:CARGO_PROFILE_TEST_DEBUG='0'
  cargo nextest run --locked --no-default-features --features project-workflow-cli,presentation-2d,desktop-winit,render-wgpu --bin nara --run-ignored only -E 'test(fresh_project_export_repair_is_one_linked_product_trajectory)' --test-threads=1 --no-capture
  ```

  Result: `1 passed`, `4 skipped`, `141.929 s` Nextest duration. Both headless runs completed one
  fixed tick and both desktop runs proved one real first presentation before normal cleanup.
- Passed focused starter and unsafe-coordinate regressions:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  cargo nextest run --locked --no-default-features --features project-workflow-cli --test project_workflow_creation -E 'test(desktop_2d_v1_is_one_closed_prevalidated_project_plan) or test(immutable_git_source_rejects_mutable_or_unsafe_coordinates)' --test-threads=1
  ```

  Result: `2 passed`.
- Passed final compilation and formatting gates:

  ```text
  $env:CARGO_BUILD_JOBS='1'
  cargo test --locked --no-default-features --features project-workflow-cli,presentation-2d,desktop-winit,render-wgpu --bin nara --no-run
  cargo fmt --all -- --check
  git diff --check
  ```
- Seven independent review lanes covered correctness, API contract, reliability, security,
  maintainability, testing and adversarial failure construction. The implementation resolved the
  public-command bypass, derived command count, missing predecessor binding, profile-preserving
  repair diagnostic, timeout cleanup loss, unsafe Git command rendering, invalid guide export parent,
  repair-result splicing and pre-launch executable substitution observation. No P0 or retained P1
  remains.
- A strict Clippy attempt exposed existing minimal-feature/dead-code lint debt outside this unit; it
  is not represented as a green gate here and did not change the product result.

# Follow-up

- This record completes FPER-U3 and the Fresh Project plan. It does not activate Editor work.
- Handle-bound executable launch and generalized terminal-control escaping remain delivery/security
  hardening topics. The TrustedLocal workflow now detects ordinary executable substitution through
  child-reported `current_exe` digest, but it does not claim a hostile local-process execution model.
- The persistent `target/nara-fper-product-cache` is verification build cache only. It is not product
  state or evidence authority.

# Citations

- `docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md`
- `docs/architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md`
- `src/project_workflow/repair.rs`
- `src/bin/nara/check.rs`
- `src/bin/nara/export/trajectory.rs`
- `starters/desktop-2d/v1/README.md`
- Commit `e37eb0f`
