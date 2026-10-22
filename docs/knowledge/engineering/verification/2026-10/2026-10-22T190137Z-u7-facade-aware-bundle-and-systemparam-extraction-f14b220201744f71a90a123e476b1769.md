---
type: "Verification Evidence"
title: "U7 facade-aware Bundle and SystemParam extraction"
description: "Exact Bevy 0.20 extraction and executed renamed-root and renamed-ECS contracts with independent review."
timestamp: 2026-10-22T19:01:37Z
record_id: "f14b220201744f71a90a123e476b1769"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "b44e4b2"
---

# Verification

Windows 11, Rust 1.99, one serial Cargo job, root target reused for independent manifests.
Source is `b44e4b2` plus U7 and concurrent planned units. Fixture behavior is recorded independently
of unqualified engine-wide/native results.

# Result

Bundle and SystemParam resolve through direct, root and renamed Nara dependency paths. The bounded
private extraction uses the exact published Bevy 0.20.0 source, with original licenses, commit and
archive checksum retained. Runtime and macro-helper dependencies are pinned exactly together.
No token-string rewriting or development-reference baseline is used. Bundle's precise capture list
also includes lifetime and const parameters; otherwise generated ECS semantics remain upstream's.

# Evidence

- Renamed ECS fixture `cargo test --locked --manifest-path
  tests/fixtures/derive-dependencies/renamed-ecs/Cargo.toml --jobs 1`: 5 runtime tests and 3
  compile-fail doctests pass.
- Renamed root fixture with the equivalent manifest command: 7 runtime tests pass (no doctests in
  that crate). Actual bundles spawn/extract, ignored fields drop, params rebind generics, commands
  apply, builders run and a 17-field parameter retains state. No direct Bevy dependency is declared.
- Both use `CARGO_TARGET_DIR=F:/SourceCodes/Rust/nara/target`; Cargo remains serial.
- Independent read-only review compared unsafe generated paths and field ordering against the
  installed released source; no blocking finding. Licenses and checksum were independently checked.
- The root test harness now invokes `cargo test`, not compile-only `cargo check`, for these fixtures.
- The first renamed-root attempt encountered concurrent U4's WorldId read-only marker error; U4
  corrected that fetch adapter, and the complete renamed-root command then passed.
- ECS and derive owning nextest cohort: 22/22 pass, run
  `8f8aafa9-46dc-462a-a05e-ce3b494429c2`. Derive all-target strict Clippy passes with no exemptions.

# Follow-up

U4 now uses the facade's SystemParam derive without a hidden alias. Broad native/product and full
Goal acceptance remain separate; this fixture pass does not claim full Bevy API compatibility.

# Citations

- [Extraction provenance](../../../../../crates/nara_ecs_derive/upstream/bevy-0.20.0/README.md)
- [Runtime facade](../../../../../crates/nara_ecs/src/lib.rs)
- [Executed fixture](../../../../../tests/fixtures/derive-dependencies/facade_contract.rs)
- [ADR 0002](../../../../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md)
- [ADR 0044](../../../../architecture/adr/0044-root-facade-and-prelude-layering-policy.md)
