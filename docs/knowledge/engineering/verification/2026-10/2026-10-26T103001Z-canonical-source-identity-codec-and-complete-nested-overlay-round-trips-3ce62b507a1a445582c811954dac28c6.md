---
type: "Verification Evidence"
title: "Canonical source identity codec and complete nested overlay round trips"
description: "Strict source-owned JSON/RON identity claims preserve all kind/origin axes without granting source admission."
timestamp: 2026-10-26T10:30:01Z
record_id: "3ce62b507a1a445582c811954dac28c6"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/canonical-source-key-codec"
base_commit: "4f4acbb"
---

# Verification

Canonical source identity value encoding under ADRs 0038/0043, and direct use of that owner in
the private structural/reference overlay comparison. This is a production value codec with a
private comparison consumer, not adoption of the proposed durable overlay format.

# Result

- `SourceDocumentKey` implements serde behind the existing feature, using private borrowed
  serialization and owned decoding. Kind and complete Named/ProjectLocation/StableAsset origin
  remain distinct. UUID parsing and existing key/path constructors validate decoded claims;
  unknown, duplicate, missing or legacy fields do not fall back to lookup or Debug spelling.
- No new envelope, source resolver, ID service, name-length policy or compatibility reader.
  A decoded key cannot admit a source. The containing format still owns preflight and aggregate
  budgets; the actual Host owns immutable source and metadata association.
- Removed the comparison's separate `SourceIdentity`, which lost document kind. Root and every
  nested base predicate now compare the actual canonical key. Existing logical retention
  accounting supplies the target-key cost.
- A real repeated mixed-origin fixture round-trips JSON/RON targets, nested reference bindings
  and placements, then compiles to the same effective document. Changing only root kind gives
  SourceChanged; changing only nested kind gives MissingTarget. The latter retains its complete
  declaration and conflict through RON. This is controlled comparison data, not hostile file or
  actual conflict Save/Reopen qualification.
- Private `ComponentIntent` now uses adjacent tags. Its previous internal tag lost nested native
  RON enum discriminants through buffering. The complete-value round trip remains in the test;
  it was not reduced to scalar identity-only serialization to hide that incompatibility.

# Evidence

Rust 1.95.0, serial Cargo with `CARGO_BUILD_JOBS=1`, existing root target:

- Initial focused run `02f13c5d-df1c-40dd-959a-f1a969369738`: 40 passed, 1 failed.
  Full RON overlay decode reported `InvalidValueForType`; adjacent component-intent tags
  corrected that comparison grammar. The initial test-name filter did not select the integration
  binary; it was corrected to `binary(source_document_key)` rather than counting unrun tests.
- Focused run `dac22fe8-db66-4132-bec6-970b85ce4233`: all 47 passed, including the six
  public codec tests and 41 private comparison tests.
- Enlarging the round-trip fixture initially failed compilation because the parent added
  `unwrap()` to an existing helper that already returns a prepared document. Corrected the
  test call, without changing the helper or product behavior.
- Full run `e93c14e0-ad27-41d7-840e-4c9685beb106`: 428 passed, 1 failed, 3 skipped.
  The new placement fixture incorrectly named its added anchor with Source origin. Corrected
  it to Addition, preserving the complete placement assertion. Focused successor
  `277ab8f5-5562-4201-8961-f769e7bca7b3` passed.
- Final all-feature Scene run `fe7ffc03-b713-40f8-bc25-22694b421fbb`: 429 passed,
  3 skipped across 24 binaries in 28.240 seconds after a 7.80-second build. Includes exact
  codec grammar, malformed JSON/RON, identity-axis distinction and the final expanded fixture.
- `cargo +1.95.0 check -p nara_scene --no-default-features --locked` passed in 2.69 seconds,
  with existing dead-code warnings.
- `cargo +1.95.0 clippy -p nara_scene --all-features --all-targets --locked -- -D clippy::correctness`
  passed in 25.37 seconds, with existing warnings. This is affected-crate correctness scope,
  not a new whole-workspace or native product qualification.
- Workspace formatting and diff whitespace pass. Changed-document local Markdown targets
  pass (1,024 checked, zero missing). Engineering memory validates and its derived views are
  current, with existing historical/advisory warnings. A formatting check initially caught the
  concurrently completed new regression-test file; the parent inspected and formatted that
  worker-owned new file after source-ready, then reran the check successfully.

Leibniz `01a177b0-a902-7f60-847f-bf9bec45a7f8` implemented only the source value codec and
six integration tests, without Cargo/fmt/commit. Parent integrated the private consumer and
owns all executed results. Hypatia `01a177b8-f3b7-7ce0-8fb5-aaa9fd11f503` independently
reviewed the codec, constructor/feature boundaries and private consumer read-only, with no
findings; it ran no tests. Both agents are closed. A later parent test-fixture correction changes
only Source versus Addition at the explicit placement parent and passes the final run.

# Follow-up

The full foundation Goal stays active. Production `PrefabInstance.overrides` remains
`ScenePatchDocument`. Complete added instance payloads, declarative instance-field claims,
pre-recursion effective declaration selection, actual Host closure demand, source history/repair,
conflict Save/Reopen and convert-to-local remain the next replacement unit. The new Proposed
sections in the overlay design record the nested missing-base counterexample and source-demand
boundary; they do not amend the accepted durable storage contract.

No Editor widget/runtime behavior or native presentation changed in this checkpoint. Existing
native qualification is not rerun or upgraded by these value-codec tests. No performance,
complete hostile-overlay admission or broad Inspector discovery claim is made.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Source identity owner](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Canonical file policy](../../../../architecture/adr/0043-scene-prefab-and-patch-document-migration-policy.md)
- [Overlay replacement](../../../../architecture/prefab-overlay-design.md)
- `crates/nara_scene/src/source.rs`, `source/wire.rs`, `tests/source_document_key.rs`
- `crates/nara_scene/src/prefab/overlay_structure_comparison.rs` and its tests/reference consumer
