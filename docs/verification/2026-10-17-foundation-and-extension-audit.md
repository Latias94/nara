---
title: Foundation and Extension Readiness Audit
date: 2026-10-17
type: audit
status: evidence-only
baseline: e5b2d85bb053e1e0730a043bfa38e766ec52764f
scope: runtime-assets-2d-editor-authority-and-continuous-verification
---

# Foundation and Extension Readiness Audit

## Judgment

Nara now has a credible foundation for continued development. The changes since the October 15
audit materially improved the owners of hierarchy, resource publication, product composition,
Editor documents, native presentation, and supervised Play. The previous audit is historical
evidence, not a current backlog. Its principal runtime defects have been corrected.

The reviewed paths do not justify another wholesale foundation rewrite. They do justify a short
correctness and verification successor: four concrete integration defects, two continuous-test
gaps, and one broken author-facing guide. The useful fearless refactors are inside the owners of
these contracts, including removal of misleading source scanners and duplicated current-status
prose. There is no evidence-backed need to replace the ECS, recreate the Editor kernel, introduce
a general transaction/process framework, or change GUI technology again.

This document recommends the scope and sequence of a successor. It does not activate a plan,
change an Accepted ADR, or claim production performance or additional native platforms.

## Baseline and Method

- Reviewed HEAD: `e5b2d85`; branch: `refactor/engine-foundation-contracts`.
- Three Codex subagents traced runtime/assets/rendering, Editor/Play/tooling, and documents/CI.
  The primary reviewer checked retained findings, the native presentation bridge, and the current
  authority chain, then ran focused tests and an independent public-API asset probe.
- Tracked files were initially clean. The existing untracked `myeasylog.log`, vendor `Cargo.lock`,
  and vendor `imgui.ini` were preserved. No production source was changed by the audit.
- Reviewed changes since audit commit `757fb6a`, as well as affected current source and tests.
  This was an ownership and failure-path review, not a line-by-line review of every vendor file.
- `codebase-audit` and `codebase-design` supplied the review method. The audit skill's requested
  Opus model is unavailable here; the user-authorized reviews used Codex subagents.
  `rust-best-practices` governed the temporary Rust probe.
- Tests used Rust 1.95.0, Windows x86_64, the existing `target`, one Cargo invocation at a time,
  `CARGO_BUILD_JOBS=1`, and one nextest test thread.
- The Editor's recorded Windows/DX12 native acceptance was inspected, not rerun in this audit.
  Linux/macOS native behavior, GPU readback, and complete workspace green are not certified here.

## What Has Actually Advanced

The four recent foundation/authoring/Editor plans are completed; the engineering-memory index has
zero active execution heads. A successor can be scoped without reopening a completed unit.

| Area | Current evidence and assessment |
|---|---|
| Hierarchy and Scene lifetime | Baseline materialization precedes topology proof; every hierarchy member participates in invalidation, including childless roots. Preserve these owners. |
| Assets and Image preparation | Usable publication is independent of the latest load attempt. Failed reloads can still supply an immutable last-good prepared image. A rejection-path revocation defect remains below. |
| Runtime composition | The managed runtime and compiled game definition consolidate lifecycle ownership. Plugin schema contributions reach the project's own strict tool process. |
| CLI Check/Export | The project headless tool owns native schema validation and publication. The old empty-recipe strict-validation defect is closed. |
| Persistent custom components | Common scalars, data-only Inspector edits, creation templates, Scene/Prefab composition, Undo, Save, and Reopen have real consumers. Unsupported structural field types remain explicit boundaries. |
| Editor | Documents/history remain in semantic owners; Dear widgets own local drafts. Scene and Game have independent view/presentation identities. The frontend migration is complete within its recorded scope. |
| Supervised Play | Background generation work, process ownership, exact READY admission, close retry, and observation deduplication are implemented. Restart admission still has one source-divergence hole. |
| 2D order | Sprite and Tilemap share visual ordering; primitive kind no longer overrides layer. Optional world-foot-Y and stable cross-kind ties are implemented. |
| Map scale | Tilemap still expands all stored cells, followed by per-view projection. Dirty chunks have no production render consumer. No large-map performance conclusion is supported yet. |
| Verification | Independent external consumers and exact committed-source acceptance exist. Vendor patch tests and ignored native Editor trajectories are not continuously protected by ordinary CI. |

The final Editor acceptance is
[`b68c3a9f938a49b7ba63779f2849d0d9`](../knowledge/engineering/verification/2026-10/2026-10-17T102351Z-editor-foundation-u9-and-u1-u8-close-with-selected-dear-frontend-and-exact-external-products-b68c3a9f938a49b7ba63779f2849d0d9.md).
Its source is `f4ffdd18`; it records a finite Windows/DX12 native workflow and the same external
game's authoring, Play, Check, and two-product Export. The limitations are stated honestly.
An engine-wide completion percentage would have no finite specification behind it.

## Findings

There are no new confirmed P0/P1 findings in the reviewed paths. The following seven P2 and two
P3 findings are ordered by correction priority. Static findings are explicitly distinguished from
executed reproductions.

### F01 — P2 — Rejected removal leaves a deleted asset available

**Confidence: executed public-API reproduction, three rejection paths.**

In [reload/resolution.rs](../../crates/nara_asset/src/reload/resolution.rs), `enqueue_record`
identifies `Removed` at line 238, but consumer admission (line 223), item admission (line 243),
and retained-byte admission (line 258) can all call `reject_request` without invalidating the
published version. [state.rs](../../crates/nara_asset/src/state.rs):265 changes the latest attempt
to Failed while retaining publication availability. The drained source change has no automatic
retry. Image preparation consults this retained `usable_version`.

The probe publishes b, then submits a Modified and b Removed. With request capacity one, a consumes
the first slot. It also tests a one-byte request budget and an absent image consumer. Actual output:

```text
item-limit:       removed b usable=Some(AssetVersion(1)), attempt=Failed
byte-limit:       removed b usable=Some(AssetVersion(1)), attempt=Failed
consumer-missing: removed b usable=Some(AssetVersion(1)), attempt=Failed
next-frame, all three cases: usable=Some(AssetVersion(1))
```

The temporary probe uses only public App/Runtime, AssetServer, Assets, source-change, and limit APIs.
It does not depend on a GPU or private `set_loaded_at`. It was compiled against the freshly built
1.95.0 libraries. Its source is at
`C:/Users/Frankorz/AppData/Local/Temp/nara-foundation-audit-20261017/removal_probe.rs`.

**Correction:** in the asset resolution owner, distinguish an accepted source revocation fact
from admission of decoding/cleanup work. A recognized removal must revoke derivation eligibility
even when work admission fails; ordinary Modified failures keep last-good. Retain explicit cleanup
responsibility for any CPU allocation. Do not patch each Sprite/UI/GPU consumer separately.

**Regression:** item limit, byte limit, absent/unclaimed consumer, a following quiet frame, and
an obsolete asynchronous result that must not reauthorize the removed publication.

This is a violation of Nara's Accepted ADR 0117 policy. It is not a claim that every mature engine
must invalidate every live cached object when its source file disappears.

### F02 — P2 — Game resize invalidates actions collected in the same UI frame

**Confidence: cross-owner static trace; no new native reproduction run.**

The Dear frontend constructs a batch using the displayed Editor revision. In
[platform/imgui.rs](../../editor/src/platform/imgui.rs):1072–1075, a changed embedded Game extent
calls `resize_game_view` and `drive_editor_frame` before that batch is dispatched at line 1097.
The drive publishes a new Editor revision through
[editor/model.rs](../../src/project_host/runtime/editor/model.rs):122. Navigation, browser,
image-import, or gesture actions make the entire batch require the current frame, so
[editor/actions.rs](../../src/project_host/runtime/editor/actions.rs):36 rejects it as stale.

Resizing/redocking/restoring layout or changing DPI can therefore discard another panel's
selection/action in the same UI output, even when its document revision has not changed. A mixed
batch also loses otherwise admissible document edits.

**Correction:** give one UI output a coherent dispatch/reconcile/publish tail. Flush captured input
while its presented geometry is still valid, dispatch the collected batch against that frame,
then reconcile extent and publish the successor. Preserve exact geometry admission.

**Regression:** a resizable Game extent successor and a real action in another panel in one frame,
including a mixed batch. Existing independent resize and picking tests do not cover this ordering.

### F03 — P2 — Restart stops a good child before discovering changed saved source

**Confidence: static end-to-end trace; contract choice confirmed by the user.**

[frontend/imgui/project.rs](../../editor/src/frontend/imgui/project.rs):724 enables Restart while
Running, including an out-of-date source. Host admission in
[external_play.rs](../../src/project_host/runtime/editor/external_play.rs):535 reacquires/checks
source for Play and RunLastGood, but not Restart. The private
[owner.rs](../../editor/src/supervised_play/owner.rs):416 reuses the old source receipt and retires
the current child. The replacement child correctly checks that receipt against current project
files in [child.rs](../../src/project_host/product_launch/child.rs):375.

Running → edit and Save a Scene/Prefab in the startup closure → Restart therefore loses the
working child and fails startup against the old digest. No old source-data snapshot exists.
The existing source-tracking test uses injected observations and does not execute this real child
sequence. The exact receipt check is correct; Restart admission and its explanation are incomplete.

**Confirmed direction:** reject source-diverged Restart before stopping the child; keep the
current process and direct the author to Stop followed by Play. Do not add source snapshots.
Refine ADR 0116's admitted-generation restart wording and align the frontend availability/reason
with Host admission. Check the actual saved closure at admission, including externally changed
files, rather than trusting a possibly stale UI out-of-date flag.

**Regression:** unchanged-source Restart succeeds; saved closure divergence rejects without
retirement; unsaved edits do not silently redefine saved source; external disk divergence is
detected; Stop→Play targets the new source. Preserve exact child/READY validation.

### F04 — P2 — Quiet external Play drives Editor redraw at approximately 60 Hz

**Confidence: static native-host deadline trace.**

[platform/imgui.rs](../../editor/src/platform/imgui.rs):1762 assigns a 16 ms deadline to every
Running state. Lines 1680–1704 also calculate runtime cadence without distinguishing placement;
lines 1721–1733 turn an elapsed deadline into a redraw. Ordinary supervised external Play has no
embedded Game image requiring this rendering cadence. Its owner already has wake/deadline inputs.

The Editor consequently keeps producing frames while the external child is quiet, consuming
CPU/GPU alongside it. This is a separate path from the old observation-duplication defect, which
has been fixed.

**Correction:** centralize a placement-aware cadence decision. Advance the embedded runtime at its
required cadence; let quiet external Play wait on owner wake/deadline and actual UI demand.
Separate a process-maintenance wake from presentation demand: observing an unchanged child at an
owner deadline should not itself request a redraw. Preserve lifecycle, text-input, browser-work,
and import deadlines.

**Regression:** quiet external Running has no animation deadline/redraw loop; embedded Running
and pause/resume/step still progress, and child exit wakes a sleeping Editor.

### F05 — P2 — Vendor patch tests are outside the production CI test graph

**Confidence: manifest and CI inspection.**

The root [Cargo.toml](../../Cargo.toml):11 excludes `vendor/dear-imgui-wgpu`. Testing the Editor
compiles that dependency but does not run its unit/integration tests. The current
[CI](../../.github/workflows/ci.yml) has no vendor-manifest test invocation. The recorded 100 patch
tests therefore do not automatically protect the native execution/presentation bridge.

The standalone command preserved by [the archived patch provenance](../research/2026-10-16-editor-foundation/renderer-020-patch.md)
(original `NARA_PATCH.md:95`) also lacks
`--locked`; the vendor's untracked lockfile is separate from the root consumer's resolution.
Running it independently can verify a different compatible Dear/WGPU graph.

**Correction:** add a finite gate for the actual WGPU 30 production cohort with explicit adapter
requirements and a resolution policy shared with, or checked against, the root graph. Do not
multiply permanent CI across every upstream optional backend. Retain vendor provenance,
licenses, the patch contract, and upgrade checks; these are useful ownership records.

### F06 — P2 — Native Editor acceptance is not a continuous regression gate

**Confidence: test attributes and CI inspection.**

The twelve real native Dear tests are ignored, Windows-specific tests; ordinary CI never selects
them. Examples are `cadence_tests.rs:69`, `game_input_tests.rs:161`, `lifecycle_tests.rs:573`, and
`supervised_play_tests.rs:32` under `editor/src/platform/imgui/`.

The recorded local acceptance is valid within its scope. However, ordinary green CI does not
protect actual native close/reopen, extent/DPI changes, Game input, device replacement, or cadence.

**Correction:** choose a small required Windows/DX12 native trajectory on a controlled runner with
an interactive desktop. Keep a broader scheduled/release trajectory where infrastructure permits.
If no suitable runner exists, retain an explicit manual release gate rather than claiming that
ordinary hosted CI covers it. Linux/macOS qualification is a separate bounded commitment.

### F07 — P2 — The persistent-component guide teaches incompatible current contracts

**Confidence: current documentation compared with actual API signatures.**

[persistent-components.md](../guides/persistent-components.md):95 returns
`PreparedComponentCandidate` from `register_persistent_component_with_codec`; the current API at
[registry.rs](../../crates/nara_reflect/src/registry.rs):1121 requires a decoded `T` instead.
The candidate-returning preflight API is a different method.

The guide's lines 74–78 still describe duplicate plugin registration and `known_providers`, while
[rust-product-recipes.md](../guides/rust-product-recipes.md):112 puts registration in the
contribution callback. Its supported-field list omits the implemented scalar types.

**Correction:** keep composition instructions in one canonical recipe guide. Make the persistent
guide own schema/field/codec semantics, update the scalar list, and compile one manual-codec
example through an existing external consumer. Do not add another string-scanning documentation
checker. The Editor README's exact-adapter requirement is not a contradiction: generic scalar
authoring itself installs an owned data-adapter receipt.

### F08 — P3 — Remaining source-name scanners prove the wrong boundary

**Confidence: deterministic false-positive mechanism in current tests.**

[reference_game_contract.rs](../../tests/reference_game_contract.rs):79 scans all production Rust
path segments and method names against a denylist that includes generic `promote`. An unrelated
gameplay `unit.promote()` would fail without touching an engine lifecycle API. The test resolves
neither receiver nor symbol ownership.

[runtime_runner_contract.rs](../../tests/runtime_runner_contract.rs):171 similarly requires and
forbids source substrings, despite already compiling and running the independent fixture.

**Correction:** remove these name/text proofs. Preserve real Cargo graph checks, independent
consumer execution, and negative compilation tests for private API boundaries. Fix public owner
surfaces if they permit forbidden lifecycle authority. The former elaborate schedule/module
scanners have already been removed and should not be reported as still present.

### F09 — P3 — Old ledger rows still advertise superseded runtime limitations

**Confidence: current ledger compared with Accepted refinements and source.**

Rows in [implementation-status.md](../architecture/adr/implementation-status.md):353,383,411 still
describe live submission reads, primitive-kind order, or single-view cardinality as current gaps,
despite completed packet work and ADR 0117's implemented refinements. These rows can recreate a
false backlog during the next planning session.

**Correction:** reconcile affected current rows with their Accepted refinements. Keep historical
plans and verification evidence. Reduce the architecture README to authority/owner navigation;
current progress belongs in the ledger/registry rather than a second detailed chronology.

## Architecture Assessment for Continued Rust Development

The important Rust boundary is who may mutate a live world/resource and when a borrowed value may
outlive that mutation. Current exclusive world mutation, frozen game/schema composition, stable
identities, immutable extracted/prepared packets, and value-only frontend/process observations
provide a coherent answer. These decisions reduce borrow pressure without sharing the entire
engine through locks or allowing widgets to mutate runtime storage.

Most new defects arise when otherwise reasonable owners are integrated in the wrong order or
one failure branch bypasses an invariant. F01 belongs to asset resolution; F02/F04 belong to the
native Host's frame tail/cadence; F03 belongs to launch admission. Enlarging public abstraction
surfaces would not repair these branches. Prefer private, explicit operations at those seams and
exercise their interleavings.

Keep the following boundaries:

- ECS/hierarchy/transform/identity and Scene ownership; narrow lifecycle-free Scene guards still
  have a real purpose alongside domain reconciliation.
- Frozen schema/native composition in the compiled game definition; project code executes only
  through explicitly requested tools/Play, not ordinary data-only Open.
- Semantic document/history authority independent of Dear widgets and native window identity.
- Immutable render snapshots and exact presented-geometry input admission. The vendor bridge
  observes actual sampled/presented views; it is not redundant test-only scaffolding.
- Private supervised process/generation owners and exact READY validation.

Remove or simplify only where the evidence identifies duplication: name-based pseudo-proofs,
contradictory current guides/status prose, scattered placement-blind Running predicates, and
mid-output publication in the native Host. A large file or a high crate/ADR count alone is not
evidence that its abstraction should be deleted.

## Mature-Engine References

Official Unity/Godot references guide the following domain semantics. Their C++/C# storage and
ownership mechanisms are not Rust implementation requirements.

- **An externally indivisible renderer group.** Unity SortingGroup keeps descendant renderers
  together against other scene renderers while retaining local order. This is the relevant
  visual contract for body/weapon/shadow composition; Nara's integer key alone does not provide it.
  [Unity 6 SortingGroup](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/Rendering.SortingGroup.html).
- **A shared external foot point with stable internal order.** Godot's parent Y-sort policy can
  keep a child subtree together while sorting it with its surroundings. This is a useful model
  for a multipart character, unlike Nara's current integer depth bucket. Retain Nara's coordinate
  convention rather than copying Godot's Y direction.
  [Godot 4.6 CanvasItem](https://docs.godotengine.org/en/4.6/classes/class_canvasitem.html#class-canvasitem-property-y-sort-enabled).
- **Sorting is separate from compositing.** Godot CanvasGroup composites children through a
  backbuffer; it is not a reason to add an offscreen pass for an ordinary visual sorting group.
  [Godot 4.6 CanvasGroup](https://docs.godotengine.org/en/4.6/classes/class_canvasgroup.html).
- **Static map chunks and interleaving geometry need different granularity.** Godot batches map
  updates and normally uses rendering quadrants, but its square-quadrant setting does not apply
  to Y-sorted maps. This supports separate cached ground and finer sorted occluders in Nara;
  it does not establish the right chunk size or a measured performance gain.
  [Godot 4.6 TileMapLayer](https://docs.godotengine.org/en/4.6/classes/class_tilemaplayer.html#class-tilemaplayer-property-rendering-quadrant-size).
  Unity similarly distinguishes grouped Chunk rendering from Individual tiles that can interleave
  with other scene renderers, with a batching/performance tradeoff.
  [Unity Tilemap renderer modes](https://docs.unity3d.com/6000.0/Documentation/Manual/tilemaps/work-with-tilemaps/isometric-tilemaps/renderer/tilemap-renderer-isometric-modes.html).
- **Dirty rebuild and view visibility are different operations.** The official implementation
  distinguishes broad rebuild causes from dirty cells/quadrants. A Nara camera move should
  normally change the visible set rather than rebuild unchanged map contents.
  [Godot source at d824ca9](https://github.com/godotengine/godot/blob/d824ca9fe29137edbcbf682f1bafabb74a435eab/modules/tilemap/tile_map_layer.cpp#L229-L285).
- **Cached resource lifetime is distinct from file existence.** Godot's reuse mode can return an
  existing cached resource. It does not promise the source-deletion behavior of ADR 0117.
  Borrow the separation of load attempt, cached object lifetime, and source index; use Nara's
  explicit policy to decide derivation eligibility.
  [Godot 4.6 ResourceLoader](https://docs.godotengine.org/en/4.6/classes/class_resourceloader.html#enum-resourceloader-cachemode).
- **Authoring source and running state have separate authority.** Unity describes Play changes
  as temporary; Godot distinguishes local authoring from remote runtime state and offers explicit
  scene synchronization. These support the separation, not an assertion that either engine uses
  Nara's exact executable/source receipt protocol. Nara's source-diverged Restart behavior follows
  the user-confirmed policy and its actual external-child contract.
  [Unity Game view](https://docs.unity3d.com/6000.0/Documentation/Manual/GameView.html),
  [Godot debugging tools](https://docs.godotengine.org/en/4.6/tutorials/scripting/debug/overview_of_debugging_tools.html).

## Recommended Successor Scope

This is a recommended sequence, not an activated implementation plan. It preserves the user's
priority: technical correctness and sustainable infrastructure before expanding game workflows.

| Order | Bounded unit | Exit evidence |
|---|---|---|
| 1 | Correct asset revocation across admission failure (F01) | All rejection branches revoke usable derivation, Modified failure keeps last-good, obsolete completion cannot restore removed content |
| 2 | Reconcile one native UI output before publishing its successor; make cadence placement-aware (F02/F04) | Same-frame resize/action interleaving passes; quiet external Play does not continuously redraw; embedded cadence and exact input remain correct |
| 3 | Refine Restart admission and ADR 0116 using the confirmed no-snapshot policy (F03) | Changed saved closure rejects before retirement, unchanged Restart works, Stop→Play adopts current source |
| 4 | Make critical accepted paths continuously repeatable (F05/F06) | Exact production vendor cohort gate; smallest real Windows native trajectory or an explicit manual release gate |
| 5 | Repair author-facing/current authority and delete pseudo-proofs (F07–F09) | Manual codec consumer compiles; composition has one guide; current ledger agrees with source; private-surface tests retain real boundaries |
| 6 | Admit one real Tilemap workload, measure, then select the rendering change | Authored map with floor, occluders and a composite actor; captured extraction/queue/upload/batch costs and correctness before/after |

Units 1–5 can be one short correctness/maintenance plan. Unit 6 should be the next evidence-driven
2D rendering slice rather than an excuse to keep widening that plan. It is a technical engine
consumer, not a commitment to build a game or a map editor first.

### Map and sorting boundaries to preserve

Current `sort_key` is a depth bucket, not an atomic multipart sorting group. Both Sprite and
Tilemap explicitly document that Y sorting stays inside this key. Y anchors are per primitive;
a composite character currently needs its parts to share the appropriate world foot point.
Do not relabel this as an already implemented Unity-style SortingGroup.

The current queue sorts by view, layer, explicit key, descending world Y, and stable source order.
It batches only adjacent compatible items, so transparent A/B/A is not silently reordered and
different Y coordinates alone do not force separate batches. Preserve those properties.

The reference floor remains 15×8 authored-in-code Sprite instances, not a Tilemap workload.
This is 120 instances, not necessarily 120 draw calls. The Collect Game fixture also does not
establish large Tilemap extraction/cache behavior merely by including Tilemap in its schema.

Before committing a cache architecture, use a small correctness map and a larger scale variant;
include static floor, foreground canopy, a moving actor, local tile edits, view movement, and
TileSet/image changes. Require real saved/reopened map data and asset references, rather than
only generating cells at runtime. If that consumer exposes a schema limitation, admit the smallest
necessary field/reference contract before widening reflection generally.
Record stored/visible cells, extracted items, extraction/queue time, bytes
uploaded, and batches. A private cache needs per-consumer revision observation and dependency
invalidation; consuming a global dirty list once does not satisfy multiple views or consumers.

Do not put GPU/native caches into persistent Tilemap data. Choose spatial culling, static chunk
rebuilds, and individually sorted occluders according to the measured consumer and visual order.
A genuine multipart sorting group is a small domain feature to admit when that map demonstrates
the need; a general render graph is not its prerequisite.

Accepted ADR 0092 remains a separate finite correctness obligation: public Color semantics,
explicit conversions, persistent finite-value admission, and linear composition/output evidence.
Close it before relying on authored tint/color-picker behavior or screenshots as a color-correctness
baseline. It need not block cell-count and CPU/GPU cost measurements, and does not justify a new
rendering architecture. Other partial ADRs should enter a plan only when its actual consumer
requires them.

## Verification Performed by This Audit

- Editor focused nextest run `d3e2034a-e6d1-49e4-9f31-5ed398921466`: **42 passed, 97 not selected**.
  Includes real widget Scene/Prefab save/reopen, custom/UI authoring, and supervised owner cleanup
  and retry tests. These are not the ignored native-window suite.
- Core nextest run `7f9c5095-1323-48f8-b3be-47386bc2a5e6`: **236 passed, 0 skipped**, across
  `nara_hierarchy`, `nara_asset`, `nara_image`, `nara_sprite_render`, and `nara_input`.
  Includes corrected hierarchy lifetime, failed-image-reload preparation, cross-kind foreground
  order, world-Y ties, fixed input, and presentation interpolation regressions.
- Independent public-API asset probe: **three removal-rejection paths reproduced**, including a
  subsequent quiet drive. The result is an observed contract violation, not a passing regression.
- Independent final document check removed an unsupported README inconsistency claim: the generic
  scalar lane installs its own exact data-adapter receipt, so the README's adapter condition is
  compatible with that implementation. Retained findings are not an unfiltered agent union.
- No formatting, production change, ignored native test, full workspace suite, or benchmark was
  performed. Existing compiler dead-code warnings were observed and were not treated as runtime
  defects.

Commands, executed serially:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --offline -E 'test(workspace::) | test(frontend::) | test(supervised_play::owner::)' --test-threads=1
cargo +1.95.0 nextest run -p nara_hierarchy -p nara_asset -p nara_image -p nara_sprite_render -p nara_input --locked --offline --test-threads=1
```

Existing happy-path green tests do not refute F01–F04: the relevant pressure/interleaving/source
divergence combinations are absent from their current regression cases. Closure requires the
specific behaviors above, not another broad test-count claim.
