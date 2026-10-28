---
type: "Verification Evidence"
title: "Source import contracts below project Hosts with typed read errors"
description: "Qualifies the pure executing source/import facet, concrete reader errors and migrated project and Editor consumers; processed Runtime delivery remains open."
timestamp: 2026-10-28T23:39:34Z
record_id: "e617bfdd9a104fb69bdf3a20936f7d57"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "refactor/asset-import-owner-contract"
git_commit: "c352614"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
---

# Verification

Continue the full active foundation Goal from `main@13a2744`. The source/import ownership unit is
committed locally as `c352614`. This evidence qualifies that dependency and execution boundary,
not complete R6/R7 or processed/source-free delivery. Rust 1.95.0, Windows 11, serial Cargo,
`CARGO_BUILD_JOBS=1` and the existing root target were used throughout. The independent workspace
also uses that target. No remote publication, vendor changes or reference-checkout edits occurred.

# Result

`nara_asset` now owns `SourceAssetImporter`, `AssetImportSource`, `AssetImportLimits` and
`AssetImportError<E>` under its existing serde feature. The root-owned parser/input/limits APIs
are removed without aliases. The independent text library's normal dependencies no longer include
root, Scene, Image, TileSet, Tooling or Editor. No extra crate or public erased executor was added.

The generic reader error preserves caller-owned IO, authority and budget failures through ordinary
`?`. Domain diagnostics remain distinct. A borrowed callback avoids another reader trait or public
adapter object. Root retains immutable source observations, read deduplication, auxiliary edge
accounting, pre-call reservations, panic containment, generation-retaining typed collections and
Scene commit. Editor validation uses that same facet and discards its temporary product before
metadata publication. Source-only settings require deserialization; the optional Editor facet
alone adds serialization and authorable defaults.

Actual public Check/Play, source closure, retained generation, successful/failed Scene replacement,
Editor Import/Reimport, source drift and held-parser cancellation use the replacement. A missing
auxiliary file retains `HostIo` and its diagnostic, rather than becoming `AssetDomain`.
Source Export's focused owning tests preserve exact members and source-change refusal. No fresh
native frontend or built-executable trajectory is claimed by this unit.

# Evidence

| Scope | Actual result |
| --- | --- |
| Root all-feature library check | Passed; 22.26s |
| Root/source-import/source-Export and Asset focused libraries | 116 passed; 880 outside-selection skips; build 1m41s, run 7.642s |
| Final independent pure input, Check/Play/Scene and Editor source-import consumers | 21 passed, no skips, three binaries; build 21.47s, run 3.203s |
| Pure importer no-default library, final | Passed; 1.38s |
| Pure importer normal dependency tree | No root, Scene, Image, TileSet, Tooling or Editor package |
| Asset library without serde/default features | Passed; 12.63s |
| Root no-default `runtime-2d,serde` library | Passed; 11.42s; warnings retained |
| Root/Asset all-feature/all-target correctness Clippy, final | Passed; 1m01s; unrelated warnings retained |
| Root and independent workspace format checks | Passed |

Focused libraries: `8bfbf600-f7da-47d1-9d1e-5bba99124b54`:

```text
cargo +1.95.0 nextest run -p nara -p nara_asset --all-features --locked --lib
  -E 'package(nara_asset) | test(source_import_refresh) | test(source_asset_import) | test(project_workflow::export::source_tests)'
  --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Final independent consumers: `141ae925-fb0e-4e3e-879f-8c7591805e67`:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml
  -p nara-asset-domain-consumer -p nara-editor-panel-module-consumer --locked
  --test import --test workflow --test source_import --test-threads=1 --no-fail-fast
  --status-level fail --final-status-level fail
```

The final run includes six pure input tests, seven project workflows and eight window-free Editor
workflows. Pure tests use a concrete sentinel type without an `Error` implementation, proving that
read propagation does not impose that bound. Final compiler/lint probes include the private reader
type alias that removes the new callback-signature complexity warnings. The library cohort
preceded that spelling-only alias; final external consumers and all-target compilation include it.

## Review And Failed Approaches

API reviewer `01a184da-82d1-7f92-8fe3-3a0fc200c447` found no blocking regression in reservations,
error classification, input lifetimes, panic/drop boundaries, settings bounds or collection
identity/retention. Retaining generic `E` avoids public error erasure and a separate Host-error
side channel. The pure facet does not yet need an independent resource-family marker.

The first independent Editor run `df8e8efa-2ba0-4f3d-8133-e6d13afeb953` passed seven of eight tests.
The existing held-parser fixture waited for physical pending to disappear while holding the gate
that prevented return. Its timeout called another Close wait and then aborted after the assertion
with Windows `0xc0000409`; this is not evidence establishing a buffer overflow. The baseline
contained the same impossible ordering. Production Close and pending behavior were unchanged.

The first test correction passed eight of eight in
`ed70c96e-660a-49cd-84a7-b3b6c3a6c735`, but independent reviewer
`01a184e5-42fc-77f1-9260-5cb375830805` found that one empty drive could merely commit Exit before
actual cancellation. Replace that weaker ordering with a public `Applied { Exit }` observation and
an explicit subsequent Application advance while the gate remains held. Require incomplete Exit,
retained work, then release, complete Close and absent metadata. The reviewer confirmed this
executes physical cancellation despite the later Application retry timestamp. The final 21-test
run above includes this correction. No pending slot was cleared early and no timeout was enlarged.

## Pinned Reference Findings

Reference reviewer `01a184d0-49e3-7a23-aa8b-17ce3ff4348b` inspected unchanged local Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`:

- Bevy separates raw/processed readers and the processor's selected output loader in
  `bevy_asset/src/io/source.rs`, `processor/process.rs:258` and `server/mod.rs:1391`. Its glTF
  `Scene{index}` labels in `bevy_gltf/src/loader/mod.rs:1200` do not prove reorder continuity.
- Godot separates Editor import from Runtime remap/target decoding in
  `editor/file_system/editor_file_system.cpp:2925`, `core/io/resource_importer.cpp:172` and
  `editor/export/editor_export_platform.cpp:1603`. Runtime mapping metadata is distinct from
  authoring import parameters. Resource UIDs do not establish matching of rebuilt subresources;
  name/path import-ID fallbacks remain vulnerable to the corresponding edits.
- Neither reference justifies an implicit processed-to-authoring fallback or using order, labels,
  content hashes or runtime handles as durable subproduct identity. Copy their responsibility
  boundaries where consumed, without copying their entire watcher, processor or DCC matching stack.

# Follow-up

Continue the ordered compound text-bundle and two-Scene processed Runtime consumer in the linked
plan. Reconcile actual product continuity, Runtime codec/catalog and target-build selection before
their implementation. Missing Runtime support must refuse without importing authoring source.
Local CAS, group publication, remote services and patch/DLC remain later foundation work.

Keep actual `DirectoryCapability::unlink_file` and reviewed installed-artifact disposal. Imported
editable copies remain project-owned and outside installation reclamation. Prefab conversion is
a separate source operation. The user clarification changes no deletion primitive or permissions.

# Citations

- [Ordered source/import and delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0103](../../../../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
- [Source/import guide](../../../../guides/project-asset-domains.md)
- [Prior delivery audit](../../subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
- [Artifact/project ownership](../../../../architecture/source-tool-artifact-ownership-design.md)
