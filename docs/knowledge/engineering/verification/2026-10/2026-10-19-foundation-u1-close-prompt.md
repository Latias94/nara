---
type: Verification Evidence
title: Foundation U1 close prompt identity
description: Close decisions retain exact prompt ownership across frame publication and reject stale replay.
timestamp: 2026-10-19T08:21:22Z
record_id: ef0aa5889cb54942b8a63b71354b383e
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
git_commit: 2713396
---

# Scope And Result

U1 landed in `2713396`. The Host publishes an opaque prompt token for the current document
decision. Resolve carries and consumes that token. Document advance, Cancel/new intent and failed
Save invalidate old decisions; ordinary frame publication keeps a still-pending token stable.
The native Dear ImGui prompt displays the same frame's document title and source binding.

All in-repository action producers were migrated, including reference-game and external panel
fixtures. A stale Save cannot affect replacement handoff, while a validated Save that immediately
fails persistence retains the existing replacement cancellation behavior. ADR 0047 records the
prompt ownership contract. An independent static correctness review found no actionable defect.

# Verification

Root ran Cargo serially on Windows with Rust 1.95.0 and the existing target directory.

| Evidence | Result | Nextest run |
| --- | --- | --- |
| Duplicate A Discard batch before token enforcement | Failed as expected: a second decision accepted against B; unwinding also aborted the failing test process | `ac374531-dcec-47aa-a6d1-6a22e5dd77b3` |
| Core replay and Scene/Prefab retarget regressions after implementation | 4 passed | `d98827c4-1192-4fcf-8d67-cb6a7d482028` |
| Real Dear ImGui prompt identity, stale replay and Saving controls | 3 passed | `342ee260-38f7-4d0d-8f73-e3e376853079` |
| Editor application, registered panels and generic component integration | 18 passed | `ce5b950c-c50c-4f70-a146-989f113fdfea` |
| Minimal-feature persistence and project content/runtime boot | 33 passed, including compile-fail persistence authority checks | `74d3fb59-350b-4b6f-a89f-77c386bbc798` |
| Core regressions with explicit idle-frame token stability assertion | 4 passed | `4a852b77-5ec3-48c4-b6c2-bc9a7fb5fc35` |

The last two runs used `--no-default-features --features tooling,runtime-2d,serde` with the three
named integration binaries or `--lib` and the four close/retarget tests. Earlier Editor integration
runs used `-p nara-editor` and the named test binaries. Overlapping totals are not unique test counts.
`cargo +1.95.0 fmt --all` and focused `git diff --check` passed before commit.

# Limits And Continuation

An attempted root all-features integration build hit MSVC LNK1140 (PDB limit); the minimal-feature
retry then hit disk exhaustion. Generated Cargo artifacts were cleaned and the relevant minimal
feature suite passed. Neither failed build is reported as a behavioral test failure or a completed
all-features gate. The final workspace check and external-consumer cohort remain required by the
broader active plan. This record does not claim a new complete native Editor release qualification.
