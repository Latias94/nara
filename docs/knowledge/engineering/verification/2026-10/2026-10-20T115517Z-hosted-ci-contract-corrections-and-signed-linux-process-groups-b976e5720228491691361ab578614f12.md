---
type: "Verification Evidence"
title: "Hosted CI contract corrections and signed Linux process groups"
description: "Observed hosted failures, signed process groups, native local-workspace policy and deterministic contract fixtures; full successor CI pending."
timestamp: 2026-10-20T11:55:17Z
record_id: "b976e5720228491691361ab578614f12"
status: "in-progress"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-ci-portability-repair-plan.md"
git_branch: "main"
git_commit: "29e82625300e6b2d46528461661667f79cb233f2"
supersedes: "b85485ff744b4ccf8d02562d94d50ce6"
---

# Verification

Initial corrections `fce2e70`, explicit local schema-bundle write `7077c35`, Windows Cargo
path input `2c26c92`, exact-discard test scope `da76a21`, signed Linux process query and live
identity fixtures `27e3716`, full-suite failure collection `49423ae`. All are normally pushed to
main; root alone runs local Cargo serially with one job and shared target. L3/L4 reports and
qualification data are separately committed at `5092c6a`.

# Result

Local repairs and the earlier hosted successes below are verified within their exact scope.
Run `39418300517` completes with the failures below; repairs await successor qualification.
Complete green CI remains unclaimed. The
previous `da76a21` run reveals two new Linux failures and is superseded. The
manual Reference Game Candidate workflow's four latest historical runs are green, but it has not
been dispatched on this source and adds no current candidate-release claim.

# Evidence

Hosted run `39414102233` at `da76a21`:

| Job | Actual result | Source fact |
| --- | --- | --- |
| Wasm portable libraries, job `118599339244` | Success | Rust 1.95.0; all three locked checks pass: FS, runtime-core and backend-neutral 2D/UI/tooling. Ubuntu 24.04.5 image 20261018.339.1, runner 2.337.0. No browser execution. |
| Linux module consumer, job `118599339019` | Success | UI 3, Scene/Material 23, Editor panel 2 and Collect Game 4 tests pass; formatting, all-target checks and Clippy pass. Ubuntu 24.04.5 image 20261012.334.1. |
| Windows module consumer, job `118599339439` | Success | Complete configured job, including independent module and Collect Game checks/tests. |
| Windows reference game, job `118599339675` | Success | Default 84/84, run `50dabfbb-f576-40c2-a8aa-115987e4dbc7`; all-feature 166/166, run `a82cb54d-47f2-493d-b25a-f966ef45a3be`. External Play/Stop/Exit passes in 381.016 s. Windows 2025/VS2026 image 20261007.264.1. |
| Linux reference game, job `118599339299` | Default passes; all-feature fails | Default 83/83. All-feature 29 passed/one SIGABRT/121 unrun: `external_supervised_stop_then_editor_exit_retires_the_owner` reaches rustix zero-PGID assertion. |
| Linux root, job `118599339523` | Fails | 152 passed/one SIGABRT/12 skipped/2485 unrun, nextest `97fe0d86-1d6c-4cdf-ba84-b1cb888f843c`; `indexed_document_admission_rejections_leave_the_workspace_unchanged` recreates identical bytes with a reused inode, so receives Opened. The earlier portable custody admission test passes. |
| Windows root | Superseded while tests were running | No complete success or failure inferred. |

Hosted run `39418300517` at `49423ae` collects independent failures with `--no-fail-fast`:

- Wasm and both module consumers succeed. Windows reference-game passes default 84/84
  (`38a20e04-8582-47be-9646-26f37230e505`, 18.558 s) and all-feature 166/166
  (`6986801f-26de-4e25-adb3-cdff36b8cdf0`, 599.229 s); external Play/Stop/Exit passes in 544.065 s.
- Linux reference-game passes default 83/83 (`611adaf3-d524-42fb-ba71-152cf898549f`, 11.594 s)
  and runs all 151 all-feature tests: 150 pass and one fails (`7089648e-0443-4b0a-8956-f89e898c7398`,
  631.677 s). The former zero-PGID assertion is gone. External Play reaches Faulted/Start; the
  fixture waits 600 s without exposing the existing bounded public output. `ebc0a1f` reports that
  terminal failure immediately and preserves the selected desktop session's XAUTHORITY input.
  `xvfb-run` supplies its authentication locator there, while exact product launches previously
  discarded it and HOME. Independent review admits only that fixed desktop grant with existing
  size/NUL limits; Sealed and environment injection exclusions remain unchanged. Source evidence
  confirms the omission, but hosted execution must establish the corrected actual journey.
- Linux root runs 2,641 tests: 2,618 pass, 23 fail, 12 skip
  (`439bc2b1-abb8-4858-976a-4d85e01c141e`, 1182.809 s). The second all-feature invocation is
  not reached. Failures: 17 native Editor Save consumers using an OS-inferred strict capability;
  two generic composition fixtures assuming synchronous Open; two stale manifest boundary
  fixtures; one feature-dependent Resource compiler-help snapshot; one transformed font license.
  No test timeout increase or Linux consumer exclusion is selected.
- Windows root runs 2,659 tests: 2,650 pass, nine fail, 42 skip
  (`7883553f-c390-414a-93ea-bca07b3743f9`, 2115.759 s). Two boundary fixtures and six generic
  Open timing assumptions overlap the selected repairs. The ninth is a real GPU pixel mismatch
  at a 5-by-3 physical window, scale 1.25: pixel (0, 2) is black instead of green although inside
  content y=0.25..2.75. Composite mapping/raster investigation is pending; no relaxed oracle or
  disabled case is selected. This first root cohort alone finishes about 71 minutes into the
  cold job, before the second all-feature suite and final Dear adapter gate can run.

The user's explicit choice is cooperative local writes: selected native workspaces receive
`ReadWrite + TrustedLocal` at the existing root entry on every platform. Atomic name publication,
identity/digest checks and receipt acknowledgement remain; competing name-bound writers can race
the checks. Lower strict callers retain rejection, Linux remains NameBound and Unix cleanup may
retain recoverable orphans. Data-only Open does not authorize Rust execution. ADR 0118 and the
product guide own this policy. Independent review finds no blocking authority widening outside
the selected native entry.

Contract repairs keep the exact direct-domain dependencies and narrowly scoped dev-only workflow
facade. Four negative cases reject normal-dependency placement, desktop features, defaults and
the wrong path. A negative compile-time trait assertion replaces only the brittle Resource
diagnostic candidate list; six other privacy/linear-authority compile-fail cases remain. All seven
focused tests pass (`2e783f1f-c817-431f-b7e9-f3d4530a48a0`, 39.800 s). Generic Open fixtures await
the exact newly issued request, require Opened and the published document, then acknowledge that
request. Domain/history/preservation assertions remain.

`cfffcc0` records Fira Sans's original 4,479 CRLF bytes as binary-preserved text via `-text`,
keeping the expected SHA-256 `99bfb7c1e845b79d1031fcf37e678178eb4a8c7cc74a3f218ddf7223083a6fcd`.
The old Git blob was 4,386 LF bytes. Index and working bytes now match the pinned source; neither
font bytes nor the expected hash change. Fira Mono explicitly remains canonical LF. All three
font admission cases pass (`9463caec-6d96-4b7e-83fc-e0fecff9caba`). The native consumer cohort
passes 196/fails one (`cb3c190a-6637-4841-88b3-fbea502fb9a9`, 429.565 s, 21 skipped), including
all eight generic composition cases and actual generated-starter Play/Stop/Exit (387.867 s).
The remaining Browser fixture assumes a hard-linked file must be Unsupported even under the
new cooperative policy; its owning contract is under review. The unchanged lower filesystem
cohort passes 30/30 with three privileged skips, including strict multi-link rejection. Local
Windows checks do not substitute for hosted Linux.

`ecb8410` commits the selected local-workspace policy and `a9b110f` commits request-specific Open
fixtures. The remaining Browser fixture's contract review confirms cooperative local hard links
are AssetSource observations with exact `InspectOnly(ReadOnlySource)` access, independently of
the lower strict rejection. Both Browser/recovery tests pass after correcting that policy
assumption (`0b53b0a5-d8fc-4e88-a943-45d8ab613dad`, 0.422 s); compile-error project source remains
unexecuted. The first 196 passing native consumers and this corrected pair jointly qualify the
affected local scope without claiming that the initially failed 197-test run passed.

`0d71c4d` changes only canvas raster viewport coverage: the integer scissor's pixel-center range
now also owns the viewport; the exact fractional rectangle remains in the source-sampling
uniform. All original pixel assertions remain. Five canvas GPU cases pass with forced fallback
(`5ef75834-2132-4cb8-897c-1064c676d79f`, 7.265 s) and automatic adapter selection
(`de3e149a-b9e5-4707-b23e-6fd7ce5ab6c2`, 6.878 s). The no-submitter configuration also passes its
one applicable case (`a22e5fab-16ac-4778-9fc1-3d71bbd9eaee`). Independent mathematical review
confirms the failed pixel is inside the shared input/content mapping and must sample source
(0, 3); there is no nearest tie to excuse the black pixel. Hosted Windows/Linux validation is
still pending.

`427eba1` raises only the root job envelope from 75 to 180 minutes so its existing cold default,
all-feature and Dear gates can execute. Three workflow policy cases pass
(`2bab1941-3fb5-44b8-9dc2-be9bc5e9d190`). This does not raise any individual test deadline or
qualify that duration as a reasonable feedback target. The user explicitly requests comparison
with local Bevy CI; follow-up attributes eight selected compiler/consumer tests to 1,614.084 s (76.29%) of the
default cohort. `29e8262` moves the two compile-only positive Scene/Startup probes into ordinary
tests in the same external integration targets. Public imports, trait implementations, lifetime
and function-signature proofs, the runtime-core gate and all twelve negative fixtures remain.
Four tests pass in each outer combination: default `da070d8e-1128-4ddb-ae0c-8c925952c936`
(13.500 s) and all-features `e113261f-fb3c-4047-a82c-92c3f22b21cb` (18.531 s). Earlier runs on
the same correction also pass; these are warm local correctness evidence, not a cold hosted
speedup. Independent review confirms equivalent public-boundary coverage and the Bevy/trybuild
source mechanism. Root, reference-game and independent module-consumer formatting checks pass.
The external HTML report has fourteen unique section IDs and 45 checked links.

The signed query avoids an upstream nonzero-value violation, not only a debug panic. The
private Linux x86-64 C-ABI adapter preserves a negative OS error, represents zero as None and
only constructs a nonzero group after a positive result. Zero observation grants no signaling
authority. Retained positive child PID, WNOWAIT reservation, ESRCH handling, other-error refusal
and final descendant/leader reaping remain unchanged. Upstream issue 1696 confirms the zero
case; version 1.1.5 and libc backend also use unchecked nonzero conversion, so an upgrade/global
backend switch is not the selected correction.

Local follow-up evidence:

- Exact private query source builds as Linux static musl tests in both debug and optimized modes;
  all three tests pass in each actual WSL execution. A temporary caller of the same source scans
  live `/proc`, observes two zero and 29 positive records, and returns success. Logs:
  `target/ci-pgid-zero-{debug,release,live}-wsl.log`. WSL network falls back to None; no network or
  distribution configuration is changed.
- Six Windows supervised-process output/flood/force-kill/nonzero-exit/descendant cases pass;
  nextest `d300936f-9c6a-4c22-afae-5e6a12e08a55`.
- Two original identity-rejection/unchanged-workspace cases pass after retaining the old file
  through rename; `e9b993e2-b73a-40c8-81ee-f087d666274e`. This supplies a provably different live
  native identity without changing the rejection contract or claiming full replacement history.
- The root authoring library cross-builds for Linux musl with project-workflow, tooling, runtime-ui,
  presentation-2d and `blake3/pure`. Its complete standard Rust test binary passes 184/184 in WSL,
  15.47 s, including the identity fixtures and Browser preparation. Because Windows cross-builds
  embed a Windows `CARGO_MANIFEST_DIR`, an external temporary working directory maps that literal
  path to the existing checkout solely for fixture reads. The first attempts pass 171/fail 13
  with missing fixture files before the correctly escaped mapping; they are excluded diagnostics,
  not product failures or acceptance. No production path/format/authority is changed. Logs:
  `target/ci-linux-authoring-crossbuild.log`, `target/ci-linux-authoring-wsl-final.log`.
- Editor all-target check passes; CI policy passes three tests after `--no-fail-fast`, nextest
  `497ec46c-5f0b-4a8f-aaf2-f1d824992d5f`. Serial execution, timeout and nonzero exit remain.
- Additional actual Linux musl/WSL library scopes pass: Tooling 51, Sprite Render 32, Image 85,
  Tilemap 39, FS library 29, Scene 148 and Input 43. Tasks passes 45/45 with each test in a separate
  process, matching the configured nextest isolation. The initial shared-process standard Rust
  harness passes 44/fails one Tasks case because another test's globally retained worker has not
  retired at its initial zero-state assertion; it is retained as a harness limitation and is not
  counted as an all-green shared-process suite. Logs: `target/ci-linux-core-libraries-{crossbuild,wsl-final}.log`
  and `target/ci-linux-tasks-isolated-wsl-final.log`. These selected library scopes are not the
  complete hosted workspace, GPU or native Editor matrix.
- Independent source review finds no blocking query/lifetime/signaling issue. Separate filesystem
  review confirms ADR 0070 permits native ID reuse after deletion/close; `FileIdentity` rustdoc now
  clarifies that a live root session does not retain every previously opened leaf.

Source/GPU L3 evidence and initial Linux FS/portable wasm qualification remain in the predecessor;
L4 qualification separately records the real public Play and exact CLI cache interoperability.

# Follow-up

Observe complete successor hosted results, repair further concrete failures and then record the
final exact source/job outcomes. Actual Linux external Play/Stop and descendant cleanup remain
required beyond the standalone query tests. Browser execution is outside the compile-only lane.

# Citations

- [CI repair plan](../../../../plans/2026-10-20-ci-portability-repair-plan.md)
- [Predecessor intermediate qualification](2026-10-20T102441Z-ci-portability-and-tile-locality-qualification-in-progress-b85485ff744b4ccf8d02562d94d50ce6.md)
- [L4 qualification](2026-10-20T112401Z-rust-play-attribution-and-windows-cargo-input-qualification-2abb1db859f04d848d88a1706ad167be.md)
- [Measured CI costs and Bevy comparison](../../../../research/2026-10-20-ci-feedback-and-bevy-comparison.md)
- [Failed and superseded hosted run](https://github.com/Latias94/nara/actions/runs/39414102233)
- [Successor source run](https://github.com/Latias94/nara/actions/runs/39418300517)
- [Upstream rustix issue 1696](https://github.com/bytecodealliance/rustix/issues/1696)
