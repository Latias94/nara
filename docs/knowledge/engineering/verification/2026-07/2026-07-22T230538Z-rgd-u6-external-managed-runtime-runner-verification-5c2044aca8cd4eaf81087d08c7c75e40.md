---
type: "Verification Evidence"
title: "RGD-U6 external managed-runtime runner verification"
description: "Verifies that an independently locked renamed-root Rust package can own one concrete managed-runtime loop through public Nara APIs."
timestamp: 2026-07-22T23:05:38Z
record_id: "5c2044aca8cd4eaf81087d08c7c75e40"
tags: ["rgd-u6", "runtime", "external-runner", "fixture", "public-api"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "12696d45b167cb4b8f0cb9ed61060f8b9cda7dae"
verified_by: "Focused nextest, independent fixture metadata and locked smoke, strict Clippy with documented pre-existing allowances, targeted source-boundary audit, rustfmt, and exact commit-scope audit"
---

# Verification

RGD-U6 was verified against implementation commit `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae`.
It proves an external code-first Rust package can own a concrete managed-runtime loop without
freezing a Nara-owned `Runner` trait, factory, registry, or provider catalogue.

# Result

- `tests/fixtures/runtime-runner/renamed-root` is an independent Cargo workspace with its own
  lockfile and exactly one dependency: the public root package `nara`, renamed to `engine`.
- The fixture acquires `RuntimeAdmissionReservation` before transfer, configures and seals a
  code-first `App`, admits and promotes a `RuntimeInstance`, then proves Pause, one exact fixed
  tick, system-fault observation, and finite Stop through public runtime methods.
- The fixture returns its bounded `ExternalRunReport` only after both the healthy and faulted
  runtime instances have reached `RuntimeState::Stopped`.
- Metadata and AST-backed source guards reject workspace inheritance, `[patch]`, private Nara or
  Bevy dependencies, hidden source redirects, glob imports, raw `App::run_once`, runtime World
  access, hidden driver plumbing, and a universal Runner trait.
- The public guide states the reservation/admission/control lifecycle while explicitly preserving
  Nara's no-universal-Runner boundary.

# Evidence

- Red-first: the new contract test initially failed because the required external fixture manifest
  did not exist; it became green only after the independently locked fixture and public loop were
  added.
- `cargo nextest run --locked -p nara --test runtime_runner_contract --test
  runtime_driver_boundary --test-threads=1`: 14 passed, run
  `fdcba614-d8bb-4319-a142-d9d8b138f67f`.
- The contract test invokes `cargo test --manifest-path
  tests/fixtures/runtime-runner/renamed-root/Cargo.toml --locked --jobs 1`; its fixture smoke
  passed under the fixture's own lockfile and renamed dependency.
- `cargo clippy --manifest-path tests/fixtures/runtime-runner/renamed-root/Cargo.toml --locked
  --all-targets -- -D warnings`: passed.
- Root strict Clippy first exposed only pre-existing warnings in `nara_app` and `nara_asset`:
  `result_large_err`, `collapsible_if`, `double_must_use`, `too_many_arguments`, and
  `derivable_impls`. Re-running the U6 target with exactly those allowances and `-D warnings`
  passed; no U6-owned lint was suppressed.
- `rustfmt --edition 2024 --check tests/runtime_runner_contract.rs
  tests/fixtures/runtime-runner/renamed-root/src/lib.rs`: passed.
- `git diff --check 12696d4^ 12696d4`: passed. The implementation commit contains exactly the
  guide, fixture manifest/lock/source, and U6 contract test.
- `architecture_docs` was intentionally not run: the user excluded it from the ordinary loop and
  the active plan assigns architecture-governance verification to U7.

# Review Scope

The source audit is deliberately a targeted boundary guard over all fixture Rust files. It rejects
known direct and aliasable shortcuts, but it is not a proof of every possible transitive call graph.
The independent fixture compile/run remains the execution proof, while U7 separately re-reviews
the broader runtime and Host authority decisions.

# Follow-up

1. Activate RGD-U7 and independently review ADR 0084 against the refreshed U2-U6 evidence.
2. Review ADR 0082 and the combined topology only if the Runtime authority decision is Accepted;
   otherwise record the bounded successor required by the active plan.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u6-prove-a-renamed-dependency-external-runner`
- `docs/guides/managed-runtime-runner.md`
- `tests/runtime_runner_contract.rs`
- `tests/fixtures/runtime-runner/renamed-root/src/lib.rs`
- Commit `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae`
