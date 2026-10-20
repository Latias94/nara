---
type: "Verification Evidence"
title: "Reflection declaration checks and Linux supervised Play storage corrections"
description: "Local target reuse, eager float proofs, bounded Play failure facts and real Linux cache publication; complete successor hosted qualification remains pending."
timestamp: 2026-10-20T15:29:24Z
record_id: "efc879a2a8974c989725df994b86e5db"
status: "in-progress"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-ci-portability-repair-plan.md"
git_branch: "main"
git_commit: "ce496963b2aadb42f408e638809f991c5587f7ae"
supersedes: "b976e5720228491691361ab578614f12"
---

# Verification

Local source: target reuse `dc2d855`, eager declaration validation `23496a3`, bounded Play
diagnostics `258c1ab`, Linux cache selection and ADR 0116 `ce49696`. Root alone runs Cargo/fmt/Git,
using Rust 1.95.0, one build job and the existing target. Independent read-only review covers
preserved negative contracts, target semantics, diagnostics/privacy/cleanup and cache selection.

# Result

The focused local results below pass. Complete hosted qualification remains pending; the hosted
run observed here uses the older `e856a5f` source. Real Linux execution proves a storage-selection
defect and its correction, but does not identify the hosted runner's remaining Start cause or
qualify the complete Linux Editor/Play trajectory. No browser runtime or cold speedup is claimed.

# Evidence

## Reflection And Ordinary External Consumers

The first pure check-only reflection run accepts two existing invalid declarations and changes
a third failure to a literal-overflow lint. The old inline const inside schema construction
does not force declaration-time evaluation. Fix the owning derive with eager unnamed constants
and actual field-access type inference; do not restore mixed positive builds or bless successful
invalid cases. All nineteen original negative sources remain. Three additional no-schema-call
cases permit overflowing-literal lints and still require E0080. Only seven inspected diagnostics
are added/updated. Real positive registration/freeze also covers wide aliases and Self projections.

| Cohort | Result | Local evidence |
| --- | --- | --- |
| Reflection default | 21/21; all 22 negative cases included | `12473add-166a-43f0-8d2f-fbb95cc6c179`, 13.413 s; `target/ci-reflect-eager-finite-default-final.log` |
| Reflection all-features | 21/21; same contracts | `4968fd31-9fda-4adf-ba7d-45b65394ad7b`, 42.151 s; `target/ci-reflect-eager-finite-all-features-final.log` |
| Ordinary consumers, repository target fallback | 2/2; real missing-import failure retained | `5d5d2978-4204-42f3-b158-25f4d0ed0b0d`, 170.478 s; `target/ci-external-fixture-target-reuse-final.log` |
| Ordinary consumers, relative `CARGO_TARGET_DIR=target` | 2/2 after derive correction | `8636e32e-b4aa-4455-bf4e-56fe0342e335`, 98.363 s; `target/ci-external-fixture-target-relative-final.log` |

Reflection command: `cargo nextest run -p nara_reflect --test derive_ui --test derive_positive
--test derive_schema --test derive_ranges --locked --offline --test-threads=1 --no-fail-fast`,
repeated with `--all-features`. External commands select only
`public_prelude_is_gameplay_first` and
`removing_documented_registry_prerequisite_fails_at_the_import` in their existing integration
targets. Independent manifests, feature graphs and locks remain; public-prelude checks all
26 bins. Cargo reuses artifacts, never cached success/failure. Warm elapsed times are not a
controlled default/relative or cold hosted performance comparison.

## Supervised Play Diagnostics

`cargo nextest run -p nara-editor --lib --locked --offline --test-threads=1 --no-fail-fast`
passes 175/175, 20 platform-scoped skips, run `9589876a-3e43-4e5c-ba27-808e9c161cad`, 32.975 s
(`target/ci-supervised-start-diagnostics-complete.log`). Earlier 172-test evidence predates the
final preparation and quiet-output corrections and is not the final acceptance result.
`cargo check -p nara-editor --tests --locked --offline` passes in 31.88 s
(`target/ci-supervised-start-diagnostics-tests-check.log`), including the clean-room diagnostic
consumer. Existing warnings are retained. The expensive clean-room trajectory is not repeated
locally for message-only assertion changes.

The existing bounded OutputNotice projection receives fixed stage/reason/operation, finite
filesystem categories, I/O kind and numeric errno. Error text, paths and environment values
never cross the preparation worker boundary. Persistent supervisor advance errors publish once
per supervisor, preserving the first output sequence. A real held-worker test proves Start,
exact reason, store return and physical close. Public failure classifications, authority,
deadlines and cancellation/cleanup order remain. Handshake Read/Write still carry their existing
kind-only failure; not every branch can provide errno. Repeated-error regression calls the real
production projection helper, not an injected failing OS supervisor.

## Real Linux Storage Selection

Baseline probe includes the actual `store_root.rs` and calls actual `nara_fs` directory publication.
On WSL Ubuntu 26.04, the old default `/tmp` is tmpfs (`0x01021994`), and publication returns
BeforeStaging Unsupported. A temporary private home/ext4 root (`0xef53`) passes the unchanged
atomicity, no-overwrite, parent and identity receipt checks; a private shared-memory tmpfs root
is rejected. Log: `target/ci-generation-root-probe-wsl-complete.log`. NameBound source publication
and StrongCompareAndSwap no-overwrite are compatible on the supported Linux tier; the receipt
guard is not weakened.

After `ce49696`, cross-rustc builds actual production source and its six Linux tests for
`x86_64-unknown-linux-musl`, with real `nara_fs` and `rustix` dependencies. All six run successfully
in WSL. The corrected default HOME cache and explicit ext4 cache both publish successfully even
with a valid tmpfs XDG runtime directory. An explicit tmpfs cache still rejects before staging,
does not fall back to HOME and leaves its private store empty. Log:
`target/ci-generation-cache-wsl-final.log`. The temporary probe stays in ignored target;
no stub implementation or permanent diagnostic framework is added.

The cache base accepts existing safe 0755 and requires current-user ownership, no final symlink
and no group/other write access. The engine leaf remains exact 0700. Tests cover missing nested
creation, unsafe modes without chmod, symlink target preservation and wrong owner identity.
Checks do not prove no-follow for every ancestor. Global cache selection is not a claim that
the user cannot configure their cache inside a project. Per-run identity, marker locks, cleanup
and session-local Last Good remain unchanged. WSL starts with a host networking fallback to None;
these native tests are offline and finish with exit zero.

## Hosted Baseline At e856a5f

Run [39437200611](https://github.com/Latias94/nara/actions/runs/39437200611), observed at this
record boundary:

- Wasm portable libraries and both module-consumer jobs pass.
- Windows reference-game passes 84/84 default (`58831bf4-4c29-4629-a16e-f4ad2f2985ef`) and
  166/166 all-feature (`2a61db88-1dd0-4d72-bd3e-57760476123d`). External Play/Stop/Exit passes
  in 547.299 s. Raw output remains in ignored `target/ci-reference-windows-e856.log`.
- Linux root default passes 2,643/2,644, with 12 skips, run
  `b24e6f37-b6ba-4bfb-8c0c-520303aef9fa`, 1067.228 s. Only
  `generated_starter_reaches_ready_stop_and_clean_editor_exit` reports Faulted/Start.
  Root all-features and the Dear adapter gate do not run after that failing command.
- Linux reference-game default passes 83/83 (`72b01d5f-029b-490d-8e8b-45061b11c762`);
  all-features passes 150/151 (`35132b14-d306-42ed-bbea-23aa36c2f517`). Only external
  Play/Stop/Exit reports Start; bounded output contains compiler warnings without the private
  cause. Its actual temporary/runtime mount cannot be inferred from that log.
- Windows root remains in progress at this boundary; no complete result is inferred.

# Follow-up

Push the reviewed batch and observe exact successor-source Linux clean-room/reference Play and
complete root/default/all-feature/native-adapter jobs at later implementation boundaries.
Use the bounded failure facts if Start remains; do not guess broader environment inheritance,
change deadlines, loosen publication or claim this WSL reproduction establishes its hosted cause.
Cache/profile/job-layout measurement remains the next separate feedback-cost slice. Keep the
unrelated untracked `vendor/` directory untouched. No new Goal or release authority is created.

# Citations

- [CI repair plan](../../../../plans/2026-10-20-ci-portability-repair-plan.md)
- [Measured CI and Bevy comparison](../../../../research/2026-10-20-ci-feedback-and-bevy-comparison.md)
- [ADR 0116](../../../../architecture/adr/0116-standalone-supervised-editor-play.md)
- [ADR 0070](../../../../architecture/adr/0070-capability-oriented-filesystem-substrate.md)
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/latest/)
- [Rust constant evaluation](https://doc.rust-lang.org/stable/reference/items/constant-items.html#evaluation)
