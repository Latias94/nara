---
type: "Verification Evidence"
title: "Explicit Cargo builds captured executables and supervised protocol qualification"
description: "Cargo-authoritative target selection, captured input and executable binding, exact bounded Editor protocol and real native consumers."
timestamp: 2026-10-31T03:50:34Z
record_id: "4cfff5128fa64423b6f234ea7b312c63"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-explicit-cargo-build-selection-plan.md"
git_branch: "refactor/foundation"
git_commit: "0c36715"
---

# Verification

Qualify the finite explicit Cargo/captured executable outcome under ADR 0110 and supervised
machine-output custody under ADR 0116. The starting branch and main were `0c36715`; implementation
was uncommitted during execution. The 35 final changed/new Rust files, ordered by repository path
and hashed as path, NUL, bytes, NUL, have SHA-256
`e15cf08498f0b2c39719209854d2dc3804122585ee6cfcc73280c651bf13d2e7`.
The obsolete `cargo_facts/desktop_build.rs` is separately deleted. Windows 11, Rust 1.95.0,
CARGO_BUILD_JOBS=1, serial Cargo and the existing root target were used for all workspaces.
Selected actual Windows/DX12 native trajectories executed. This is not full R7/foundation,
all-native/platform, packaged Player or reproducible executable qualification.

# Result

`project_workflow::cargo` supplies pure explicit package/bin/profile/triple/default-feature and
requested-feature facts. Cargo owns resolution and eligibility. Workspace metadata features are
a separate observation; selected compiler-artifact features establish the actual build result.
Dependency-qualified required-features remain declarations, not a Nara feature resolver. Built-in
test/bench output directories follow debug/release. Fixed Desktop/Headless decoders and duplicate
bootstrap/Export selectors are removed; starter policy stays in adapters.

`project_tooling::cargo_build` owns finite trusted native execution, actual selected tools,
bounded interpretation, per-file declared-input revalidation and exact captured output bytes.
One cooperating output-directory lease extends through capture and all unconfirmed child cleanup.
Nested cleanup retains every original lease. Tool file bytes, identity stdout and output payload
have separate allowances. Captured clones share bytes; original Cargo path replacement cannot
retarget them. Export binds each captured role's original input path/digest inventory to its
preparation and retains captured bytes through staging. Product/startup agreements remain separate.

The Editor keeps its supervised cancellation, generation, Ready and last-good owners. Build stdout
now has exact cumulative protocol capture independent of lossy stderr/display logs. Small reads
coalesce into at most 16 KiB chunks; consumption does not replenish the byte allowance. Normal
stdout/log delivery alternates when both are pending. Fatal stdout loss and reader failure precede
normal data. Readers never wait for consumer capacity. Physical reader return and output drain are
separate completion conditions, with the existing 64-record poll budget and cleanup custody.

The native maintenance oracle now compares the actual earliest owner deadline using the same
Host scheduling clock, while independently requiring process deadline progress, three due quiet
cycles, no redraw/output/presentation deadline and actual exit presentation. It permits legitimate
Scene-preview rechecks. Native retirement still runs before production scheduling time is sampled;
the private post-retirement helper is not a new clock service or public frontend contract.

# Evidence

| Check | Actual result |
| --- | --- |
| Final root/Editor Cargo, Export, process, generation and output owner cohort | 151 passed, 1287 skipped; `0ccb9ca4-4405-4a10-b183-e913843aa795` |
| Final deadline/maintenance algorithm tests | 6 passed, 297 skipped; `dae896dd-0245-4456-8cd4-d3e38b075645` |
| Independent actual custom Host/Player and foreign WASI build | 2 passed, 0 skipped; `471a2420-4501-4047-af91-513582390446` |
| Original renamed-root processed Input process after fixture extraction | 1 passed, 0 skipped; `a8772a76-5a81-4632-8225-48f8e78922db` |
| Real source CLI Check/Run/Repair/Export and relocation | 1 passed, 7 skipped, 1178.403s; `bb1e1fea-6c43-4299-ba5b-33b54114fbd9` |
| Final actual Windows/DX12 supervised last-good/successor and external quiet/exit trajectories | 2 passed, 301 skipped; `cff8e813-7f40-4650-8e2e-45c3d7437002` |
| Workspace all-feature/all-target correctness Clippy | Passed; existing non-correctness warnings retained |
| Final affected root/Editor all-feature/all-target correctness Clippy | Passed after canonical v1 and equivalent style cleanup |
| Independent Cargo build consumer all-target Clippy `-D warnings` | Passed |
| Root project-workflow-only and cargo-facts-only library checks | Passed |
| Root, independent Cargo consumer and original runtime-runner formatting | Passed |

The actual consumer generates an ordinary workspace with custom package/bin/profile, same-bin
decoy, workspace-only metadata activation, dependency-qualified eligibility, native Host/Player
and a foreign WASI target. It proves exact tool allowance and one-short refusal independently
of a 1 MiB output limit. A failed B, stale BeforeReturn source, rejected reentry and overwritten
Cargo output preserve captured A. After source removal, the relocated Player runs all seven
original processed physical-input/missing/malformed/wrong-identity/conflicting/wrong-digest/
wrong-target cases with Cargo absent from the run path. Foreign output is captured but never run.

Owner tests additionally cover strict decoder poisoning/limits, actual feature facts, output
directory mappings, source inventory omissions, both stale Export roles, changed inventories,
original output overwrite/removal, real direct-child cleanup faults and nested lease custody.
The real process burst writes 1 MiB stdout and 1 MiB stderr before the first owner poll; exact
stdout survives, stderr loss stays explicit, first-batch diagnostics remain visible, each batch
is bounded and physical cleanup completes. Queue tests cover tiny reads, lifetime cumulative
limits, one-byte overflow, cancellation, noisy-stderr reader failure and multi-poll drain beyond
the former reader deadline. No existing threshold was widened to make a failing case pass.

Native last-good qualification uses actual UI Play controls, failing project Rust and successor
generation, real child READY/presentation and original cleanup. The cadence trajectory performs
three quiet process-maintenance cycles, then observes and presents natural child exit. These
selected trajectories do not replace the entire native critical group or qualify Linux/macOS,
monitor scanout, OS-injected input or graceful Runtime teardown after process exit.

The source CLI and public execution cohorts preceded final equivalent Clippy cleanup/canonical
invocation-domain reset. Final owner, feature compilation and actual native runs cover the final
source fingerprint. Default CLI tool/output allowances remain the same 512 MiB; its later
separation allows callers to select them independently. Source Export still carries authored
content; these results do not turn it into the next cooked package workflow.

Independent reviews closed actual-tool/proxy binding, object identity, callback/reentry output
attribution, allocation/custody, original input binding, metadata/compiler feature, dependency
eligibility and protocol-transport findings. Final process review closed fairness and failure
starvation, while a separate cadence review confirmed the stale test assumption and exact-clock
correction. Implementation/review lanes are closed. Earlier progress records retain their scope.

Failure history:

- Early compilation exposed module-path and RelativePath accessor mistakes, corrected through
  explicit component conversion rather than changing filesystem authority.
- Public run `d3f5bbf6-a918-421a-963c-243516609eba` passed foreign but failed native on an
  unsupported cross-build byte-equality assertion. Equivalent selections now prove equal actual
  features and behavior plus each capture's own exact digest; executable reproducibility is not
  claimed. Captured A immutability remains exact.
- First native run `eeb0e7bc-1f2f-428a-bd4f-27cc8301e7b9` failed both cases before READY.
  A focused diagnostic confirmed Metadata stdout OutputLoss from the 256 KiB diagnostic queue.
  The owning protocol replacement, not a log-cap increase or `--no-deps` restoration, repairs it.
- Run `902ba87c-11f6-46f0-b0d2-f790b8aa7941` passed last-good/successor but failed the cadence
  oracle after READY. Diagnostic `259b4f9c-d4ea-4545-905e-7874b280ab3c` showed correct process
  deadline progress and an earlier legitimate Scene recheck. Final two-case native run passes
  the corrected exact earliest-owner oracle and all prior behavioral requirements.

The supplied ZIP was rechecked: all 91 manifest entries match length/SHA-256 and all ten imported
studies remain byte-identical. Its older receiving observations are kept separate from this
checkpoint. No reference checkout or vendor content was modified, and no build artifacts were
deleted. Code and wire facts stay canonical v1 without legacy aliases or migration readers.

# Follow-up

The explicit build-only R7 outcome is qualified; the full foundation Goal stays active.
The [cooked Player successor](../../../../plans/2026-10-31-cooked-player-delivery-plan.md) orders
complete target content, generated independent Player expectation, captured input/code/content
binding and directory publication. It reuses the original pack, admission, Scene and last-good
owners. Remote cache, a universal build graph, reusable prebuilt Player trust, complete target
activation/last-good and the remainder of Proposed0086/0088 remain separate.

Rendering/resource graph, exact repeated-output/UI completion, editable Nara-drawn Editor UI,
joint execution/motion and authored Schema/Prefab evolution retain their existing consumer gates.
No full-platform, engine performance, RSS, hermetic compilation or simultaneous source-snapshot
guarantee is inferred. No push or PR is performed.

# Citations

- [Unit plan](../../../../plans/2026-10-31-explicit-cargo-build-selection-plan.md)
- [Build guide](../../../../guides/cargo-builds.md)
- [Accepted workflow contract](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#explicit-cargo-selection-and-captured-build-results)
- [Supervised custody contract](../../../../architecture/adr/0116-standalone-supervised-editor-play.md)
- [Pinned source study](../../../../research/2026-10-31-cargo-target-selection-and-artifact-custody.md)
- [Independent consumer](../../../../../tests/fixtures/cargo-build-consumer/README.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Protocol integration progress](../../progress/2026-10/2026-10-31T030020Z-native-cargo-metadata-exposes-lossy-protocol-transport-and-requires-owned-queue-replacement-566938962bc9455cbfb9516d2fbae4ee.md)
