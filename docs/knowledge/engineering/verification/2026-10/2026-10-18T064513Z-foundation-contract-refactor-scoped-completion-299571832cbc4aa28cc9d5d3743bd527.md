---
type: "Verification Evidence"
title: "Foundation contract refactor scoped completion"
description: "Final closure, durable references, real multi-owner reconciliation and measured scale evidence complete the required foundation slice."
timestamp: 2026-10-18T06:45:13Z
record_id: "299571832cbc4aa28cc9d5d3743bd527"
producer_id: "codex-root"
run_id: "foundation-contract-refactor-20261018"
related_plan: "docs/plans/2026-10-18-1105-refactor-foundation-contracts-plan.md"
git_branch: "refactor/foundation-contract-hardening"
---

# Result

The required units in the foundation contract plan are complete. All six P1 findings and ten of
seventeen findings overall are fixed; five consumer-gated P2 extensions and two scale boundaries
remain explicit. This is scoped foundation completion, not native Editor or cross-platform release
qualification. The goal did not add Tilemap, physics/audio backends or a public execution/render graph.

Final commits after U3:

- `4c0b49f`: actual-device Surface extent admission before configuration or cache publication.
- `784f4ab`: U4/U13 final candidate closure, selected materialization policy, direct authoring
  namespace and typed unloaded/deleted export outcomes, with an independent durable consumer.
- `237997b`: actual threaded Image/Prepare plus hook/RequiredComponents native-owner consumer.
- `f6f3901`: reproducible ignored scale/allocation harnesses, compressed raw data and measurements.

# Evidence

All Cargo work ran serially with `CARGO_BUILD_JOBS=1` and the existing target directory. Individual
rows overlap in coverage and must not be summed into a unique-test count.

| Scope | Result and run |
| --- | --- |
| Reflect + Scene with serde | 361 passed; `8362d6f1-4356-4026-8ca8-a1f52b8d2047`. |
| Independent durable consumer | 7 passed; `bfa5fc27-3050-4ff5-851c-b385a35bbe29`. Required/optional and explicit/default nested UUID values cover file publication, spawn, repeated Prefab, patch, strict export and stale-incarnation commands. |
| Root authoring/file/Inspector/identity consumers | 53 passed; `482f8a6b-522c-41b0-af94-6c9427b952aa`. |
| Independent real Image/native consumer | 8 passed; `65465bef-c952-4b29-b962-1fc049668652`. Includes pending latest revision, native Retry, unload/late completion, removal, immutable snapshots and physical Stop/CloseIncomplete/RetryClose. |
| WGPU all-feature suite | 107 passed, 2 skipped; `b963f04b-5672-41d2-a7dc-5e27672ff60a`. The device-limit test avoids a live maximum-size Surface allocation. |
| Editor authoring loop | 1 passed, 1 skipped; `d83abdc9-5340-4e8c-ac65-a0fe104ea495`. Unsaved source and explicit Play lifecycle pass; native release qualification remains skipped. |
| Reference game | 34 passed, 0 skipped; `ca80ff09-f007-4b14-8de2-7dbec1daea0b`. `authoring`, `desktop_render` and `first_wave` run with `--features desktop`. |
| Root Host close and successor receipts | 3 passed; `909aa131-5a26-4992-8a27-c73a478b7590`. Complementary host evidence does not make the external fixture a proof of every host combination. |
| Scale timing | Three hierarchy processes and one Transform process pass complete independent correctness oracles; run IDs, protocol and raw hashes are in the dated benchmark report. Stress hierarchy rows have insufficient samples for P99. |
| Isolated allocations | `4b711344-1aff-4c2d-9616-badbb4164d5c`; exactly the allocation test selected, two imported ignored timing tests skipped. All 32 windows have consistent counters and stable adjusted requested heap over four blocks. |
| Workspace | `cargo check --locked --workspace --all-targets --all-features` passed in 46.42 s. Existing deprecated/dead-code warnings remain. This is not execution of every workspace test. |
| Hygiene and report | Workspace and independent-consumer formatters and diff whitespace checks pass. Five compressed evidence archives verify against raw SHA-256 receipts. HTML has 17 matching finding IDs, 75 existing local targets and no invalid anchors, duplicate IDs or external assets. Visual/print qualification is not claimed. |

# Decisions Worth Preserving

Direct authoring records use the source document namespace even if an unrelated Prefab anchor is
present. ADR 0038 rejects treating generated projection IDs as durable source identities. Do not
restore anchor-wide deferral or parse opaque ID prefixes to bypass this closure.

The U9 fixture originally expected ordinary ImagePlugin import to retain the Editor pixel budget.
That expectation was wrong: ordinary `commit` releases work admission at publication; Editor
`commit_retained` is a separate policy. The corrected consumer proves immutable pixels and balanced
transient work admission, not a general resident-memory bound or a production memory fix.

Allocation instrumentation is isolated from the timing executable. Hierarchy windows destroy
30,208 bytes of preallocated input: add that amount back to raw negative live deltas before
interpreting retained growth. Warmed Transform static/one-leaf windows observe zero allocations;
Hierarchy batch-eight requests 1,487,768 bytes per cycle versus 11,428,052 for eight independent
transactions. Finite requested-heap windows are not RSS, GPU memory or general leak qualification.

# Follow-up

- F11: before a real large-world multi-producer consumer, attribute candidate copying and settle
  transaction cost. At 4,096 members, independent-eight P95 is 4.41–5.13 ms versus batch-eight
  1.04–1.06 ms across three processes. Never silently combine independently atomic transactions.
- F12: retain the current correct Transform implementation. A concrete consumer above the 2 ms
  P99 / 4,096 sparse-participant investigation threshold in two of three complete processes
  triggers attribution; one current process does not establish that trigger.
- U7/U8: reopen only for a real independently extensible repeated substep or same-phase
  ancestor-write/current-child-pose consumer that existing managed sets cannot express.
- U10/U15: reopen for shared Runtime/Editor logical-canvas multipass or actual non-default GPU
  capability demand. Current direct-window and Editor image/IntegerFit contracts remain admitted.
- U9: production physics/audio and broader successor compositions need their own consumer proof.
  Do not extract a universal reconciler from two fixtures.

The standalone HTML remains outside the repo; the audit records its Downloads location. Unrelated
`myeasylog.log` and `vendor/` were preserved; `repo-ref/` was read-only.

# Citations

- [Foundation plan](../../../../plans/2026-10-18-1105-refactor-foundation-contracts-plan.md)
- [Audit and complete validation register](../../../../audits/2026-10-18-foundation-contract-audit.md)
- [Scale report and retained datasets](../../../../benchmarks/2026-10-18-foundation-scale.md)
- [ADR implementation ledger](../../../../architecture/adr/implementation-status.md)
