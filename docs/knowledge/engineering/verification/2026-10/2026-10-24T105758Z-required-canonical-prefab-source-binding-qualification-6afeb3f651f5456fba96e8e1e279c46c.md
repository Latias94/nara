---
type: "Verification Evidence"
title: "Required canonical Prefab source binding qualification"
description: "All Prefab resolvers and projection steps require canonical source keys; the in-memory adapter owns one document per key and all aliases share its publication."
timestamp: 2026-10-24T10:57:58Z
record_id: "6afeb3f651f5456fba96e8e1e279c46c"
producer_id: "codex"
run_id: "session-01a1629b-required-prefab-binding"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_base: "f335d2b"
status: "qualified"
verified_by: "serial focused nextest; independent module consumer; compile-fail doctests; workspace check; scoped Clippy correctness; cargo fmt"
---

# Verification

# Result

`PrefabSourceResolver` now has one required operation returning the immutable document and
canonical key as a single borrowed `ResolvedPrefabSource`. The bare-document/default-binding
methods, optional keys and requested-reference cycle fallback are removed. In-memory authors
explicitly provide a named, location-authored or admitted stable key. Every projection step
retains its required key; ancestry and the complete copied chain still admit their logical byte
cost before copying. Root retention now charges the key unconditionally.

The in-memory adapter owns one canonical document per key and a request index. Insertion publishes
the target key's document to every alias and returns that key's previous document. A request rebind
prunes the old source only if no other request addresses it. This intentional last-write-wins
authoring policy is not a content-preserving alias operation, a history service or a substitute
for Project Content's duplicate stable-file admission refusal. The two file-candidate operations
validate before changing either index.

Production project resolvers, direct examples, integration tests, module consumers and the existing
UI fixture use the required contract. No compatibility alias or extra public resolver operation
remains. This is a dependency-ordered prerequisite to required containing-Scene materialization,
not its completion. It changes no durable file envelope or version.

# Evidence

- Initial Scene cohort: 300 passed, 1 skipped, run
  `4b268f2b-02df-48a9-bd9f-c29fb817e29c`. A subsequent multi-alias failure test and UI fixture
  correction require their final cohort below before qualification.
- Root/content/workflow cohort: 440 passed, 6 skipped, run
  `d2daa7b6-c547-405c-bd07-86a945a0cdb1`, using
  `cargo nextest run -p nara --all-features --lib --test project_content_boot
  --test project_content_limits --test project_content_delivery --test project_scene_content
  --test project_workflow_creation --test project_workflow_cli --locked -j 1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`.
- Integrated Scene/UI cohort: 390 passed, 1 skipped, run
  `25116d54-91e5-4c92-90d6-37f8981480bf`, using
  `cargo nextest run -p nara_scene -p nara_ui --all-features --locked -j 1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`. It includes the
  multi-alias file-publication rejection matrix, required-key migration and the existing UI
  provenance consumer; no shared-UI identity redesign is implied.
- Separately compiled module consumer: 17 passed, 1 skipped, run
  `befe485b-25b3-4473-90df-bb882f302b1b`, using
  `cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --target-dir target
  -p nara-scene-module-consumer --test prefab_sources --test nested_schema --test durable_identity
  --test scene_spawn --test source_context_trial --locked -j 1 --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`. The new public alias
  consumer executes alongside nested references, durable identity, exact retirement and the
  explicitly prototype-only source/occurrence experiment.
- `cargo test -p nara_scene --all-features --doc --locked -j 1`: all four compile-fail doctests
  pass, including refusal of a bare-document resolver implementation.
- After the final production simplification, all ten selected resolver/source tests pass, run
  `9c6609df-8374-4089-9249-6e3132b8c76e`, using
  `cargo nextest run -p nara_scene --all-features --locked -j 1 -E
  'test(prefab::resolver_tests) | binary(prefab_sources)' --no-fail-fast
  --failure-output immediate --status-level fail --final-status-level fail`. The 292 other
  cases were filtered or otherwise skipped; they are not 292 unresolved tests in this slice.
- Final serial integration gates passed:
  `cargo check --workspace --all-features --all-targets --locked`,
  `cargo clippy -p nara -p nara_scene -p nara_ui --all-features --all-targets --locked --
  -D clippy::correctness`, `cargo fmt --all -- --check`, and
  `cargo fmt --manifest-path tests/fixtures/module-consumer/Cargo.toml --all -- --check`.
  Existing non-blocking warnings remain; no warning-free workspace is claimed.
- `cargo check -p nara_scene --no-default-features --all-targets --locked` also passed; the
  required binding does not accidentally depend on the Serde feature union.
- Eight changed/new Markdown documents have no missing local link targets; engineering-memory
  validation and rollup checks pass with existing legacy warnings. `git diff --check` passes.
  A scoped source scan found no removed binding method, optional canonical projection key or
  optional-key use remaining in `crates`, `src`, `tests` or `examples`.
- All Cargo work is serial, `CARGO_BUILD_JOBS=1`, reusing `target`.
- This is a public interface/owner refactor with existing cycle, topology, reference rewriting,
  wrong-kind and exact provenance-budget coverage retained. New alias publication and rejection
  tests were added after the implementation; no red-before-production execution is claimed.

## Independent Review

The scoped correctness review (`01a16d7c-9919-7f21-aa6e-23ed226f2579`) found one missed UI test
callsite, which now supplies its explicit named key. That was a static finding; no failed Cargo
run is claimed. Its suggested multi-alias file-publication rejection coverage was added, including
canonical replacement and request rebind through both insertion paths. Other reviewed source,
retention, ancestry and alias-publication paths had no new deterministic finding.

The bounded architecture/ergonomics review (`01a16d7c-ced1-73b2-8515-c637a8026d0a`) supports the
adapter-local canonical map and request index, explicitly distinguishes canonical publication from
alias-only insertion, and retains source/occurrence separation. Its pinned references are Bevy
`85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. In-memory publication must not replace file-backed
identity conflict admission. Neither review executed tests.

The pipeline simplification pass (`01a16d8e-eefa-7231-bd76-bbdfe09315c1`) used the skill's
supported inline/serial fallback because its client lacked nested dispatch. It removed one key
clone by filtering the previous binding before moving the key into canonical storage. It left all
proof tests unchanged and rejected a low-value merger of the two project resolver adapters. This
pass is not an independent portable code-review receipt; final focused verification follows it.

Code review: skipped (ce-code-review unavailable). The actual delegated top-level invocation
(`01a16d94-dac1-7c20-8a37-d45134b7807e`) returned `status: failed`: its client could not discover
the mandatory native dispatch and bounded collection facilities, including the sequential route,
so it could not execute the independent merge/report leaves. No completed portable receipt is
claimed. The main owner manually reviewed the final source, migration and test diff in addition
to the native scoped correctness review above; this is recorded as the unavailable-path fallback,
not relabelled as completion of the portable workflow.

The final bounded native follow-up (`01a16d99-4115-7ed2-9b4d-fab0f4448154`) found no new defect
in the UI callsite correction, the four-combination multi-alias rejection test, clone reduction or
public cycle/refusal/republication consumer. It statically checked those named paths, not a fresh
whole-diff review, and executed no tests. The public consumer proves cycle refusal and recovery;
the existing `max_depth(2)` test separately constrains earliest cross-alias cycle detection. The
rejection matrix uses a structural error and does not claim DB-specific asset-failure coverage.

# Follow-up

Required root-source candidates and Scene-owned active occurrence remain the next owning
replacement, with unbound ordinary preparation and shared Runtime/Canvas lowering afterward.
The design now records the independent lifecycle findings: revalidate deferred registration/domain
proofs before publication, preserve empty and individually emptied occurrence occupancy, use
domain/activation association rather than a changing entity roster, admit private-resource
lifecycle, and include Scene association in projection epoch validation. These are integration
obligations, not evidence that the unimplemented interfaces already satisfy them.

Current Runtime UI still encodes Prefab lookup requests. Mandatory provenance keys do not silently
fix canonical placement equality, containing-source collisions, focus/actions or cache continuity.
No new native Editor, Linux/macOS product, performance or compound-importer qualification is claimed.

# Citations

- [Resolver and canonical storage](../../../../../crates/nara_scene/src/prefab.rs)
- [Required provenance](../../../../../crates/nara_scene/src/provenance.rs)
- [Public failure and budget tests](../../../../../crates/nara_scene/tests/prefab_sources.rs)
- [Independent public consumer](../../../../../tests/fixtures/module-consumer/tests/prefab_sources.rs)
- [Source and occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
