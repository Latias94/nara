---
type: "Verification Evidence"
title: "Source-context owner experiment and production entry gaps"
description: "Flat direct/file source and World-bound occurrence intent evidence; production Scene preparation, canonical Prefab and shared UI adoption remain open."
timestamp: 2026-10-24T03:44:32Z
record_id: "b9598e00692a49e8ac835a63a3d4384a"
producer_id: "codex"
run_id: "session-01a1629b-source-context-trial"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "e66ee61"
verified_by: "serial offline nextest; independent native policy and adversarial reviews; focused ce-code-review receipt"
---

# Verification

All Cargo work was serial, root-owned, offline, with `CARGO_BUILD_JOBS=1` and the shared checkout
target. Only the independently compiled module-consumer fixture and the Proposed design change.

- The initial exploratory five-test run passed (`c2dac1b1`). This was characterization/experiment
  work, not a red-before-fix production claim. No engine implementation changed.
- The expanded source trial passed six tests, with its explicit timing test skipped (`4c1bf7da`).
- After review follow-up, `cargo nextest run --locked --offline --manifest-path
  tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test scene_spawn --test
  property_authoring --test source_context_trial --test-threads=1` passed 11 tests, 1 skipped
  (`a77ea193`). This combines six experimental cases with five existing ordinary public-consumer
  cases; it is not eleven production source-context conformance tests.
- The explicit ignored timing run passed (`b7240758`). For one flat one-entity Scene in an
  unoptimized Windows test build, after 16 warmups, 64 samples reported intent admission/capture
  P50 1,300 ns / P95 1,600 ns and commit P50 270,400 ns / P95 320,700 ns. Commit includes the actual
  existing Scene decode/preparation/replacement. Intent timing is not engine preparation timing.
- The disposable tagged binary payload was 58 bytes for the measured named source/slot/entity.
  It is not an existing UI key encoding or a durable file format. The size-limit experiment uses
  the public kernel limit setter, not Runtime/Canvas key projection or UI compilation.
- Final cleanup boxes the experimental diagnostic failure lazily, removing its new large-Result
  warnings without allocating that box on successful publication. The final 11-test cohort passes
  (`069cf6f7`, 1 skipped). Focused fixture Clippy correctness passes with no fixture warning; ten
  pre-existing root warnings remain, so this is not workspace warning-clean evidence.
- The final explicit timing rerun (`a288f829`) reports the same 58-byte payload, intent P50 1,200 ns
  / P95 1,900 ns and commit P50 268,500 ns / P95 412,900 ns. Prior observations above remain prior
  samples, not a before/after performance comparison. None is a release-performance claim.

# Result

The flat experiment supports a dedicated authored non-nil source-universe UUID and explicit
native-document policy as a viable direction. Named, stable-asset/product and location origins
are distinct structured coordinates. Stable source equality does not depend on the receiving
universe. There is no metadata-first fallback, load-time minting, source rewrite or UI-generated
namespace. Location admission refuses competing adjacent metadata; stable path/ID requests both
require current matching metadata and Scene kind.

The prototype owns its World and active slot claims together and delegates materialization,
replacement and exact retirement to current public Scene APIs. Two logical slots can reuse the
same bound source. A slot cannot gain a second occupant by changing source. Empty instances,
source switches, failed Scene admission, stale replacement/end and independent retirement pass.
Intents borrow an immutable bound candidate and cannot commit in another World.

Actual canonical Scene/metadata files are read, moved and reopened. Clearing a disposable cache
file and moving the entire root preserves declared location identity; renaming the location
changes it. Updating authored stable metadata after moving a Scene preserves its key, including
explicit path admission outside the narrow initial metadata scan root. Path/ID aliases and
equal/different source/occurrence/product encodings have direct byte assertions.

This is not the complete production trial exit. The real project manifest/Loader/Browser,
canonical nested Prefab resolver, retention custody and Runtime/Canvas identities remain unchanged.
Named imported products are distinguished only as key data, not decoded or materialized here.
No shared UI refreeze, native Editor qualification, new Accepted ADR or foundation completion is
claimed. The private prototype owner must be removed when its responsibility moves into Scene;
it is not a compatibility adapter or a second production identity service.

# Evidence

The source-policy reviewer `01a16be9-9ad9-74e3-ad65-e8e7a3cf88d9` confirmed a real existing mixed
input: the authoring rename test uses a metadata-free Scene with a stable-metadata Prefab. A root
policy therefore needs an explicit production decision: migrate that workflow to all-stable
native identity with authored Scene metadata, or establish a named requirement for mixed identity
policies. A path request alone does not require location identity. No exception list is adopted.

The same review identified production integration obligations that a simple catalogue edit would
miss: selected Scene metadata is not in the retained catalogue today; Prefab metadata admission
can depend on catalogue presence; Browser Open permits native documents outside configured roots;
metadata observations must enter re-admission and closure capture. Existing domain-relative
catalogue paths cannot be mixed with full project-relative Scene paths without owning context.

A focused `ce-code-review` completed in the root context with a native same-family adversarial
review, zero retained actionable findings and explicit residual/testing gaps. Receipt run:
`nara-review-e67ee8cdfd13430e979d201d8066cfdb`, `status: complete`, four staged fixture files against
`e66ee61`. External review was prohibited; no cross-model independence is claimed. The reviewer
initially stopped on the root-owned concurrent design edit; attribution was reconciled and the
same unfinished read-only review resumed. No unknown user change was discarded.

Review follow-up makes the boundary more honest: the pre-commit object is `PlacementIntent`, not
a decoded Scene preparation; it now carries World binding, and tests compare encoded bytes as
well as structural keys. A second temporary project proves stable-key independence from the
receiving universe. The final root diff scan and serial tests cover those corrections. Initially
vacant intents describe current absence, not historical vacancy generations; no permanent
occurrence-history service is added. Both review agents were closed.

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` keeps source/path/label separate in
`bevy_asset::AssetPath`. Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` resolves UID/path
through the resource owner. These support separating source resolution from placement and runtime
activation; they do not justify copying a global UID service, inferring continuity from paths or
using a runtime counter as source identity.

# Follow-up

Do not freeze public source types based only on this helper. The next production-facing trial must
cover canonical Prefab sources, the selected Scene and Browser admission together, and prepared
Scene placement with real cancellation/custody semantics. The current advanced prepared replacement
entry exposes asset bindings, native claims, product limits/callbacks and scratch-entity disposal.
Compare a source-owned allocation-delayed preparation against explicit World-bound preparation;
do not call slot-intent timing a solution to that interface problem.

Reconcile the native identity policy and metadata locator coordinates before updating project
schema/creation inputs. No load-time migration or new migration guide. Carry source/occurrence
through publication, exact end and both UI projections; then amend/refreeze ADR 0102 under owning
0038/0058 contracts. Do not promote all of Proposed ADR 0083. Preserve the explicit remaining R2,
R5b and later delivery waves; the full goal remains Active.

Filesystem spelling, symlink/hardlink aliases, simultaneous capture, dynamic products, actual
Editor interaction and release performance remain unqualified. Their absence is not a claim that
the current production implementation already meets the proposed contract.

# Citations

- [Executable source experiment](../../../../../tests/fixtures/module-consumer/tests/source_context_trial.rs)
- [Private experimental owner](../../../../../tests/fixtures/module-consumer/tests/support/source_context_trial.rs)
- [Proposed owning design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Existing mixed-source workflow](../../../../../src/project_host/authoring/mod.rs)
- [Selected Scene loader](../../../../../src/project_content.rs)
- [Metadata catalogue](../../../../../src/project_content/catalogue.rs)
- [Advanced prepared publication](../../../../../crates/nara_scene/src/spawn.rs)
- [Bevy source/path/label](../../../../../repo-ref/bevy/crates/bevy_asset/src/path.rs)
- [Godot resource UID resolution](../../../../../repo-ref/godot/core/io/resource_uid.cpp)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
