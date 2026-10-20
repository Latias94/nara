---
type: "Verification Evidence"
title: "CI portability and Tile locality qualification in progress"
description: "Confirmed hosted failures, Linux capability enumeration, portable wasm compilation and measured per-view queue correction; final hosted and timing evidence pending."
timestamp: 2026-10-20T10:24:41Z
record_id: "b85485ff744b4ccf8d02562d94d50ce6"
status: "in-progress"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-ci-portability-repair-plan.md"
git_branch: "main"
git_commit: "8afbe42cf0656063ef2ca9dbcf8858a7bef9ae72"
---

# Verification

Implementation: Tile `15e65649eeb103fc93371137fa72ad748ef74c08`, CI/portability
`fce2e70fc793b384646dfe0dbf12ac69edcb1678`, normally pushed to main with matching remote HEAD.
The initial source is `8afbe42`. Root alone runs Rust 1.95.0, serial Cargo, one job and shared target.

# Result

L3 source/GPU correction and three optimized processes pass, with independent source and numeric
review. Local CI repairs pass their affected source, consumer, policy and wasm checks. Hosted
run `39409199063` is still in progress; its portable wasm job has succeeded. This intermediate
record does not claim complete green CI, native browser execution or L4 completion.

# Evidence

| Check | Result and identity | Retained local diagnostic |
| --- | --- | --- |
| `nara_fs` / Sprite libraries and filesystem contract | 91 passed, 3 explicit ignored; `47877b10-91f6-4c59-b855-875ab3778d6d` | `target/ci-fs-sprite-final.log` |
| WGPU sprite/UI library including real unclipped pixel oracle | 133 passed, 1 explicit ignored; `01df868e-d1fa-499e-abd7-d47ba0802bdc` | `target/tile-gpu-final.log` |
| Linux library cross-check | x86_64-unknown-linux-gnu, locked/offline, passed | `target/fs-linux-check.log` |
| Linux filesystem actual execution | 33 passed, 0 failed; Ubuntu 26.04 WSL2 kernel 6.18.33.2; cross-built static musl test with `blake3/pure` and rust-lld | `target/fs-linux-musl-build.log`, `target/fs-linux-wsl-final.log` |
| Three exact portable wasm combinations | `nara_fs`; root runtime-core; root runtime-2d,presentation-2d,presentation-ui,tooling,serde; locked/offline library checks all passed | `target/wasm-matrix-final.log` |
| Reference-game all-feature affected consumers | 9 passed; `694ff5d7-ffc7-4139-8728-e9fd1da67559` | `target/ci-reference-final.log` |
| Independent module consumer all-target Clippy | Passed with `-D warnings`; dependency warnings remain separately reported | `target/ci-module-clippy-final.log` |
| Independent module consumer complete suite | 29 passed, 1 explicit ignored; `e2ccdcef-2b29-4839-aaa9-462f51991058` | `target/ci-module-tests-final.log` |
| CI policy | 3 passed; `d0128698-3adc-451f-adf4-10c5f54ec8be` | `target/ci-policy-final.log` |
| Root, reference-game and module workspace formatting | Passed; final diff check passed | Cargo fmt and Git checks before integration |

The Linux executable runs on the WSL `/mnt/f` filesystem; it is runtime evidence, not a hosted
ext4 release qualification. WSL's mirrored-network initialization fails and falls back to no
network; the local static filesystem test still returns exit 0. No distribution, network or
proxy configuration was changed. The Windows filesystem/source count overlaps the GPU and other
consumer scopes and is not summed into a new aggregate.

Tile release nextest identities: `653d4cc7-4bcb-42dc-be2c-10f311721e97`,
`22a45159-fdba-40fa-86bf-f79c9fd94549`, `80367d0a-f72f-4340-b092-17a6c1f12b68`.
Each passes unchanged pixel/semantic/retirement oracles with 654 numeric rows including warmups
and six cleanup observations. The report binds 126 groups and uncompressed raw hashes. At
65,536 authored cells, 1,972 conservative world candidates plus 89 UI quads use 164,880 instance
bytes; managed-drive P50 is 4.165-4.645 ms across static/changing scenarios. Whole-world extraction
and material admission remain O(N). Draw-call count remains 91. No retained geometry cache follows.

Excluded diagnostics: initial wasm compilation fails in `getrandom 0.3.4` without a backend;
the first locked retry correctly requires a lock update, then offline resolution adds only
`js-sys`/`wasm-bindgen` dependency edges. CI-policy run `f09c1430-d8f0-4203-84ac-16d714e4d078`
passes 2/fails 1 because the old exact fixture dependency list predates its test-only public
component consumer. The corrected policy retains strict library dependencies and exact facade
features. Earlier hosted failures and their source causes belong to the CI repair plan. No
failed diagnostic run is counted as acceptance.

Source reviews: independent filesystem/custody review, conservative clip/order/resource review
and real-GPU oracle review report no blocking findings. Independent data review recomputes all
three raw hashes, 126 new groups, 126 historical runtime groups and 48 displayed timing values.
It corrects draw-versus-instance and compressed-versus-uncompressed hash terminology.

# Follow-up

Observe the full hosted run after repairs; diagnose any newly exposed failures. Complete L4's
ordinary public Play measurements, stable Cargo diagnostics and any evidence-selected correction.
The remaining scalar encoders, clone and O(N) state work stay explicit. Tile authoring tools,
chunk streaming and browser WebGPU/worker execution remain outside these bounded changes.

# Citations

- [Tile report](../../../../benchmarks/2026-10-20-tile-visibility.md)
- [Tile plan](../../../../plans/2026-10-20-tile-visibility-locality-plan.md)
- [CI repair plan](../../../../plans/2026-10-20-ci-portability-repair-plan.md)
- [Hosted CI](https://github.com/Latias94/nara/actions/runs/39409199063)
- [Portable wasm commands and limits](../../../../guides/rust-product-recipes.md#compile-only-webassembly-checks)
