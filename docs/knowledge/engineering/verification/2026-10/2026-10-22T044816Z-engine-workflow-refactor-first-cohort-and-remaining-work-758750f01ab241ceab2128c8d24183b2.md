---
type: "Verification Evidence"
title: "Engine workflow refactor first cohort and remaining work"
description: "U1 U2 U3 U4 U9 implementation evidence before native Scene and authoring extensions."
timestamp: 2026-10-22T04:48:16Z
record_id: "758750f01ab241ceab2128c8d24183b2"
producer_id: "codex-root"
run_id: "engine-workflow-2026-10-22"
related_plan: "docs/plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md"
git_branch: "test/compiled-asset-domain-export"
---

# Verification

Local first cohort for the [workflow plan](../../../../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md).
All Cargo commands were sequential, reused `target`, and used `CARGO_BUILD_JOBS=1`.

# Result

U1 routing, U2 tint batching and initial target residency preflight, U3 close semantics before
module extraction, and U9 task admission passed focused tests. U4 diagnostics and final runtime
extraction still require verification. U5-U8 and the catalogue measurement remain in progress.

# Evidence

| Command scope | Result | Nextest run ID |
| --- | --- | --- |
| UI `nonmodal_` | 3 passed | `11f1300a-902f-45f9-904e-ce97d2f934d9` |
| Sprite tint regression | 1 passed | `3a081975-c395-46bf-ac72-46c57b6a59b7` |
| WGPU default | 59 passed | `d90a51c1-4364-4a6e-9be5-ee1649a52232` |
| WGPU sprite/UI submitters | 135 passed, 1 skipped | `d17be9d1-a775-45fb-9261-f8c814625669` |
| App public `runtime_close` integration | 3 passed | `e1a19354-b903-4f84-adcd-21da7a67b981` |
| UI/Input/Sprite/Tasks libraries | 212 passed | `a95c7602-c4c1-4246-b41d-744f53bfe059` |

The 512-job unpressured admission regression counted zero queued-node visits. The old rebuilding
loop would visit 130,816 nodes for that burst. This is an operation-count result, not a timing or
tail-latency benchmark. Cancellation pressure and same-key coalescing passed in the same task suite.

The initial App integration tests assumed Stop and RetryClose finish on the first `drive`; those
calls only accept the request. Their assertion panic left an unfinished runtime in thread-local
quarantine and reached the existing fail-fast guard at test-thread exit. Adding the required next
drive made all three integration tests pass. Windows exit `0xc0000409` did not establish memory
corruption in this case.

The first extraction build found a stale capacity-constant reference and a rustc 1.99.0 incremental
fingerprint ICE. The reference was corrected; only `cargo clean -p nara_app` was used before retry.

# Follow-up

Rerun App after extraction, U4 Check/Open with required features, and the updated WGPU complete-target
suite (partial-cache and namespace-clear fixes followed its first green run). Independent review
found RON `SpannedError` also carries Serde data failures; syntax classification now uses explicit
lexical/delimiter codes and needs header/payload contrast tests. Do not treat this record as final
closure of U1-U4 or the complete plan.

# Citations

- [Source review](../../../../audits/2026-10-22-engine-first-principles-source-review.md)
- [Workflow plan](../../../../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md)
