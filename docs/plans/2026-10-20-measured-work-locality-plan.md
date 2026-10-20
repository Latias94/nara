# Measured Work Locality Follow-Up

Status: L1 scalar, L2 Browser, L3 Tile queue and L4 Rust/Play are complete within their qualified
scope. Remaining scalar latency and seconds of Rust compilation/startup are explicitly open
measurement results rather than universal timing guarantees.

## Evidence And Intent

The [production cost baseline](../benchmarks/2026-10-20-production-costs.md) measures ordinary
Editor actions, Browser scanning, a large mostly-offscreen Tilemap and Rust rebuild/restart.
The first narrow correction indexes admitted component schemas at `d21d8c0`; post-correction
observations belong to that report. This plan does not reopen completed foundation corrections,
select a universal async interface, or activate deferred Tile authoring tools.

Keep the existing document/history owner, exact schema readiness, managed runtime and resource
retirement contracts. Use the local 8 ms owner-call investigation trigger for prioritization,
not a supported capacity or timing assertion. A real game still chooses its admitted population.

## Ordered Units

### L1: Attribute Remaining Scalar Preparation

Disposition: [implemented and qualified](2026-10-20-scalar-preparation-locality-plan.md) at
`601b467`. The [report](../benchmarks/2026-10-20-scalar-preparation.md) records 4,096-record edit
P50 38-42 ms versus 117-120 ms, separate allocation evidence, full-validator fallback and the
remaining O(N)/whole-encoding work. This is above the 8 ms investigation trigger; further scalar
optimization needs fresh attribution of this implementation. The original selected brief follows.

First split one complete field edit and Undo into bounded observations of candidate copying,
field preparation, format validation, classification/reference checks and owner publication/drop.
Use temporary private probes in a separate exploratory process; remove them before production
verification. Retain the current public workload and its unchanged semantic oracle.

Full validation currently pretty-encodes and shape-parses JSON and RON. Compare alternatives
only after attribution: reuse immutable admitted facts and touched-record candidates, or design
an explicit pending preparation/commit interaction if expensive owned work must move off-thread.
Do not drop an encoding's acceptance, defer rejection until Save or report success before commit.

Acceptance: identical successful values, exact inverse history/presence, revision/dirty/source
state, unavailable-record preservation, field/reference/format limits and history refusal.
Compare any incremental candidate against the complete validator over accepted and rejected
cases. Repeat optimized public timings and record retained-state memory costs separately.
If pending edits change the public action-result contract, reconcile ADRs 0047/0118 before
implementing that alternative. No generic Future-based owner trait follows from this unit.

### L2: Prepare Browser Filesystem Work With Finite Custody

Disposition: [implemented and qualified](2026-10-20-browser-preparation-locality-plan.md) at
`7f78c59`. The [report](../benchmarks/2026-10-20-browser-preparation.md) separates reduced owner
occupancy from total scan/Open completion, and binds the explicit pending/result interface,
exact predecessor authority and physical task/Host retirement. Original selected brief follows.

Enumeration is the measured long owner call; terminal publication is smaller in this fixture.
Move the existing source-index attempt, capabilities and budgets into a finite owned worker
using current task pools. Preserve last-good snapshots and typed progress; coalesce Refresh,
reject stale attempt/project results and keep safe owner publication/query slicing explicit.

Acceptance: the same sorted snapshot, unsupported-entry diagnostics, byte/work limits, identity
and lineage rejection, marker insertion/removal, cancellation and retry. Hold a worker across
project Close/Drop and prove physical cleanup custody. Repeat owner-call and total-completion
measurements independently. A lower call time must not conceal unbounded work or disposal.

Browser Scene Open is a subsequent subunit: reserve one pending Open intent before preparing
immutable reads/catalog classification; admit on the current owner/index incarnation before
dirty Save/Discard/Cancel, lane retirement and replacement. Model preparation as an explicit
pending request/result in the public projection and native maintenance; do not disguise it as
Opened or AwaitingClose. Reconcile ADRs 0047/0118 before changing those action outcomes. Bind
the replacing DocumentId/revision, schema/source basis and current index rather than active
selection. Validate fallible admission facts before beginning destructive lane retirement.
Keep the old document on preparation refusal; after physical close has begun, retain explicit
close/open outcomes and custody rather than promising rollback of an irreversible close.
Preserve the existing digest re-observation unless its safety replacement is explicitly proved.
Prove terminal result acknowledgement, changed source identity/digest, replaced index, newer
intent, project close and stale completion through the ordinary public consumer.

### L3: Bound Large-Map Work To Views And Changes

Disposition: [Tile visibility locality](2026-10-20-tile-visibility-locality-plan.md) is implemented
at `15e6564`; its [report](../benchmarks/2026-10-20-tile-visibility.md) binds source/GPU oracles
and three optimized processes. Streaming candidates and conservative per-view clipping avoid
cross-frame caches; O(N) extraction/material work remains. Original selected brief follows.

First split the real managed drive into Tile extraction, per-view queueing and sort/batch work;
the baseline includes UI and is not per-function CPU attribution. Retain the fixed camera,
one Tilemap entity, static and one-cell mutation cases. Keep 65,536 cells a stress population,
not a product support promise.

Compare early per-view visibility against a private chunk index and retained geometry. Use
pinned Bevy visibility and Godot dirty-quadrant designs as references. Choose the smallest
solution that removes measured work; parallelizing all invisible cells is not locality.

Acceptance: an uncached reference oracle preserves camera affine transforms, rotation/negative
scale, pixel canvas mapping, edge coverage, sort/Y-sort order, layers, materials and resource
budgets. Multiple views cannot reuse another view's visibility. Tile edits and TileSet/Image
generation changes invalidate exact cached content; old packets retain their resources.
Map replacement/removal, entity/runtime incarnation and direct public tile size, layer, sort
key, Y-sort anchor or TileSet changes also invalidate cached geometry. A reset/cleared dirty
chunk counter is not sufficient reuse authority. A first no-cache view-culling implementation
can avoid those cache obligations if it resolves the measured cost.
Measure queued/uploaded bytes and changing/static cost separately with actual pixel readback.

### L4: Attribute The Rust And Play Iteration Path

Disposition: [Rust/Play attribution](2026-10-20-rust-play-attribution-plan.md) is complete, including
the bounded Windows Cargo path correction at `2c26c92`. Its [report](../benchmarks/2026-10-20-rust-play.md)
records baseline and final public Play, separate Restart/headless controls, private native Cargo
diagnostics, exact Supervisor-to-CLI Fresh reuse and compile-failure recovery. Profiles/linker
settings remain unchanged; mixed uncontrolled timing changes do not establish a general Rust
speed gain. Original selected brief follows.

Use the retained independent-game collector and Cargo's existing build diagnostics to identify
build units and dependency critical paths. Add bounded compiler/linker observations only when
those costs remain material; Cargo unit timings alone do not split code generation and link CPU.
Measure actual ordinary Play through a new
accepted first presentation separately from the headless process endpoint. Keep no-edit,
function-body and runtime-layout cases distinct; retain compiler/cache/profile context.

Choose a profile, dependency or link change only from those costs and validate the complete
game result. Dynamic-library ABI, native function patching and a second official language are
separate proposals with compatibility/state/retirement obligations under ADR 0093.

## Verification And Integration

Root owns serial Cargo/fmt/Git and one aggregate build budget. Assign disjoint implementation
paths, independently review each changed owner and preserve concurrent edits. Keep benchmark
data immutable per implementation revision; do not pool before/after samples or subtract
independent percentiles. Update the external HTML and the implementation evidence after each
completed unit. Native UI claims require the relevant native trajectory; source/optimized
offscreen observations alone do not grant cross-platform release qualification.
