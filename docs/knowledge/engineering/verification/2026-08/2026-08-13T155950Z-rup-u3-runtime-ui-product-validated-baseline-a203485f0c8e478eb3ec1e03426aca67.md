---
type: "Verification Evidence"
title: "RUP-U3 Runtime UI Product-Validated Baseline"
description: "Closes the data-authored Runtime UI product slice with exact desktop, headless, accessibility, package and performance evidence."
timestamp: 2026-08-13T15:59:50Z
record_id: "a203485f0c8e478eb3ec1e03426aca67"
tags: ["runtime-ui", "reference-game", "rup-u3", "product-validated", "wgpu", "accessibility", "package"]
status: "completed"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fa9c43c70e2a6d7a8db3f42fb19aa4ede884730a"
verified_by: "serial-nextest-release-wgpu-windows-uia-package-clippy-independent-review"
---

# Verification

The final implementation was exported from exact commit
`fa9c43c70e2a6d7a8db3f42fb19aa4ede884730a` with `git archive` into a clean temporary source tree.
Every Cargo command below ran serially with `CARGO_BUILD_JOBS=1`. Release binaries, platform
probes, the candidate archive and guarded negative copies were produced from that source snapshot;
the concurrent working-tree changes were neither copied nor staged.

The following focused gates passed:

- V1: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test ui_product
  --test ui_authoring_roundtrip --test-threads=1 --no-tests fail` — **11/11 passed**.
- V2: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test first_wave
  --test runtime_drive --test headless_snapshot --test headless_cli --test-threads=1 --no-tests
  fail` — **32/32 passed**.
- V3: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --features desktop
  --test desktop_flow --test desktop_render --test desktop_parity --test-threads=1 --no-tests
  fail` — **13/13 passed**. The formal `desktop --candidate-smoke` release path, not the legacy
  continuity probe, owns exact packet, glyph, resource, backend-transaction and receipt evidence.
- V4: `desktop_accessibility_probe.exe` ran as a bounded real Windows UI Automation process and
  emitted `desktop_accessibility_probe: ok`. The UIA action used presentation epoch `177` and frame
  revision `178`; its successor committed at epoch `183`, revision `184`, with run generation `2`
  and WGPU frame interval `100..183`.
- V5: `cargo nextest run --locked -p nara --test artifact_package_policy --test-threads=1
  --no-tests fail` — **11/11 passed**. A release candidate created with the clean snapshot's
  `reference-game/tools/package.py` then passed the clean snapshot's `smoke_artifact.py` from an
  unrelated working directory with isolated user and temporary directories.
- V6: commit `1f4a83b` changed only ordinary Scene/Prefab source, widening Retry from `176` to
  `184` logical pixels. The final clean release headless binary, real desktop/WGPU candidate and
  packaged desktop/headless consumers all reported source `reference-ui.prefab.json` entity
  `retry-button`, action `reference-game.retry`, name `Retry`, and authored size `184 x 48` without
  a Rust source change or test-private presenter.
- V7: `cargo fmt --all -- --check` passed. Scoped strict Clippy passed for the Reference Game
  library, `headless`, `desktop`, `desktop_accessibility_probe`, `ui_product`,
  `ui_authoring_roundtrip` and `ui_performance_observation` with `--no-deps -D warnings`; only the
  root dependency's pre-existing compiler dead-code warnings were reported. `git diff --check`,
  engineering-memory validation and deterministic rollup checking passed.
- V8: all product assertions use structured source, geometry, semantic, packet, receipt and
  resource facts. SHA-256 is retained only for archive transport integrity; opaque pixel hashes and
  a state-by-target Cartesian product are not completion gates.
- V9: the exact planner and real desktop paths recorded the raw samples and resource facts below.

The release build command was:

```text
cargo build --manifest-path reference-game/Cargo.toml --locked --release \
  --features desktop-accessibility-probe \
  --bin headless --bin desktop --bin desktop_accessibility_probe
```

The candidate creation and smoke commands are reproducible without hidden parameters. Starting in
the repository root, the following PowerShell derives every transient path from the full commit:

```powershell
$finalSha = 'fa9c43c70e2a6d7a8db3f42fb19aa4ede884730a'
$runRoot = Join-Path $env:TEMP ("nara-rup-u3-$finalSha-" + [guid]::NewGuid().ToString('N'))
$cleanRoot = Join-Path $runRoot 'source'
$artifactRoot = Join-Path $runRoot 'artifact'
$sourceArchive = Join-Path $runRoot 'source.tar'
$archive = Join-Path $artifactRoot 'nara-reference-game-windows-x86_64-fa9c43c.zip'
$sharedTarget = [IO.Path]::GetFullPath((Join-Path (Get-Location) 'reference-game/target'))
New-Item -ItemType Directory -Path $runRoot | Out-Null
New-Item -ItemType Directory -Path $cleanRoot, $artifactRoot | Out-Null
New-Item -ItemType Directory -Path $sharedTarget -Force | Out-Null
git archive --format=tar --output=$sourceArchive $finalSha
tar -xf $sourceArchive -C $cleanRoot

$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = $sharedTarget
cargo build --manifest-path (Join-Path $cleanRoot 'reference-game/Cargo.toml') --locked --release `
  --features desktop-accessibility-probe `
  --bin headless --bin desktop --bin desktop_accessibility_probe

python -B (Join-Path $cleanRoot 'reference-game/tools/package.py') create `
  --repository-root $cleanRoot `
  --platform windows-x86_64 --version 0.1.0 `
  --source-revision $finalSha `
  --headless-binary (Join-Path $sharedTarget 'release/headless.exe') `
  --desktop-binary (Join-Path $sharedTarget 'release/desktop.exe') `
  --output $archive

python -B (Join-Path $cleanRoot 'reference-game/tools/smoke_artifact.py') smoke `
  --archive $archive `
  --work-root (Join-Path $artifactRoot 'smoke') `
  --expected-platform windows-x86_64 `
  --expected-source-revision $finalSha `
  --desktop-launcher-json '[]' `
  --desktop-environment 'NARA_WGPU_FORCE_FALLBACK=1'
```

The real Windows accessibility probe was invoked from an unrelated working directory so that the
executable-relative project lookup, Winit bridge, WGPU presentation and UI Automation client all
used the release product path:

```powershell
$probeRoot = Join-Path $runRoot 'accessibility-probe'
$probeBin = Join-Path $probeRoot 'bin'
$probeProject = Join-Path $probeRoot 'project'
$probeCwd = Join-Path $runRoot 'accessibility-cwd'
New-Item -ItemType Directory -Path $probeRoot, $probeBin, $probeCwd | Out-Null
Copy-Item -LiteralPath (Join-Path $sharedTarget 'release/desktop_accessibility_probe.exe') `
  -Destination (Join-Path $probeBin 'desktop_accessibility_probe.exe')
Copy-Item -LiteralPath (Join-Path $cleanRoot 'reference-game') -Destination $probeProject `
  -Recurse
Push-Location $probeCwd
try {
  $env:NARA_WGPU_FORCE_FALLBACK = '1'
  & (Join-Path $probeBin 'desktop_accessibility_probe.exe')
  if ($LASTEXITCODE -ne 0) { throw 'the Windows UI Automation probe failed' }
} finally {
  Remove-Item Env:NARA_WGPU_FORCE_FALLBACK -ErrorAction SilentlyContinue
  Pop-Location
}
```

The release-negative cases used two independent guarded copies. Each copy was mutated only inside
its unique temporary root; the exact release binaries stayed bound to the proving commit:

```powershell
$negativeCases = @(
  @{ Name = 'missing-ui-prefab'; RelativePath = 'reference-game/prefabs/reference-ui.prefab.json' },
  @{ Name = 'missing-font-license'; RelativePath = 'crates/nara_text/assets/FiraSans-LICENSE.txt' }
)
foreach ($case in $negativeCases) {
  $negativeRoot = Join-Path $runRoot $case.Name
  $negativeSource = Join-Path $negativeRoot 'source'
  $negativeArchive = Join-Path $negativeRoot 'candidate.zip'
  New-Item -ItemType Directory -Path $negativeRoot | Out-Null
  New-Item -ItemType Directory -Path $negativeSource | Out-Null
  tar -xf $sourceArchive -C $negativeSource
  Remove-Item -LiteralPath (Join-Path $negativeSource $case.RelativePath)

  python -B (Join-Path $cleanRoot 'reference-game/tools/package.py') create `
    --repository-root $negativeSource `
    --platform windows-x86_64 --version 0.1.0 `
    --source-revision $finalSha `
    --headless-binary (Join-Path $sharedTarget 'release/headless.exe') `
    --desktop-binary (Join-Path $sharedTarget 'release/desktop.exe') `
    --output $negativeArchive
  if ($LASTEXITCODE -eq 0) { throw "$($case.Name) unexpectedly packaged" }
  if (Test-Path -LiteralPath $negativeArchive) {
    throw "$($case.Name) published an archive after rejection"
  }
}
```

# Result

RUP-U3 and the preserved NUI-U15B product obligation are complete:

- R1-R3: Foundation record `a628828a8ff941239d79a0b8b3d60316`, Product Composition record
  `56264181873249c18d5477ce2170c8b1`, and Presented UI Evidence Correlation record
  `a87355beec2241669936985528c9c172` remain immutable. Product corrections did not replace a shared
  UI baseline.
- R4-R8: one managed gameplay-to-Retry trajectory covers terminal entry, exact receipt-bound
  pointer/keyboard/gamepad/accessibility activation, pending suppression, rejection restoration,
  application retirement and successor generation.
- R9-R12: structured tests cover `1280 x 720`, `1920 x 1080`, `2560 x 1440`, one `1.25` input-scale
  case and one real `1024 x 768` run. The 4:3 run produced content origin `[0, 96]`, content extent
  `[1024, 576]`, content scale `1.0666667`, Retry logical rect `[396, 280, 184, 48]`, and physical
  rect `[422, 395, 619, 446]` from presentation epoch `14` / frame revision `15`.
- R13-R15: Scene/Prefab remains the structural and presentation authority. Headless, desktop and
  packaged consumers reported the same changed Retry tuple. The package contained 20 fixed files,
  expanded to `48,931,466` bytes, and had archive size `17,704,134` bytes with SHA-256
  `18a4de450f716077c27951d44d1ce8fa96c6b3530d34f954317a8cd20834ae9f`.
- R16: native Windows UIA mapped the exposed Retry node and Invoke action into the exact presented
  session and produced one typed command; no internal numeric action injection was used.
- R17: exact warmed planner, GPU completion and measurement-end retained-resource facts are retained
  below. These observations are not an SLA or CI threshold.

Two guarded copies of the exact clean snapshot were used for release-negative cases. Removing
`reference-game/prefabs/reference-ui.prefab.json` or
`crates/nara_text/assets/FiraSans-LICENSE.txt` made the same `package.py create` path exit non-zero
with privacy-safe `candidate path metadata could not be read`, and neither invocation produced an
archive.

# Evidence

## Planner baseline

The canonical `1280 x 720` observation used four warmup iterations and sixteen measured calls over
one exact packet identity: surface `1`, frame revision `51`, source revision `50`, slot generation
`2`, route/geometry/presentation revisions `2`, and theme/font revisions `1`. Every sample retained
the same 154 primitives, 149 glyph primitives, glyph page key
`(font_revision=1, page_index=0, generation=1)`, and the same glyph-to-page references.

```text
218100, 220400, 220800, 226400, 230700, 225700, 227300, 222400,
222900, 221500, 222100, 219900, 217900, 221700, 218100, 214900
```

Using `nearest_rank_v1` with rank `ceil(p * n)`, `n=16`, ranks P50/P95/P99 are `8/16/16` and
min/P50/P95/P99/max are `214900 / 221500 / 230700 / 230700 / 230700 ns`.

## Real desktop baseline

The canonical local release candidate used fallback adapter selection
`NARA_WGPU_FORCE_FALLBACK=1`. Its presented observation bound backend/device `1/1`, App frame `16`,
Render transaction `16`, admission/presentation epoch `16`, UI frame revision `17`, source revision
`16`, slot generation `2`, route/geometry/presentation revisions `2`, and theme/font revisions `1`.
It submitted 154 UI primitives, 150 glyph primitives, one glyph page, 156 UI quads, 150 glyph
quads, two referenced texture bindings, 129 sprites and 124 image sprites.

```text
132452300, 133383600, 99129200, 99628800, 99559300, 99157600, 99232400, 99012100,
100003700, 99509400, 99749000, 98858700, 133141600, 133243500, 133388900, 99481400
```

For `n=16`, nearest-rank ranks are `8/16/16`; min/P50/P95/P99/max are
`98858700 / 99559300 / 133388900 / 133388900 / 133388900 ns`. P95 and P99 equal the maximum because
the sample is deliberately small.

The submitted frame's logical texture/instance/total bytes were
`2,232,324 / 22,800 / 2,255,124`. At the measurement-end boundary, current
texture/instance/total bytes were `1,183,748 / 0 / 1,183,748`, with zero pending and zero lost
retirements. Candidate-run peak texture/instance/total bytes were
`2,232,324 / 68,400 / 2,281,124`. These are bounded backend telemetry facts, not UI-exclusive VRAM
or driver residency.

## Packaged desktop baseline

The arbitrary-working-directory package smoke retained the same authored tuple and reported:

```text
99728600, 98960200, 99649200, 99099900, 99046700, 98864700, 99454800, 99228300,
99760800, 99857600, 99586300, 99815000, 99358900, 99953600, 99550600, 99552900
```

Nearest-rank min/P50/P95/P99/max are
`98864700 / 99550600 / 99953600 / 99953600 / 99953600 ns`. The package recorded submitted
logical texture/instance/total bytes `2,232,324 / 22,800 / 2,255,124`, measurement-end current
bytes `1,183,748 / 0 / 1,183,748`, zero pending/lost retirements, and candidate-run peak
`2,232,324 / 68,400 / 2,288,404` bytes.

## Measurement environment and boundary

- Windows 11 Pro `10.0.26200`, build `26200`, 64-bit.
- Intel Core i9-13900KF, 24 physical and 32 logical processors.
- Release profile for the real desktop and package observations; dev/test profile for the planner
  observation.
- The machine exposes an NVIDIA GeForce RTX 4090 with driver `32.0.16.1088` plus virtual display
  adapters. The measured candidates explicitly requested WGPU fallback selection. The product JSON
  does not serialize `WgpuAdapterSummary`, so this record does not falsely assign one enumerated
  adapter name or driver to the selected fallback device.
- Planner duration measures one exact `UiPresentationPlanner::plan` call after four warmups.
- Desktop duration measures the backend-owned `FrameExecutionStart` to GPU-completion callback for
  exact same-generation product submissions after four accepted product-frame warmups.
- Submitted-frame logical bytes exclude driver padding and are not total VRAM. Current/peak
  telemetry excludes surface images, pipelines, bind groups, samplers and untracked driver memory.

# Follow-up

No successor plan is activated by this record. Editor Domain/Shell, editable text and IME,
localization/project fonts, scroll/virtualization, dynamic content, project creation/export/repair,
and Nara UI Inspector parity remain independently admissible product lanes. Any new work must create
its own active plan and registration instead of reopening this Product-Validated evidence.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-12T051123Z-uifd-u2-successor-foundation-conformance-baseline-a628828a8ff941239d79a0b8b3d60316.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-13T073115Z-pcc-u4-reviewed-product-composition-conformance-baseline-56264181873249c18d5477ce2170c8b1.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-13T155000Z-puec-u3-presented-evidence-correlation-conformance-a87355beec2241669936985528c9c172.md`
- `reference-game/prefabs/reference-ui.prefab.json`
- `reference-game/src/bin/desktop_candidate_smoke`
- `reference-game/src/bin/desktop_accessibility_probe`
- `reference-game/tests/ui_performance_observation.rs`
- `reference-game/tools/package.py`
- `reference-game/tools/smoke_artifact.py`
