---
type: "Verification Evidence"
title: "Paused presentation no longer rewinds and late display completion cannot replace its successor"
description: "Verification Evidence for Paused presentation no longer rewinds and late display completion cannot replace its successor."
timestamp: 2026-10-15T20:02:46Z
record_id: "114c2cfd30854192a821c22b8afd8487"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7718a86"
---

# Verification

- Actual opt-in extraction red: `cb0587b6-cf9e-4a3f-82ea-851cb5c81de7`, five tests, four passed
  and one failed. A paused fixed Step displayed x=16; Resume with no authoring edit displayed x=12.
- Transform/render/sprite libraries and extraction consumer after the correction:
  `ea60195c-f02b-4f75-aefc-bb80c27bd5da`, 75/75 passed, 4.236s after 27.38s compilation.
- Existing real Host CPU toolkit test with late display completion:
  `8bac4b1e-486f-482e-b986-a8c8c76f83e8`, 1/1 passed, 0.186s after 22.39s compilation.
- Affected transform/sprite Clippy correctness/suspicious gate: session `24511`, exit 0, 16.41s;
  `cargo fmt --all -- --check` also passed.

# Result

Paused extraction now resets previous/current local samples to the displayed current value.
Resume cannot expose a historical sample before the next real tick. This changes only the
existing transform owner's presentation history and never writes simulation components.

The CPU adapter submits two actual Host candidates, completes the successor first, and attempts
the delayed predecessor. The presentation owner rejects it with ImageMismatch while the current
token, displayed pixels and picking remain valid. No production guard was added or relaxed for
the coverage case; it exercises the existing monotonic completion rule.

# Evidence

The prior paused consumer edited a transform before Resume; that edit reset history and masked
the standalone paused-Step path. The added consumer isolates that path and physically retires
the Runtime before assertions, so the original red remains a readable value mismatch.

The late-completion case came from the full testing review's coverage candidate. Its first build
failed because the test lacked the existing error type import; the corrected test then passed.
The commit is `7718a86`; original staged changes remain byte-identical. The full review receives
this delta and evidence without treating a primary-agent fix as independent corroboration.

# Follow-up

Finish the full review, validate any remaining findings, and close the active plan's evidence.
The review's platform and untracked-file coverage limits remain explicit.

# Citations

- crates/nara_transform/src/presentation.rs
- crates/nara_sprite_render/tests/presentation_interpolation.rs
- tests/support/fake_preview_toolkit.rs
- src/project_host/runtime/editor/presentation.rs
