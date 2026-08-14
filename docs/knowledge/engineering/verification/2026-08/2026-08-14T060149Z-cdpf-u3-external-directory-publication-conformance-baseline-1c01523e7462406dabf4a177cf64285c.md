---
type: "Verification Evidence"
title: "CDPF-U3 external directory publication conformance baseline"
description: "Verifies the public nara_fs directory-publication Interface through an isolated external consumer and records the exact hosted support boundary."
timestamp: 2026-08-14T06:01:49Z
record_id: "1c01523e7462406dabf4a177cf64285c"
resource: "nara-fs-directory-publication"
tags: ["filesystem", "cdpf-u3", "directory-publication", "external-consumer", "conformance"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e6f542f"
verified_by: "external-consumer-nextest-clippy-linux-cross-check-root-policy-static-review"
---

# Verification

Commit `e6f542f` completes CDPF-U3 with an isolated `module-consumer/fs-consumer`
workspace member that depends only on the public `nara_fs` crate. The consumer receives an
authorized `DirectoryCapability`, stages a known finite nested tree, validates it through
capability-relative reads, consumes the move-only publication transaction and reopens the exact
published destination. It contains no project, starter, export, repair or recursive filesystem
policy.

The hosted Windows x86-64 run exercises the local NTFS tier. The success case proves the reopened
destination identity matches the receipt, the staging name is absent and the finite contents are
complete. The collision case proves the existing destination identity and sentinel remain
unchanged while the complete rejected candidate remains available as a recoverable orphan. The
read-only case proves rejection before any staging name exists.

Linux x86-64 code and tests compile, and the external test accepts only a truthful live
`NameBound`/`SameMount` publication or a pre-staging `Unsupported`/`Unproven` result. Generic CI does
not execute that filesystem test and therefore does not claim hosted ext4 evidence. The injected
unproven-parent proof remains owner-local in `nara_fs`; the public consumer does not gain a private
proof-injection backdoor.

# Result

- The public directory-publication Interface is independently consumable without the root facade,
  workspace dependency inheritance, private Nara crates or ambient native paths in production
  code.
- Every post-begin member-construction failure consumes `abort` and returns the exact clean or
  orphan terminal outcome; an active transaction is never silently dropped as cleanup.
- The external Windows local-volume cases preserve candidate, destination and receipt identity
  separately and never overwrite an existing destination.
- Production staging-name ownership is explicit: the caller supplies a bounded high-entropy
  same-parent name. Fixed names are limited to fixtures that exclusively own their temporary
  parent.
- Linux/ext4 remains hosted-unverified. Cross-target compilation and generic Linux CI do not
  broaden the support claim.
- CDPF-U1 through CDPF-U3 are complete. The Fresh Project successor remains inactive until a later
  registration explicitly activates it.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check --manifest-path module-consumer/Cargo.toml --locked --workspace --all-targets` - passed.
- `CARGO_BUILD_JOBS=1 cargo clippy --manifest-path module-consumer/Cargo.toml --locked --workspace --all-targets -- -D warnings` - passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path module-consumer/Cargo.toml --locked --workspace --test-threads=1` - 5 passed on Windows x86-64, including live success, collision, read-only admission and post-begin terminality; run ID `0e5de38d-c95d-4806-ace5-8e188846d467`.
- `CARGO_BUILD_JOBS=1 cargo check --manifest-path module-consumer/Cargo.toml --locked --workspace --all-targets --target x86_64-unknown-linux-gnu` - passed; compile-only evidence.
- `cargo fmt --manifest-path module-consumer/Cargo.toml --all -- --check` - passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --test ci_policy --test module_consumer_boundary --test-threads=1 --no-fail-fast` - 11 passed, run ID `5602f8de-267e-46ce-9c39-e26576cb5d91`.
- `CARGO_BUILD_JOBS=1 cargo clippy -p nara --test module_consumer_boundary --test ci_policy --no-deps -- -D warnings -A clippy::result_large_err -A clippy::needless_lifetimes` - passed.
- `cargo fmt --all -- --check`, `git diff --check` and `git diff --cached --check` - passed on the implementation tree.

# Review

The final immutable-snapshot correctness pass found no unresolved P0, P1 or P2 runtime defect. The
review corrected three issues before the proving commit: module-consumer CI now installs its
formatter and linter components, the CI policy binds each test command to the intended operating
system branch, and the `nara_fs` documentation accurately assigns staging-name entropy to the
caller. A stale adversarial pass against an earlier snapshot was excluded rather than treated as
current evidence.

# Follow-up

This conformance baseline unblocks, but does not activate, the Fresh Project and source-carrying
export plan. A later registration may activate that plan. Hosted Linux/ext4 publication remains a
separate evidence task; until it runs, the exact external baseline is Windows x86-64 on a live local
NTFS volume plus Linux x86-64 compilation.

# Citations

- `module-consumer/fs-consumer/src/lib.rs`
- `module-consumer/fs-consumer/tests/directory_publication.rs`
- `module-consumer/fs-consumer/Cargo.toml`
- `crates/nara_fs/README.md`
- `tests/module_consumer_boundary.rs`
- `tests/ci_policy.rs`
- `.github/workflows/ci.yml`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/plans/2026-08-14-001-refactor-capability-directory-publication-foundation-plan.md`
