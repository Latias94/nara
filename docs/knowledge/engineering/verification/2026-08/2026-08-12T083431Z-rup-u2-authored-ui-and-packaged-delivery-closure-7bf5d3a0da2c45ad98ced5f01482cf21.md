---
type: "Verification Evidence"
title: "RUP-U2 authored UI and packaged delivery closure"
description: "Closes ordinary Scene/Prefab authoring, structured surface mapping, exact WGPU payload facts, headless parity, and clean-commit arbitrary-CWD package delivery."
timestamp: 2026-08-12T08:34:31Z
record_id: "7bf5d3a0da2c45ad98ced5f01482cf21"
tags: ["ui", "runtime-ui", "reference-game", "rup-u2", "authoring", "package", "wgpu"]
status: "complete"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ec38c43"
verified_by: "codex-root"
---

# Verification

The implementation under verification is the exact committed lineage `43e01bf`, `d1a3c2a` and
`ec38c43`, against successor Foundation Conformance Baseline
`a628828a8ff941239d79a0b8b3d60316`:

- `43e01bf` records exact WGPU target payload facts: packet identity/count, expanded primitive
  count, glyph primitive/page counts and target-isolated transaction identity.
- `d1a3c2a` makes Scene/Prefab facts authoritative for Runtime UI structure, style, labels, theme
  roles and action identity; adds structured desktop/headless observations, one canonical-writer
  authoring roundtrip and warmed planner observations.
- `ec38c43` closes package source/resource policy, arbitrary-working-directory smoke and equal
  structured authored facts across the packaged desktop and headless executables.

Automated verification passed as follows:

- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --features desktop --test ui_product --test ui_authoring_roundtrip
  --test ui_performance_observation --test headless_cli --test desktop_flow --test desktop_render
  --test desktop_parity --test-threads=1 --no-tests fail` passed all 31 tests with run ID
  `f92a4e56-9d4a-4de3-98b0-552b07e92dbb`.
- The focused exact WGPU payload suite passed all three tests with nextest run ID
  `9ce7b76f-cb88-4880-94ba-127f8785ddeb`.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --test artifact_package_policy
  --test-threads=1 --no-tests fail` passed all ten package-policy tests with run ID
  `4a30fcbd-8d41-4870-8054-1f2ffa3f44cd`. This includes fail-before-publication cases for a
  missing `reference-ui.prefab.json` and missing `FiraSans-LICENSE.txt`, fixed package membership,
  hostile archive rejection, bounded live-process output/time limits and isolated smoke roots.
- Strict scoped Clippy passed for `nara_render_wgpu`, the Reference Game library and product
  binaries/tests, and `artifact_package_policy`. Only the root dependency's existing allowed
  dead-code warnings were emitted. `cargo fmt --all -- --check`, Python syntax validation and
  `git diff --check` passed before the implementation commits.

Release/package evidence was rebuilt in a detached clean checkout at exact commit
`ec38c430d79dcee9cb939c4063984aa254722bb5`, with one serial Cargo process and an external target
directory:

```text
CARGO_BUILD_JOBS=1 cargo build --manifest-path reference-game/Cargo.toml --locked --release --bin headless
CARGO_BUILD_JOBS=1 cargo build --manifest-path reference-game/Cargo.toml --locked --release --features desktop --bin desktop
python reference-game/tools/package.py create --repository-root <clean-root> --platform windows-x86_64 --version 0.1.0 --source-revision ec38c430d79dcee9cb939c4063984aa254722bb5 --headless-binary <release-headless> --desktop-binary <release-desktop> --output <external-candidate.zip>
python reference-game/tools/smoke_artifact.py smoke --archive <external-candidate.zip> --work-root <unique-external-root> --expected-platform windows-x86_64 --expected-source-revision ec38c430d79dcee9cb939c4063984aa254722bb5 --desktop-launcher-json []
```

The clean candidate passed verification, isolated extraction and bounded desktop/headless launch:

- archive SHA-256 `c31c2542c913a5d281e031f880ae1f531b5cf485d47a02a25c8f01116c62a8e8`;
- archive size `14,249,287` bytes, expanded size `39,640,714` bytes and 20 fixed members;
- manifest source revision exactly
  `ec38c430d79dcee9cb939c4063984aa254722bb5`;
- structured authored tuple: Surface `1`, authored design `960 x 540`, entity
  `reference-ui/retry-button`, source `reference-ui.prefab.json` / `retry-button`, action
  `reference-game.retry`, accessible name `Retry`, authored size `176 x 48`;
- one exact UI packet with 154 expanded primitives, 150 glyph primitives, one glyph page, one
  uploaded page and 149 page reuses; retained texture bytes `2,232,324`, peak texture bytes
  `2,232,324`, peak instance-buffer bytes `39,440` and peak combined bytes `2,271,764`.

The warmed canonical `1280 x 720` planner observation used four warmup samples and recorded these
16 raw nanosecond samples for the same product frame:

```text
216200, 217500, 220300, 214800, 211100, 214800, 213700, 216400,
211600, 211600, 215200, 213000, 213900, 213700, 210200, 209400
```

Its observed minimum/P50/P95/P99/maximum were
`209400 / 213700 / 220300 / 220300 / 220300 ns`. The bounded packaged desktop run recorded 79 raw
`FrameExecutionStart`-to-GPU-completion samples, with no sample loss:

```text
2225900, 11647000, 18140500, 13564800, 12188200, 14240700, 13513900,
19386200, 19690000, 12122000, 11638100, 12160300, 12736300, 12765600,
13914700, 13358500, 13082800, 19341100, 20157100, 12551600, 13788400,
13458900, 12980700, 12926100, 13327500, 11796800, 12676400, 13770000,
14529900, 11493700, 13839800, 13932800, 12355700, 12840600, 14370800,
13965600, 12713800, 13791500, 12988000, 12889900, 14416900, 13640700,
12968300, 20776100, 19844900, 13100500, 13277200, 12777800, 11757700,
13649300, 10892200, 13465500, 13807200, 14826300, 13685000, 12577400,
13058300, 12581000, 14210100, 13212300, 12530800, 14389600, 12426600,
10382500, 11598000, 14292200, 13839500, 13302900, 13560400, 14368400,
13837500, 14321900, 13252900, 12529800, 12460600, 11214800, 13827900,
11762400, 6933300
```

Their observed minimum/P50/P95/P99/maximum were
`2,225,900 / 13,277,200 / 19,690,000 / 20,776,100 / 20,776,100 ns`. These are bounded baseline
observations, not an SLA or CI performance threshold.

# Result

RUP-U2 is complete at `ec38c43`:

- R9-R12 are covered by structured `ui_product` assertions for the three named 16:9 physical
  targets, one `1024 x 768` letterbox resize, one fractional `1.25` pointer mapping, safe margins,
  non-overlap, text bounds, modal coverage, focus/semantics and exact packet/receipt identity. They
  do not use a state-by-target Cartesian product or image digest as acceptance.
- R13-R14 are covered by the ordinary Scene/Prefab loader plus canonical writer. The isolated
  roundtrip changes Retry width from its baseline to `216` and action to
  `reference-game.retry-authored`; after reload the source prefab, expanded startup scene and
  Runtime UI semantic capture agree without changing Rust or invoking a private presenter.
- R15 is covered by equal structured authored observations from the local desktop candidate,
  presentation-free headless product and clean packaged arbitrary-working-directory launch. The
  archive includes the Runtime UI Prefab and embedded-font license and rejects either omission
  before publication.
- R16 remains proven by immutable RUP-U1 record
  `882397b5b744448cbd5cb8d5c3d0568a`, whose native Windows UI Automation action is causally bound
  to the exact presented Retry session and one typed command.
- R17 is covered by the raw warmed planner and real WGPU completion/resource observations above.

The Reference Game remains an acceptance carrier. It did not acquire a shared binding DSL,
renderer topology, widget registry or generic asset/import interface. No shared Foundation API
changed after the pinned UIFD baseline.

# Evidence

- Structural facts, typed actions, exact identities and the ordinary loader/writer path are the
  product-correctness authority. The archive digest only proves transport integrity; it does not
  prove UI layout or behavior.
- Component fixtures prove focused contracts. The clean desktop candidate proves the first-party
  WGPU path; the headless binary proves presentation-free parity; the package smoke proves the
  fixed artifact and arbitrary-working-directory launch. None is substituted for another.
- Backend submission and completion facts prove accepted work and exact resource accounting. They
  do not claim that a human saw or approved the pixels.
- The bounded desktop probes intentionally exit after their evidence barrier. A short-lived blank
  background or rectangular test content is expected probe behavior, not the final game UI visual
  design.

# Follow-up

RUP-U3 is the only eligible next unit. It performs no new Runtime UI product design: it reruns final
gates on the committed scope, simplifies the settled implementation, completes Standards/Spec
review, updates the implementation ledger and publishes one Product-Validated baseline that closes
the preserved NUI-U15B obligation. It must not activate NUI-U16, Editor, dynamic assets or another
generic infrastructure lane.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-12T051123Z-uifd-u2-successor-foundation-conformance-baseline-a628828a8ff941239d79a0b8b3d60316.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-11T074247Z-rup-u1-exact-gameplay-to-retry-trajectory-882397b5b744448cbd5cb8d5c3d0568a.md`
