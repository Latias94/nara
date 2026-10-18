---
type: Work Progress
title: Foundation U3 effective values and reference closure
description: Effective default materialization, scene/prefab/export closure, asset collection limits, and Inspector projection landed after U2.
timestamp: 2026-10-18T05:39:59Z
record_id: foundation-u3-effective-closure-20261018-053959
producer_id: codex-root
run_id: foundation-contract-refactor-20261018
git_branch: refactor/foundation-contract-hardening
related_plan: docs/plans/2026-10-18-1105-refactor-foundation-contracts-plan.md
---

# Landed Contract

Commit `53586dc` completes U3's effective-value and current reference-closure slice after U2's
recursive shape contract. `nara_reflect` owns one bounded schema-only normalizer with explicit
logical node/byte/depth limits. It returns a borrowed raw value when no default is inserted and
moves owned values without rescanning them. Expansion preflights raw values, defaults, shared Map
ancestors, List suffixes, map-key bytes and depth before cloning or mutation. Required presence is
separate from explicit nullable Null and target existence; unknown/unrelated required fields remain
the full validator's responsibility.

Registry native preflight, structural validation and whole-record asset collection consume the same
effective candidate. Asset collection accepts separate materialization and reference-work limits.
Scene publication carries selected `SceneFileLimits` through canonical candidates. Native Scene
preflight checks effective SceneLocal references against the complete current namespace; Shape and
intermediate patch checks skip target closure. Prefab source patching has a private source-namespace
seam so remap errors are reported before expanded-document validation. Prefab defaults are applied
after migration and before aggregate materialization/reference accounting. Export normalizes encoded
owned values, rewrites references, and charges the final remapped aggregate value budget without
mutating World. Inspector projects effective defaults while preserving raw omission, explicit Null,
edit commands and undo history; authenticated KnownUnbound records are read-only projections only.

F05 is fixed for effective-value interpretation and current local/asset closure. U4 still owns final
patch asset closure, and U13 owns durable unloaded/deleted residency outcomes. These boundaries must
not be collapsed into a general resolver or budget framework.

# Verification

All commands used `CARGO_BUILD_JOBS=1` and the existing root `target` serially on Windows/Rust 1.99.

| Scope | Result |
| --- | --- |
| Reflect U3 | 174 nextest tests passed: `793660df-70bb-4946-80d8-0c6fb79ed420`; effective-value tests cover defaults, Null/presence, shared ancestors, contiguous List insertion, sparse rejection, raw/opaque fast paths, budgets, native callback ordering and partial asset records. |
| Scene/Prefab/Export | 129 nextest tests passed: `87625d56-60c0-4a9b-b566-48f6ee94da39`; includes 6 new effective-reference tests, exact export aggregate budget, canonical SceneFileLimits propagation and current local target rejection. |
| Tooling | 47 nextest tests passed: `6033f4db-1173-437f-9991-3a39349025d7`. |
| Root Inspector | 10 nextest tests passed: `fe494c67-d19c-4b9e-a29e-1f3563441c09`; Defaulted projection, explicit edit, raw omission and undo are covered. |
| Workspace compile | `cargo check --locked --workspace --all-targets --all-features` passed after final formatting. Existing deprecation/dead-code warnings remain outside this unit. |
| Formatting | `cargo fmt --all` and `git diff --check` passed. |

# Remaining Work

U12 has an untracked ignored measurement harness and JSONL summarizer under
`target/foundation-refactor-plan-research`; it has not been executed or committed. U4/U13 remain
active plan units. The external HTML audit is outside the repository and now records F05 as fixed
with nine fixed findings; visual browser qualification remains unavailable.
