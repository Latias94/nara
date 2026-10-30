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
