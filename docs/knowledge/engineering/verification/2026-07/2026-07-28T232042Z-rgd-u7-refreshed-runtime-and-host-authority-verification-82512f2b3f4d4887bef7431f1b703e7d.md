---
type: "Verification Evidence"
title: "RGD-U7 refreshed runtime and host authority verification"
description: "Closes the refreshed Runtime-first and dependent Host compatibility review after the direct fault-bridge bypass repair."
timestamp: 2026-07-28T23:20:42Z
record_id: "82512f2b3f4d4887bef7431f1b703e7d"
tags: ["rgd-u7", "runtime", "host", "authority", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "27cbd1298c8cefd470efc25a4d99396305b5b4ea"
verified_by: "Serial nextest, targeted strict Clippy, source-boundary review, and independent correctness review"
supersedes: "38a8bf4d48614a829bdd6388f02c9446"
---

# Verification

RGD-U7 re-reviewed ADR 0084 before its dependent ADR 0082 against the refreshed executable
authority. The first review baseline, `088e233b4b80f1cafc6f56997751ebbfccc4d77c`, exposed a P1
direct-App fault-bridge bypass and therefore did not close the unit. Implementation commit
`27cbd1298c8cefd470efc25a4d99396305b5b4ea` closes that bypass and is the exact final Runtime,
Host, and compatible-pair review subject.

# Result

- `App` freezes the canonical `RuntimeFaultReporter` and selected Bevy fallback error handler at
  first execution. Final identity, structural revision, and rolling change epochs are all required.
- Remove, replace, remove/reinsert, and normally change-detected temporary mutable
  replacement-and-restore reject through a sticky canonical Runtime fault on direct and managed
  paths.
- Rolling epochs cover built-in schedules, custom schedules, custom runners, direct `run_once`,
  candidate scopes, published driver scopes, and Bevy change-tick maintenance without depending on
  exact raw tick equality.
- The integrity claim covers structural mutation and ordinary writes tracked by Bevy change
  detection. Explicit `bypass_change_detection`, manual tick rewriting, unsafe/raw ECS mutation,
  and equivalent Host-trusted escape hatches are outside this guarantee.
- A real fallible system proves that the selected handler reaches the canonical reporter before
  execution can be accepted as healthy.
- The final Runtime-first review retains ADR 0084 as Accepted for already-compiled, Host-trusted
  paths. The dependent review then retains ADR 0082 and the compatible pair without admitting a
  universal Host, service registry, package activation model, replacement Render Host, or Runner
  SPI.
- Current-revision hosted CI and every candidate/approval/publication action remain downstream
  gates; this record does not authorize an external mutation.

# Evidence

- `cargo nextest run --locked -p nara_app --test-threads=1`: 84 passed.
- `cargo nextest run --locked -p nara --test runtime_instance --test runtime_runner_contract
  --test runtime_driver_boundary --test-threads=1`: 81 passed.
- `cargo nextest run --workspace --locked -E 'not binary(architecture_docs)'
  --test-threads=1`: 1,091 passed, 3 skipped. Run ID
  `96ca52be-e3e2-48b9-b574-1b4c4b7066d0`; the owner-excluded documentation binary is not claimed.
- `cargo clippy --locked -p nara_app --all-targets -- -D warnings
  -A clippy::result_large_err -A clippy::collapsible_if`: passed with only the explicitly named
  pre-existing lint classes allowed.
- `cargo fmt --all` and `git diff --check`: passed.
- Independent review iterated over temporary replacement, custom-runner duration, Bevy change-age,
  and `CheckChangeTicks` observer failure cases. The final pass reported no P0/P1; its one P2 scope
  wording finding is reflected in the bounded integrity claim above.

# Follow-up

1. RGD-U8 refreshes the ordinary hosted Windows/Linux root, reference-game, and module-consumer
   matrix at the final integrated executable revision.
2. Candidate dispatch, evidence ingest, approval, tag, draft, release, and publication remain
   unauthorized by this record.
3. The dependency correction lane remains separate and incomplete until its four plan gates prove
   the production-edge graph, hierarchy/transform/UI consumers, the `nara_reflect -> nara_asset`
   deletion test, and both direct and renamed-root consumers.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u7-re-review-runtime-then-host-authority`
- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/knowledge/engineering/decisions/2026-07/2026-07-28T214815Z-rgd-u7-refreshed-runtime-and-host-independent-decision-matrix-cb08ecb6f5054f938f8a6d7de30941e4.md`
- Commit `27cbd1298c8cefd470efc25a4d99396305b5b4ea`
