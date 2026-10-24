# Retained Construction Owner Comparison

## Scope

This decision-local observation compares the existing explicit-patch preparation with retained
construction on the same uncommitted source based on `7868d48`, Windows, debug profile,
one Cargo build job and serial nextest execution. It is not release performance or native Editor
qualification. The source test is
[`retained_construction_prepare_and_commit_cost_comparison`](../../crates/nara_scene/tests/prepared_authoring.rs).

The original command used unpinned `cargo`; its compiler version was not captured at invocation.
A later inspection observes default stable Rust 1.99.0, so the original numbers must not be labeled
Rust 1.95.0 qualification. The pinned follow-up below supplies its own source/toolchain record;
the earlier evidence remains a historical observation, not MSRV proof.

Both cases begin with one existing entity and prepare two new root entities, one component per
new entity, mutual source-local references and one scalar replacement: seven expanded operations
and three property/field writes. Session admission, caller patch/draft construction, result
disposal, projection, Save and rendering are outside the timer. Retained preparation includes
fresh 22-byte ID allocation and property lowering. The explicit control uses fixed 13/14-byte IDs.
The inputs are semantically comparable, not byte-identical or equal work before preparation.

Each mode uses eight warmups and 64 measured samples. Modes run control-first, not randomized;
cache, scheduling and independent process noise are not isolated. P50 is the upper median at
sorted index 32 and P95 uses sorted index 60, matching the initial observation's convention.

## Pinned Follow-Up

Run `2b175295-820d-468c-879d-c20d2ae087c5` explicitly uses Rust 1.95.0
(`59807616e 2026-04-14`) after candidate-Schema reuse and owner-only sibling lookup. Its 18-file
LF source manifest is `cc44a9915e7fdafce3a98fa286d067d705d49a2adfef7c8a23b2d20f4563b073`;
the [qualification record](../knowledge/engineering/verification/2026-10/2026-10-24T224939Z-retained-source-construction-pinned-public-and-native-qualification-e1813acad6f04a93898a1e342989fa44.md)
defines the manifest and scope. The workload, timer exclusions and sample convention are unchanged.

| Mode | Prepare P50 | Prepare P95 | Commit P50 | Commit P95 |
| --- | ---: | ---: | ---: | ---: |
| Explicit control | 363,400 ns | 409,700 ns | 4,800 ns | 6,800 ns |
| Retained construction | 419,600 ns | 499,900 ns | 4,200 ns | 7,200 ns |

Retained preparation is about 22% higher at P95 in this observation. Both retained tails remain
below the original 735,000/10,200 ns investigation thresholds. Different compiler/source states
and unisolated scheduling/cache effects prevent causal comparison with the historical run; the
small commit-tail difference is not a demonstrated regression or speedup. The owner-only sibling
change is not exercised by this two-root workload, so no measured benefit is attributed to it.

## Historical Unpinned Observation

Nextest run `02898646-42d8-4837-8df1-98d4a93a309e` passes the workload and records:

| Mode | Prepare P50 | Prepare P95 | Commit P50 | Commit P95 |
| --- | ---: | ---: | ---: | ---: |
| Explicit control | 340,100 ns | 391,100 ns | 4,100 ns | 8,300 ns |
| Retained construction | 389,800 ns | 446,000 ns | 3,800 ns | 5,500 ns |

Retained preparation is about 14% higher at P95 in this observation. It remains below the design's
735,000 ns preparation investigation threshold; consuming commit remains below 10,200 ns.
The lower observed commit tail is not evidence that the retained wrapper makes publication
faster. Allocation, ID-length and lowering costs have not been independently attributed, and no
memory, throughput, frame-tail or overall tool-latency claim follows. Repeat and attribute before
acting on a noisy regression or claiming an improvement.

## Caller Responsibility Comparison

The current independent two-entity/two-component consumer lowers ten operations in either form.
Retained intent does not claim fewer construction/write calls. It removes five caller-owned
responsibilities from the explicit helper: choosing fresh persistent IDs, building source entity
records, copying template owner/version/value metadata, assembling structural/field patch variants,
and encoding new local references as stored IDs. The source owner now owns those steps.

The caller instead retains one bounded draft and opaque local handles, and may select local
parent/sibling placement. It can inspect the prepared patch/inverse/mapping and obtains actual IDs
only from successful publication, without parsing patches or predicting selection. This is a
responsibility and correctness improvement, not a demonstrated reduction in all author-visible
concepts. The direct guide and separately compiled consumer exercise that claim; registered-panel
prepared hooks and complete field queries remain separate R2 work.

## Reproduction

Run alone, serially, reusing the root target:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --locked -p nara_scene --all-features --test prepared_authoring --run-ignored only -E 'test(retained_construction_prepare_and_commit_cost_comparison)' --success-output final -j 1
```

Raw sorted sample arrays remain in the command output, outside tracked source at
`%TEMP%/nara-r2-construction-pinned-cost.log` for the pinned follow-up and
`%TEMP%/nara-r2-construction-cost-comparison.log` for the historical run. The
[construction design](../architecture/authoring-construction-design.md) owns the selected
contracts and investigation thresholds. This report does not close full R2 cost or ergonomics
qualification merely because one owner comparison passes.
