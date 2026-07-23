---
type: "Verification Evidence"
title: "RGD-U9 local first-playable measurement preparation verification"
description: "Records the committed non-decisive U9 plan helper, policy boundaries, and focused local verification without claiming a product baseline."
timestamp: 2026-07-23T01:30:33Z
record_id: "7d09e5953e7f4d50862ac7645438c9be"
tags: ["rgd-u9", "measurement", "first-playable", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bcbc452ef29d2fa734a01100448ce589472a4c4f"
verified_by: "Focused nextest, direct Python catalog check, formatting, diff validation, and staged-scope review"
---

# Scope

This evidence verifies only RGD-U9's locally admissible preparation: a cross-platform planner,
the accompanying policy tests, and the first-playable baseline instructions. It does not execute
the U14 workflow or record an observed baseline.

# Result

- `measure_first_playable.py plan` accepts only a clean repository root, removes inherited
  case-insensitive `GIT_*` overrides for its Git proof, writes a new external plan directory, and
  never runs Cargo, creates a worktree, or emits a product verdict.
- The planner reads the committed U14 catalog and requires exact coverage of all 20 U14 metric
  requirements. It records each metric's canonical minimum sample count instead of accepting a
  misleading global count.
- Public headless, body/data/structural edit, build, desktop, and coverage workflows are named.
  The missing public module-addition task, window-slot task, and desktop pressure telemetry are
  recorded as blockers. Packet batch, instance, retained-byte, and clone-cost visibility remains
  explicitly unavailable.
- The desktop command activates the required `desktop` feature. A catalog/workflow mismatch,
  dirty subject, output inside the subject, or inherited Git-location override rejects before a
  plan can be mistaken for evidence.

# Verification

- `python -B -c '... load_metric_requirements(Path(".")) ...'`: passed; the committed catalog
  yielded 20 U14 requirements and `frame.p99_ns` retained its minimum of 1000 samples.
- `python -B reference-game/tools/measure_first_playable.py --help`: passed.
- `rustfmt --edition 2024 --check tests/measurement_helpers.rs tests/measurement_policy.rs`:
  passed.
- `git diff --check -- <five U9 paths>`: passed.
- `cargo nextest run --locked -p nara --features runtime-core,serde --test measurement_policy
  --test measurement_helpers --build-jobs 1 --test-threads=1`: passed, 21/21 tests. The run used
  the shared repository target with one build job and one test thread after other local Cargo work
  had finished.
- The staged commit audit contained exactly the U9 helper, baseline document, protocol note, and
  two focused test files. No concurrent architecture, strategy, or research change was staged.
- `architecture_docs` was intentionally not run under the user's instruction; U9 does not modify
  architecture governance authority.

# Remaining Boundary

RGD-U9 remains active. U8 must first provide a final hosted Windows/Linux CI matrix for the
integrated revision. Only then may a separately admitted collector execute the listed workflows,
preserve raw and failed samples, and produce a compact reproducible baseline. This preparation
does not claim headless or desktop success, timing, memory, GPU, packet, build, module, or slot
measurements.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- `docs/benchmarks/reference-game-first-playable-baseline.md`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- Commit `bcbc452ef29d2fa734a01100448ce589472a4c4f`
