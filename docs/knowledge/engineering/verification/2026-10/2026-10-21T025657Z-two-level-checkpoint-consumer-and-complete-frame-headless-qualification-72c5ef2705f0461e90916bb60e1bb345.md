---
type: "Verification Evidence"
title: "Two-level checkpoint consumer and complete-frame headless qualification"
description: "Verification Evidence for Two-level checkpoint consumer and complete-frame headless qualification."
timestamp: 2026-10-21T02:56:57Z
record_id: "72c5ef2705f0461e90916bb60e1bb345"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-persistent-two-level-game-plan.md"
git_branch: "main"
git_commit: "23b741d"
verified_by: "serial nextest and exported product processes"
---

# Scope And Result

P1-P3's bounded Windows consumer is implemented in `samples/two-level-game`. It uses public
product, semantic command, Scene and filesystem APIs, with ordinary private Rust modules rather
than a new general Save service or SceneManager. It preserves the separate wave Reference Game.
ADRs 0027/0110 now distinguish game-owned checkpoint selection and full-frame headless execution
from component registration, exact stepping and delivery receipts.

The sample prepares other referenced sources sequentially, then prepares/validates/commits its
final target directly. Repeated Retry uses the retained committed source and expected instance
receipt even after its file becomes invalid. Dynamic marker observation checks actual entities,
tokens, IDs, transforms and logical records; creating another marker after restore preserves
already placed roots. Preparing ignores new movement presses but always consumes releases.

# Verification

Root runs Cargo serially, reuses `target` and sets `CARGO_BUILD_JOBS=1`.

- Rust 1.95.0 `project_runtime_boot` all-feature nextest: **19/19 pass, one pre-existing skip**,
  run `0a8fd929-cff6-48f3-9451-d5c9642b8b78`, compile 46.69 s, tests 4.337 s.
  `target/headless-frame-final-tests.log` covers exact/complete-frame distinction, zero delta,
  zero time scale with frame services, terminal observation, frame/tick bounds, retained cleanup,
  startup resources and existing product failure paths.
- Rust 1.95.0 sample nextest after final queue simplification: **24/24 pass**,
  run `bad9dfda-911d-449f-9f82-849bb86d7233`, compile 32.26 s, tests 2.162 s.
  `target/two-level-consumer-final-tests.log` includes actual separate save/restore processes,
  A -> B -> alternate B -> original B -> Retry -> A, distinct logical object keys, source change,
  corrupt/missing logical data, migration, external write conflict, uncertain readback and interrupted
  temporary-file custody. The movement recovery regression uses public command admission.
- Final Rust 1.95.0 locked workspace all-feature/all-target check passes in **1m33s**;
  the sample's locked all-feature/all-target check passes in **10.88 s**.
  Logs: `target/two-level-workspace-final-check.log` and
  `target/two-level-all-features-final-check.log`. Existing feature-subset dead-code/unused warnings
  remain; this is not a warning-free build claim.
- Root and sample `cargo +1.95.0 fmt --all -- --check` and intended diff whitespace checks pass.
- Product protocol 2 schema generation, schema-check and strict Check pass; the declared delivery
  contains **8 members** across both levels and their Image/metadata closure.
- Ordinary protocol-2 Export builds both selected release binaries using the installed default
  `rustc 1.99.0 (b940084d7 2026-09-28)`. Initial release compilation takes **8m43s**; this is not
  attributed build-speed or dependency-cost evidence. Export reports `success=true`, **8 members**
  and **2 exact product observations**, including real desktop first presentation and normal close.
  Local log: `target/two-level-export.log`.
- The published `nara-two-level-qualification-20261021T024608Z/candidate` lives outside the checkout.
  Direct child invocations from its unrelated parent run `--verify-travel`, `--verify-save` and
  `--verify-restore`, with a fresh process for each and an external 90-second deadline. All exit 0
  and publish `verified=true`. Restore returns to Meadow with score 30 and three collected facts;
  the file readback has format version 2. Logs: `target/two-level-exported-*.stdout.log` and
  `*.stderr.log`; the local parent location is retained in `target/two-level-export-parent.txt`.

# Retained Failures And Review

The first real travel hit the exact-step tick bound: PreUpdate never produced the script's commands,
and TaskUpdate never integrated prepared results. The correction adds explicit full managed frames
through the existing Runtime/App owner; exact Editor stepping is unchanged. A new proof initially
expected no pause maintenance in exact mode; the actual existing pause frame runs TaskUpdate/Last,
so its characterization now retains that behavior. A later zero-scale test initially used a private
fixture field; it uses the fixture's existing public path accessor after the compile rejection.

The first complete consumer run passed 23/24 and caught a real reconstruction bug: adding marker 4
after restore tried to Place already placed marker 2. The game now recognizes existing roots and
rejects unexpected parenting. The same fresh-process test passes after the fix.

Independent read-only headless design and consumer source reviews found no remaining blocker.
Their timeout/test/document corrections and the simplification pass's duplicate preparation finding
were applied. No alternate model tool or concurrent Cargo was used.

Code review: skipped (ce-code-review unavailable). The delegated skill attempt and its recovery
returned prose without the requested structured receipt, so no completed CE receipt is claimed.
The independent scoped source reviews and root's explicit final diff scan remain the actual review
coverage; they are not presented as the missing workflow receipt.

# Limits And Follow-Up

This proves the selected checkpoint workflow on Windows, not arbitrary-tick snapshots, ambient
World serialization, additive active scenes, native rollback or a complete interactive key trajectory.
Injected postcommit native reconstruction recovery is not qualified; first physics/audio consumers
must establish their own acquisition and reconstruction contracts.

Linux failed-candidate cleanup still cannot unlink by exact opened-object identity under the current
filesystem adapter. The save owner reports CleanupPending and remains retained; it never replaces
that guarantee with unchecked pathname deletion. Normal publication and conflict/uncertain states
retain their separate actual evidence. No complete Linux/macOS game qualification is claimed here.

Hosted CI `39463001464` at **3a9284a** is now **7/7 success**, including Windows/Linux and portable
wasm compilation. New Windows/Linux sample formatting/build/headless gates are added, but that
earlier hosted result does not qualify these later local commits. Browser execution remains outside
the portable wasm check. Deprecation and measured task/dependency costs remain the ordered follow-up.

# Citations

- [Plan](../../../../plans/2026-10-20-persistent-two-level-game-plan.md)
- [Sample guide](../../../../../samples/two-level-game/README.md)
- [Headless product owner](../../../../../src/project_host/runtime/action.rs)
- [Earlier residency/request proof](2026-10-20T183137Z-hosted-scene-residency-and-owned-preparation-qualification-d8d63279f87c4b34bc83eeeccdc7550e.md)
