---
type: "Verification Evidence"
title: "Finite typed render programs through the original Host production qualification"
description: "Verification Evidence for Finite typed render programs through the original Host production qualification."
timestamp: 2026-10-31T13:35:36Z
record_id: "a32bacec9d8b4572801df1f79fc31c01"
producer_id: "codex-foundation-main"
run_id: "render-program-production-20261031"
---

# Verification

Receiving baseline: `f14755a93836265b7c240a499d1d90d052d7dabe`, branch `refactor/foundation`.
This record accompanies the finite native-program implementation commit. Windows11, Rust1.95.0,
locked WGPU30.0.1, serial Cargo/nextest, `CARGO_BUILD_JOBS=1`, root `target`. Native qualification
explicitly selected `WGPU_BACKEND=dx12` and normal production adapter acquisition, with
`NARA_WGPU_FORCE_FALLBACK=0` for independent consumers. No new performance comparison was run.

# Result

Accepted ADR0094's bounded typed texture/buffer program executes through the original production
Host, device owner, producer admission and original-prefix custody. A real external workspace
implements immutable exact capture/adapter pairing, two-pass blur, an independent effect, raw
integer statistics, separate normalization and actual STORAGE-to-VERTEX histogram drawing. It
uses no private graph/native fixture, World, Runtime, window or independent queue submitter.

Whole-group native admission, exact old output imports, same-capture retry, typed reports and
producer-local retirement pass actual GPU readback. Output groups retain selected versions/reports
after settlement; scratch and original reconstruction input release independently. Device
requirements distinguish supported/requested/enabled facts and preserve the original recovery request.

Independent adversarial review found four P2 defects; all were repaired in their owners and rereviewed:

- Complete graph/execution/resource/report/output/state/automatic-retention metadata is admitted
  before growth. Exact per-F inline wrappers admit before capture allocation and writer commit.
  State indexing is a bounded flat Vec; user indirect heaps/driver overhead remain outside the model.
- All known local resource guards preflight before any native local allocation. A 64+64 request
  against127 refunds completely, leaving an independent64 admissible; exact128 guards retire separately.
- Enabled device hard limits and hard custody-count limits refuse deterministically instead of
  reporting deferred budget pressure. Late contribution refusal does not invalidate healthy producers.
- Original submitted custody is a bounded flat Box container with iterative reverse cleanup,
  replacing a recursive Arc chain. Refusal returns the original value outside the mutex; scoped
  uploads acquire retention first. A real65,536-entry test completes on a64KiB thread stack.

Final read-only reviewer `01a19216-79cd-7851-96c2-13ea1046a048` reports no remaining concrete P1/P2
within this prerequisite. This is not a complete R8/R9 or full foundation completion claim.

# Evidence

| Executed check | Actual result and coverage |
| --- | --- |
| Initial production focused no-default CPU |79 passed /93 skipped, run `a921d918-6eb9-45e6-a40a-a0cc1309dceb`; superseded for subsequent changes |
| All-feature owning library before flat-custody follow-up |253 passed /29 skipped, run `d89c5d52-e240-4dd6-b713-12909b21de50`,96.295s; covers model/device/stock baseline before final custody replacement |
| Flat-custody/model/typed-budget CPU successor |78 passed /207 skipped, run `d52c9b0a-7127-49df-9672-e10846edb045`,9.404s, including65,536-entry small-stack and reverse cleanup |
| Final affected Host/submission/program/device cohort |71 passed /214 skipped, run `5a818fb6-7084-44e4-93a3-544a27919adc`,42.035s, `--all-features --run-ignored all`; includes actual native failure, prefix, callback and loss/retirement cases plus final typed/custody errors |
| Final independent program workspace |7 passed /0 skipped across3 binaries, run `8be0259a-a235-421a-ae91-7443bef4b6e7`,3.867s;5CPU plus2actual Windows/DX12 numerical cases |
| Final independent semantic-port doctests |2 compile-fail examples passed; separate `cargo test --doc`,0.82s |
| Existing independent image-output regression |3 passed /0 skipped, run `df2b40d0-e0ed-46ee-8ac5-36b578c597b1`,2.788s; original public image/producer/reconstruction consumer after flat-custody replacement |
| Final correctness Clippy |`nara_render_wgpu` + `nara-editor`, all-features/all-targets, `-D clippy::correctness`: passed48.52s; existing non-correctness warnings remain |
| Final wasm32 no-default check |passed1.83s; compile-only affinity evidence, not browser initialization/runtime qualification |

Commands use `cargo +1.95.0 nextest run --locked --test-threads=1 --no-fail-fast` with explicit
filters above. Independent manifests use `--offline`, committed lockfiles and
`CARGO_TARGET_DIR=F:/SourceCodes/Rust/nara/target`. Actual public pixels/frequencies use1e-6
absolute error; raw integer counts match exactly. Every histogram pixel is checked against an
independent CPU rectangle oracle. A1 remains readable while A2/B execute and after A closes.

Host native tests cover an actual post-Ready destroyed-resource submission rejection without group
publication/watermark advancement, private-upload rejection and same-capture retry, and scratch/
reconstruction release after physical completion. Earlier pre-flat native runs `34749862-58c8-41a3-97c8-3c515cb65ed1`
(4) and `d2a3abc7-ed03-4253-8b06-26657d3f439b`(25) are diagnostic predecessors; final71 covers
their changed responsibility, rather than treating the earlier source as final evidence.

Local logs remain in TEMP and are not committed. Formatting, document links and engineering-memory
validation are checked during integration; no product rerun is required solely for recording them.

# Follow-up

Keep the full foundation Goal active. Continue ownership transition step4 through actual stock
Sprite/UI and Runtime/Editor/image-only consumers, deleting central payload/match/quad-only shapes.
Before any upload/state creation, add the selected preparation-free per-capture preflight and
candidate-private complete Image working-set collector. Preserve exact main/material snapshots,
shared native realization, original glyph/fallback behavior and Host-bound instance-budget commit.
Do not collapse stock deferred/device errors into generic Feature content rejection.

Then qualify View/instance/history groups, alternate policy, source/processed generation interleaving,
contribution withdrawal, independently paced two-window/image targets and clipped Nara-drawn
composition. Proposed0121 remains Proposed. Current prerequisite does not qualify full Editor UI,
native hot unload, external raw resource import, browser runtime, Linux/macOS execution, general
scheduling, measured performance/RSS/residency or all broader foundation delivery waves.

# Citations

- [Production design](../../../../architecture/render-program-production-design.md)
- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Public program guide](../../../../guides/render-programs.md)
- [Independent consumer](../../../../../tests/fixtures/render-program-consumer/README.md)
- [Accepted finite program boundary](../../../../architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs)
- [Previous immutable-output qualification](2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md)
