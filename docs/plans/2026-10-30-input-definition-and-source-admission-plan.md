# Input Definitions And Source Admission

Date: 2026-10-30. Status: source workflow qualified; processed Input and endpoint successors remain open.
Parent: [foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md), R7/R10.
Baseline: `6ef3cb0`. Implementation: `ff228af`. Predecessor: qualified finite Source Scene capture at `f96951a`.

## Problem And Consumer

The manifest exposes `input.action_map`, but source candidate projection refuses it before any
file owner can fulfill the selection. `ActionMap` combines editable bindings, compiled lookup,
mutable context membership and mapping lifetime. Its bare serde shape persists runtime state,
and arbitrary context names can grow that state. A file loader alone would preserve those flaws.

Complete one source-authored physical-input workflow before processed startup input. The real
consumer loads file-defined bindings, drives normalized keyboard/mouse events, observes semantic
provenance and gameplay effects, changes declared contexts and travels A-to-B-to-A. Source Check
and Export must retain the same actual file receipt. The [source review](../research/2026-10-30-input-definition-and-runtime-ownership-review.md)
owns baseline evidence and candidate comparisons, not adopted implementation authority.

## Selected Boundaries

1. `nara_input` owns finite authoring definitions, compilation, immutable bindings and World-local
   context/lifetime state. Code-first and file-backed definitions use one compiler. No incremental
   mutable binding list or live-resource serde reader remains. Mapping replacement is an explicit
   admitted operation; enable/disable cannot create undeclared contexts.
2. Immutable data and retained bindings are distinct. An owning shared bindings handle carries
   the actual compiled data and any original source/acquisition owner. Borrowing a definition
   does not supply a way to clone data away from its reservation. World state is move-only;
   another independently mutable state requires its own finite admission.
   Bindings capture compiled data from an actual `AsRef<ActionMapDefinition>` shared owner once;
   later projection changes cannot reinterpret that data. Private compiled storage sharing stays
   behind move-only definitions. An unrelated keepalive supplied alongside a borrowed definition
   is insufficient. Queued lifetime snapshots
   and delivered held lifetime identities retain the original independent state allowance after
   resource replacement. They do not retain immutable bindings or an entire Scene snapshot.
3. Input installation uses the existing unpublished `RuntimeCandidateScope`, before Startup.
   A private Input owner pairs its pristine mapping identity with a single installation slot.
   Resource presence, empty bindings and equality to a default value cannot authorize replacement.
   Explicit empty code configuration competes with a selected file just like nonempty code.
4. Preserve non-reverting `ActionStaging` authority for mapping/context exits, physical endpoint
   membership, multi-binding aggregation, paused input and fresh presses after cancellation.
   Persistent identity never becomes a runtime mapping-lifetime identifier.
5. A strict canonical version-one JSON action-map envelope carries intrinsic primary Product
   identity, declared contexts/initial state and ordered current key/mouse bindings. This native
   format does not need a second identity sidecar. Existing canonical source identity checks must
   reject conflicts with actual admitted Scene/Prefab/resource sources. Header, shape and domain
   limits apply before corresponding allocations. No historical reader is retained.
6. Source projection explicitly retains the unfulfilled input selection while admitting shared
   Runtime values. Source-independent projection still rejects unresolved paths. Clearing source
   settings to reuse that projection would discard the obligation and is prohibited.
7. The existing source content owner reads the exact project-relative input file, captures its
   digest/identity, accounts for work/compiled output and retains a separate global input owner.
   Check/Export consume those facts. Input custody is independent of Scene CPU collection
   replacement. Travel verifies/reuses original input ownership; changed source requires restart.
   A missing or changed receipt refuses the target while preserving the running World. Restoring
   the original source permits retry; adopting changed global data requires restart.

ADRs 0041/0051/0035 are reconciled for these production boundaries.
Processed Product configuration, codec selection and startup input acquisition follow source
qualification through the existing [delivery plan](2026-10-29-asset-import-and-runtime-delivery-plan.md#input-and-target-build-successors).
No new Scene manager, importer registry, generic allocation kernel or target-build tool is needed.

The context/revision allowance covers the fixed maximum contexts across finite timeline and
synthetic edge logs. It is not a bound for all ingress endpoint memberships, semantic contributions
or transient action outputs. The review found a separate endpoint/contribution growth gap; retain
that finding for an Input-owned admission correction rather than reporting complete input-memory
qualification from definition costs alone. Installing only through `RuntimeCandidateScope` avoids
a public arbitrary-World receiver and needs no new App lifecycle query or Startup marker system.

The endpoint correction follows this source workflow as part of R10 before claiming complete
bounded Input state: admit physical endpoint and held-contribution counts at the same Input-owned
mutation boundary that reserves timeline edges. Unknown routes and fresh pointer IDs must not
grow cross-frame state without a finite allowance. Capacity failure leaves physical/semantic
membership and release ownership unchanged; focus loss, disconnect and retirement still release
existing memberships. Qualify repeated unique endpoints across drained frames, multi-endpoint
aggregation, exact/one-short refusal and recovery after release. This is a concrete ingress/state
repair, separate from action-file compatibility or a new engine-wide budgeting framework.

## Execution

1. Replace the Input definition/state boundary and old live serde; preserve all current action
   semantics with migrated real consumers. Admit compile/output/state costs and test their exact
   boundaries, unknown contexts, duplicate bindings, fanout and restoration counterexamples.
2. Add the strict source format and nonempty golden, using Core envelope/shape admission and
   bounded domain containers. Validate intrinsic primary Product and canonical writer output.
3. Retain source obligations through project candidate/plan admission. Capture actual input data
   and receipt, require the actual compiled Input owner and install once in the candidate.
4. Qualify a repository-external consumer with physical input, context cancellation, travel,
   failed candidates, escaped bindings, exact retention, Check and relocated source Export.
5. Independently review complete contracts, run focused serial nextest, affected feature and native
   checks, update current guides/evidence, commit and ff local main. This closes the source unit
   only. Implement processed startup input and its prebuilt source-free process proof next.

Verification belongs to actual results, not this plan. Reuse root `target`, one Cargo job and no
concurrent Cargo. Preserve user/concurrent changes and `vendor/`; reference checkouts are read-only.

The [source qualification](../knowledge/engineering/verification/2026-10/2026-10-30T083131Z-immutable-input-definitions-and-global-source-workflow-qualification-f8db4628f79943ac94a90593cef6c736.md)
records the immutable owning replacement, actual source/Authoring receipts, Startup selection,
independent physical/travel consumer, relocated Export, games and selected native DX12 paths.
It preserves failed-fixture history and final focused corrections. Continue processed Input
through the delivery plan and the endpoint correction above; this source unit does not close R7,
R10 or the full foundation Goal.

### Startup Action Map Installation Ownership

The [round twelve embedded App case](../audits/2026-11-23-prefab-authoring-player-and-input-review.md#p3-findings)
transfers the donor World's untouched `ActionMaps` before the receiving App installs Input.
Startup file selection must preserve that caller-supplied PRIMARY. `InputPlugin` records the map
revision only when the installation creates the default map. The existing revision check and
code configuration guards continue to govern replacement before Startup. With no file selection,
the transferred configuration can reach ordinary Startup.

Bevy initializes a resource only when it is absent. Its World API treats explicit insertion as a
separate operation with its own replacement semantics. Nara's pre-Startup selection uses the
same separation between initialization and an explicitly supplied resource.
[Bevy World resource initialization](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/world/mod.rs#L2034-L2058)

The Windows x64 Rust 1.95.0 pre-fix run `b87c64bc-f8b3-405b-955c-7630cd912747` observed
`Ok(())` for the transferred resource and aborted after the failed assertion while the candidate
still owned its Runtime. The regression now retires the candidate before reporting an unexpected
result. Release nextest `a09b249f-e8af-4307-9681-b912fa344c71` passes 80/80 selected Input
tests. The receiver refuses a file selection, reports no captured startup definition, and then
completes Startup using the transferred bindings with no file selected. The existing cases
also retain first Startup visibility of an admitted file and preserve code configuration on
other seats.
[Input selection tests](../../crates/nara_input/tests/input_selection.rs)

Root release nextest `c5e31fef-ebdb-4d3b-86c2-c18955a00871` passes 29/29 selected source
Input, processed Host and bootstrap cases. Processed startup sees the installed file map on its
first Startup and retains the existing refusal and retirement behavior for an explicit empty map.
Windows Winit nextest `43faa185-b9c1-4db5-a89e-aaee608fcc6b` passes both posted-key trajectories,
including focus recovery and balanced Tab, Enter and punctuation edges. Input correctness Clippy
and workspace formatting also pass.

### Distinct ISO Physical Key

The vendored macOS Winit adapter normalizes the two Cocoa event codes according to the Carbon
keyboard type. The ISO type swaps the raw `0x0a` and `0x32` before the physical lookup. The
normalized table and its inverse use `0x0a` for `IntlBackslash` and `0x32` for `Backquote`.
Logical key and text translation still use the original Cocoa event code. The Input owner
gives the extra ISO position its own serialized key. Desktop dispatch and
configuration Check accept the same key. The Windows continuation inventory retains its narrower
selection. A Mac project that intentionally bound the extra key through its former alias can
rebind that position to `IntlBackslash`.

Godot's macOS mapping keeps the positions in separate rows. Bevy carries Winit's physical code
through ingress independently of logical text. SDL's Cocoa event adapter swaps the code pair
for the ISO keyboard type, which explains why the fixed table alone was insufficient.
[Godot physical mapping](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/platform/macos/key_mapping_macos.mm#L83-L123)
· [Bevy ingress](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_winit/src/converters.rs#L20-L30)
· [SDL Cocoa event adapter](https://github.com/libsdl-org/SDL/blob/release-3.2.10/src/video/cocoa/SDL_cocoakeyboard.m)

Windows x64 release nextest `f981b062-2ba6-4100-a85d-41e52730d808` passes 270/270 Input and
Winit tests, including exact wire accounting and an overlapping hold that keeps Backquote active
after the ISO key releases. Run `d525eeb7-874b-4d79-a4a5-105aaf316aa2` passes both Win32 message
pump trajectories for balanced key edges and ordinary refocus. The initial test command named an
absent `action_map_file` target. The corrected run uses `action_map_format`.

Independent review raised the Cocoa ISO normalization issue after the fixed-table change. The
revised vendored Winit library and test targets pass locked `aarch64-apple-darwin` compilation on
Rust 1.95.0. The added test covers both code pairs, their inverse and an unaffected key. The full
`nara_winit` cross-check reaches the BLAKE3 C build. After selecting Clang for this target, the
build lacks the macOS SDK's `assert.h`. Native macOS execution, hold/release pairing on real ISO
hardware, and event attribution with mixed ANSI and ISO devices remain open. The Windows test
runs the same normalized action pipeline with Windows scancodes.
[Vendor patch](../../vendor/winit/NARA-PATCH.md#macos-iso-key-identity)
