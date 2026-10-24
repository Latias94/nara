---
type: "Verification Evidence"
title: "Retained construction owner public and lifecycle regression checkpoint"
description: "Source-owned retained construction passes owner, independent Application, lifecycle and root/Editor regressions; final static review and selected native qualification remain open."
timestamp: 2026-10-24T21:55:22Z
record_id: "82b171c17ee24213934e375e976c3e0c"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7868d48072a893b5a9f176d93d4597d0a5ecb56a"
---

# Verification

Successor to the [owner/public adapter checkpoint](../../progress/2026-10/2026-10-24T214058Z-retained-construction-owner-and-public-adapter-checkpoint-a65bea496a9442b18628572e20667490.md).
Source remains uncommitted on `7868d48`; no claim of completed R2, delivery wave or foundation Goal.
The final 16-Rust-file LF manifest is
`4833c8787c0f68cfa191d93bb7ac7da4db038432bf62f7bfa466ed9748535f2f`.
It covers `git diff --name-only` Rust paths plus both new construction modules, sorted by path.
Each entry is `{path,sha256}` over CRLF-to-LF normalized bytes; the composite is SHA-256 of UTF-8
`json.dumps(entries, sort_keys=True, separators=(',', ':'))`.

# Result

Owner construction and direct Workspace/Session/Application routing are implemented and pass
their non-native regressions. The independent consumer uses exact returned IDs, two components,
local parenting and mutual references on non-active Scene/Prefab documents. It checks nonempty
selection preservation, last-invalid refusal, dropped candidates, one Undo/Redo, Save/Reopen,
stale rejection and historical receipt lifetime. Completed Close, same-raw-slot project replacement
and physical retirement all reject old candidates without returning a mapping.

The source owner reuses existing validation, inverse, replay and history admission. New tests
cover unavailable/staged same-key refusal, exact resolved local-reference byte charging before
overwritten-root coalescing, the combined declaration/root 256/257 boundary and retained Reopen.
The root regression directly obtains candidates from Workspace to bypass Host prepare, then
proves Host commit still refuses both adding structure to an existing UI Prefab and introducing a
UI component through a template into an ordinary Prefab. Source/revision/history remain unchanged.

# Evidence

All Cargo commands ran serially with Rust 1.95.0, `CARGO_BUILD_JOBS=1` and the shared root `target`.

| Executed scope | Nextest run ID | Result |
| --- | --- | --- |
| `-p nara_scene -p nara_tooling --all-features` | `7d94aad5-fada-4d49-869e-9075af7a2519` | 392 passed, 3 skipped |
| Independent `editor-panel-consumer::prepared_authoring` | `3b711195-b730-4747-bd82-d99a48939081` | 4 passed |
| `-p nara -p nara-editor --all-features --lib` | `99beb87a-a363-4c3a-96ad-239561dd62db` | 581 passed, 28 skipped |
| `-p nara --no-default-features --features runtime-2d,tooling,serde --test editor_ui_domain_feature_off` | `624911ad-39b3-46e0-820c-e17124e79225` | 1 passed |
| Ignored retained/control cost comparison | `02898646-42d8-4837-8df1-98d4a93a309e` | 1 passed |

The strengthened public lifecycle test first timed out after a cancelled Close because it did not
acknowledge the retained terminal intent result before requesting another Close. The test now
acknowledges it and asserts admission before waiting; production intent policy was unchanged.
Run `d3bf9099-5ee9-4ff2-9e96-56658709d165` records that failure. It was not a constructor defect.

The feature-off test's pre-existing fixed 128-poll busy loop outran asynchronous source indexing
on the cold feature combination. Run `a3c2a0c4-367a-4274-a68e-3bbc109c9dca` rejected Browser Open
with `project.editor.document-index-unavailable`; the test panicked before the new assertion and
aborted during cleanup. It now waits on actual source-index/Open progress with a finite deadline
and short sleep. The passing successor exercises the constructor's specialized UI gate with the
UI runtime feature absent; no production retry or admission bypass was added.

The [cost report](../../../../benchmarks/2026-10-25-retained-construction.md) records debug retained/
control prepare P95 446,000/391,100 ns and commit P95 5,500/8,300 ns. It states timer exclusions,
different ID lengths and unisolated scheduling/cache effects; it does not claim a commit speedup.
The caller-responsibility comparison does not pretend fewer operations mean better ergonomics.

Root and fixture formatting completed before these runs. Changed Markdown local links and wiki
validation passed, with existing legacy-memory warnings; subsequent evidence shards require a
fresh rollup render. Existing compiler warnings remain. Logs stay outside tracked source under
`%TEMP%/nara-r2-construction-*`.

# Follow-up

Unified exec sessions 69029, 11097 and 21040 are terminal. Do not restart or poll them. All three
subagents from this turn are closed; there are no live Cargo jobs or agent handles to resume here.

Finish scoped static compilation/correctness lint, simplification/fresh final review and relevant
selected native regression before claiming full qualification or committing this unit. No fresh
CE receipt exists for this construction diff. Native evidence for ordinary creation at `7868d48`
must not be promoted to this implementation. Do not rerun the long source-carrying starter export
merely because it previously existed; match new evidence to this change's affected scope.

The [next-unit query investigation](../../subagents/2026-10/2026-10-24T215152Z-r2-selection-independent-field-reading-seam-389b97a2cf8f4275abbae7eeaa84bc6d.md)
identifies the remaining component/root-value read -> computed edit workflow. Registered-panel
prepared hooks and complete R2 remain open, as do the later foundation waves. Preserve the
untracked `vendor/`, existing source/runtime incarnation contracts and local-only delivery.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Construction design](../../../../architecture/authoring-construction-design.md)
- [Source implementation](../../../../../crates/nara_scene/src/authoring/construction.rs)
- [Workspace implementation](../../../../../crates/nara_tooling/src/workspace/construction.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
