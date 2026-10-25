---
type: "Work Progress"
title: "Retained construction committed and source read projection checkpoint"
description: "Construction is locally committed; bounded read design and borrowed source prototype advance R2 while public query transport remains open."
timestamp: 2026-10-24T23:15:07Z
record_id: "843e9e4a886c4d7fa8e9f89ea8433591"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7f2cafa"
---

# Summary

The retained-construction unit is locally committed as `7f2cafa`
(`refactor(scene)!: own transaction-local authoring construction`). Its 18-Rust-file LF manifest
still matched `cc44a9915e7fdafce3a98fa286d067d705d49a2adfef7c8a23b2d20f4563b073` before staging.
The complete CE receipt and resolved P2 direct-Session test remain in the prior qualification;
this turn did not rerun that already completed acceptance. Memory render/validation,
1,004 changed-Markdown relative file links and staged diff checks passed. Only the 33 owned paths
entered the commit; `vendor/` remains untracked and untouched. Nothing was pushed.

The next R2 read-projection design and borrowed source implementation are uncommitted work on
that base. Neither the source prototype nor this checkpoint implements the Tooling query
transport, qualifies the independent Application read/computed-edit workflow, or completes R2.

# Design And Reference Evidence

The [read-projection design](../../../../architecture/authoring-read-projection-design.md) selects
paged actual component/root metadata followed by selected finite root reads, rather than eager
Inspector construction or mandatory value-bearing flat streams. Source owns interpretation;
Tooling must own bounded copying, exact basis/target routing and outcomes. Availability is not
component existence or a grant to edit every root. Missing only describes an omitted declared
root with no supplied default; unknown components/fields and invalid paths reject separately.

Read-only reference agent `01a17022-fb67-7e81-b445-c8ca1bda0bd1` compared Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. It confirmed actual-instance inventory, separate
interpretation/readability and borrowed/named current-value access. Bevy whole-struct dynamic
conversion and Godot whole-list Inspector construction are not bounded selected-root mechanisms.
Godot validity is separate from NIL; default/revert lookup is separate from current value, and
placeholder updates are not Nara's preservation contract. Neither reference supplies Nara's
complete immutable-basis, bounded owned continuation protocol. Exact source paths are in the
design; reference checkouts were read-only.

Read-only design review `01a17026-f78a-79f2-b5e9-0a17efb578df` found no blocking defect in the
selected experiment. Its four implementation checks are retained explicitly: distinguish all
missing/default/null/error cases; index actual unavailable/empty sources and publish indexes with
their source; bind concrete component/entity/projection in continuation; charge aggregate copied
metadata including page/cursor identity rather than cloning complete `AuthoringRecordFact`.
ADRs 0047/0118 now state these responsibility rules without claiming implementation completion.

# Source Prototype And Evidence

- `AuthoringEntityView` borrows actual direct records; component headers include all retained
  readiness states and exclude catalog-only creation entries and Prefab patch records.
- `AuthoringComponentView` borrows admitted roots only for Available records. Root lookup uses an
  immutable Schema field-ID index, then the selected wire path. `Present`, `Defaulted` and `Missing`
  distinguish explicit and defaulted Null without cloning a component/default tree.
- The original prototype indexed every entity position. Follow-up simplification reuses the
  already canonical sorted entity vector for binary lookup instead: file `finish_decode`, Prefab
  instantiation and authoring publication preserve that order. The existing direct-record map is
  broadened to index all direct classification facts, with no entries needed for empty entities.
  No performance or allocation-free claim follows; `get_path` still allocates path prefixes.
- New public owner tests first failed with the expected missing API/type errors (E0432/E0599).
  The first two tests then passed under explicit Rust 1.95.0, nextest run
  `3f12c545-935b-4f5a-9050-fb11cf9edc3b`.
- A broader compile exposed a private oracle still naming the replaced Available-only map. Its
  comparison now checks the complete direct-fact index instead of deleting/weaking the assertion.
  The next serial Scene/Tooling all-feature run `d29a1dda-467c-409c-95b6-d29e457e792a` passed
  395 tests, three skipped, before the final binary-lookup simplification and supplemental test.

Read-only kernel review `01a1702c-43e1-71f3-bbfd-4a0f81368f06` found no concrete correctness defect
in the first source prototype. It traced joint publication, field-candidate index reuse, structural
reclassification and Reopen. It correctly identified two focused test gaps: observing changed
values through field-candidate history and Present-to-Defaulted transitions after removing a
stored root. The supplemental third test covers those paths for Scene and Prefab. That review
predates the binary-lookup simplification and is not a final full CE receipt for this new unit.
All three research/review agents are closed.

After the canonical-lookup simplification and third test, serial explicit Rust 1.95.0 nextest run
`327fed5c-6b30-4062-b753-c318db3bea81` passed 396 Scene/Tooling tests, three skipped. The final
six-Rust-file LF manifest is `3d517ff9fa3e61524900e7ad8ba21e7ce118ecc1138d884a3bb1ed9914fd6112`.
It uses sorted tracked Rust diff paths plus untracked Rust files under `crates/nara_scene`, the
same normalized per-file/composite SHA-256 algorithm as the prior construction qualification.
No Rust source changed after that run.

The following explicit Rust 1.95.0 commands also pass, serially with `CARGO_BUILD_JOBS=1`:

```text
cargo +1.95.0 check --locked -p nara_scene --no-default-features --lib
cargo +1.95.0 clippy --locked -p nara_scene -p nara_tooling --all-features --all-targets -- -D clippy::correctness
cargo +1.95.0 fmt --all -- --check
```

Existing unused-code and non-correctness Clippy warnings remain; this is not a warning-clean,
root/Editor/native or performance qualification for the new read interface. Changed document
relative links and `git diff --check` pass. Unified Cargo sessions `94978`, `25480`, `35699`,
`20341` and `74448` are terminal; no agent or Cargo command from this unit remains live.

# Next Action

The owner regression above is terminal. Implement the public bounded inventory/root-read
transport over these borrowed views. Use one shared document-query basis instead of unreleased
entity-only naming, exact nominal continuations and compact owned metadata. Preflight aggregate
metadata including captured page/cursor targets and selected value nodes/bytes before deep copies.
Do not add a general query language, second document, complete Inspector or default materializer.

Compile an independent non-active Scene/Prefab discover -> read -> computed edit consumer through
Application and existing exact result/history/persistence paths. Then complete cost comparison,
matching public/native regression, simplification and actual CE review before the next source/query
implementation commit. Registered-panel prepared hooks and all remaining foundation waves remain
open; the full Goal stays active. Preserve `vendor/`, use `cargo +1.95.0`, root `target`,
`CARGO_BUILD_JOBS=1` and serial Cargo. No remote publication is authorized.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Construction qualification](../../verification/2026-10/2026-10-24T224939Z-retained-source-construction-pinned-public-and-native-qualification-e1813acad6f04a93898a1e342989fa44.md)
- [Read-projection design](../../../../architecture/authoring-read-projection-design.md)
- [Borrowed source read implementation](../../../../../crates/nara_scene/src/authoring_decode/read.rs)
- [Owner consumers](../../../../../crates/nara_scene/tests/authoring_reads.rs)
- [Existing entity queries](../../../../../crates/nara_tooling/src/entity_query.rs)
