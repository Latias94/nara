---
type: "Verification Evidence"
title: "Nested instance repair baseline and effective source-demand replacement design"
description: "Current exact nested replacement avoids discarded sources; the Proposed successor now separates source preservation, selection, activation and ordering."
timestamp: 2026-10-26T10:38:59Z
record_id: "a6dbd3843c104c40b6ba817e428478f7"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/canonical-source-key-codec"
base_commit: "c31c6dc"
---

# Verification

Qualification of the current production expansion baseline, and independent review of the next
declarative overlay replacement. The five new tests use public Scene expansion and an
instrumented in-memory resolver; no production algorithm or persistent override type changes.

# Result

- Current exact `SetPrefabInstance` inside an outer instance override replaces P's nested
  anchor n before resolving its old Q. Missing Q and a registered Q with invalid topology
  are never requested; the only requests are P/R. Complete canonical provenance and effective
  hierarchy identify R, while P-local anchor components/children and Scene-local children survive.
- Detach resolves only P, preserves n's own components and P-local children, and does not
  convert Q's projection to local records. Complete subtree removal resolves only P and removes
  n's containing-source descendants too, retaining unrelated siblings/Scene-local data.
- A stale expected declaration with the same Q lookup but a different embedded override rejects
  with the exact context-mismatch diagnostic. No candidate/provenance publishes, and neither
  Q nor R is requested. Every test checks original Scene and resolver source documents unchanged.
- Proposed instance pins replace complete optional declarations before recursion, without durable
  old CAS snapshots. Exact CAS/history remains in containing-source transactions. Ordinary intents
  compile by active layer from inner to outer; forwarded bindings retain original scope.
- Proposed additions carry complete components, optional nested instances and per-component
  bindings, with placements as the sole hierarchy authority. Optional overlay absence permits a
  normal unresolved source link; an existing empty overlay retains its explicit canonical base.
- Actual source demand belongs to the same Scene selection kernel, not a second Host interpreter.
  The first implementation direction is a synchronous fallible mutable loading callback returning
  owned shared immutable key/document results. Host retains filesystem/budget/receipt ownership.
  Share raw sources, not occurrence-specific compiled output. A continuation needs an actual
  yield/progress requirement; no universal job framework or full compile restart per source.

# Evidence

Rust 1.95.0, serial Cargo, `CARGO_BUILD_JOBS=1`, existing root target:

- All-feature `prefab_instance_expansion` run `d9464703-d193-439d-b6d3-3e3a15b816f5`:
  all 5 passed in 0.302 seconds after a 5.39-second build.
- No-default-feature run `e5573590-ea90-4dd0-98d9-1356fdd21de9`: all 5 passed in
  0.324 seconds after a 1m05s build, with existing dead-code warnings. This baseline does not
  depend on serde or an Editor/Host.
- All-feature correctness Clippy for this integration target passed in 2.27 seconds with existing
  library warnings; the new target adds no warnings. Workspace fmt and diff whitespace pass.
- Changed-document local link targets pass (846 checked, zero missing).
- Engineering memory validation and derived-view checks pass with existing historical warnings.
- The preceding codec checkpoint's 429 Scene tests retain their own source and scope; those
  results are not recounted as a new full run after adding this separate test target. No
  Runtime/Authoring Host, JSON/RON file repair or native Editor trajectory ran for these tests.

Wegener `01a177be-9dea-7262-87fc-e8cd0fd95aeb` wrote only the new integration test file,
without Cargo/fmt/commit/spawn. Parent inspected, formatted and executed it; worker is closed.
Averroes `01a177b1-0004-7563-830f-38e7dc508335` supplied the read-only production-selection
study distinguishing syntactically reachable old behavior from actual source/Host qualification;
it ran no tests and is closed.

Volta `01a177c0-a887-7461-9576-4664723911d7` independently reviewed the Proposed design
and all five tests, with no test findings and two confirmed design findings:

1. P1: replacing closure scanning alone is insufficient. Current native Prefab canonicalization
   recursively validates old embedded overlays before selection. A decodable P whose discarded
   overlay references an obsolete field could reject before n is replaced. Design now requires
   bounded source-preservation declarations before selection, and activation semantics only for
   selected surviving declarations. Inactive payloads retain raw budgets/evidence; syntax and
   active semantic failures remain errors. Future 0043/0090 reconciliation is explicit, while
   exact transaction snapshot rules stay unchanged. A real Host acceptance case is added.
2. P2: flat post-recursion placement could insert a's local child before its nested Q roots.
   Design now preserves ADR 0101's nested-root prefix and containing-layer child sequence,
   bounds First/Before/After to the relevant sequence, and rejects cross-scope ordering.
   Explicit reparenting preserves origin, and same-parent reorder cannot transfer ordering scope.
   Follow-up review found that the initial wording could incorrectly classify `Parent::Root`
   by its physical outer anchor. The correction retains the original declaring overlay's effective
   root sequence, permits moved-in q before inherited p, removes moved-out roots from old prefixes,
   and applies the local-child assignment rule only to `Parent::Entity`. Both added-instance and
   nested-root reparent acceptance cases are explicit; neither is claimed executed.

Volta's final narrow re-review closes both findings at the design level, with no new findings.
It also confirms that inactive asset inventory cannot force successful Runtime reads and that
effective/default asset demand stays with the existing Schema owner. All review was read-only,
with no Cargo/fmt/commit; Volta is closed. This is design closure, not production or native
qualification of the successor.

# Follow-up

Complete production overlay replacement remains active. The next implementation must include
the new data grammar, aggregate pre-copy admission, effective declaration selection and actual
Host source loading, selected activation, persistence/classification/history/repair, actual
immutable closure fences and convert-to-local. No Accepted durable override contract changed in
this design checkpoint; production still embeds exact patches.

The five current tests cannot prove stale selectors in discarded embedded overlays survive
real file admission; they intentionally exercise a lawful old embedded addition. The malformed-Q
case is an invalid already-constructed in-memory topology, not malformed encoded bytes.
Rescued descendants, multilayer declarative pins, cross-scope placement, effective dependency
loading and source/Schema publication changes remain explicit production acceptance cases.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Proposed overlay design](../../../../architecture/prefab-overlay-design.md)
- [Current ordered placement](../../../../architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md)
- [Source preservation](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [Exact snapshot policy](../../../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md)
- [Unity/Godot editing study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Canonical identity predecessor](2026-10-26T103001Z-canonical-source-identity-codec-and-complete-nested-overlay-round-trips-3ce62b507a1a445582c811954dac28c6.md)
- `crates/nara_scene/tests/prefab_instance_expansion.rs`
- `crates/nara_scene/src/prefab.rs`, `migration.rs`
- `src/project_content.rs`, `src/project_host/authoring/project.rs`
