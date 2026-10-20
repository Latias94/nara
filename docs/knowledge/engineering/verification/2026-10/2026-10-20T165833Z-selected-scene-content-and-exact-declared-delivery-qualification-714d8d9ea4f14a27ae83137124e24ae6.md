---
type: "Verification Evidence"
title: "Selected Scene content and exact declared delivery qualification"
description: "Verification Evidence for Selected Scene content and exact declared delivery qualification."
timestamp: 2026-10-20T16:58:33Z
record_id: "714d8d9ea4f14a27ae83137124e24ae6"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-persistent-two-level-game-plan.md"
git_branch: "main"
git_commit: "08b94f9"
---

# Verification

P1 in the persistent-game plan is qualified on Windows 11 with Rust 1.95.0. The metadata
`git_commit` is the starting committed baseline; this record accompanies the P1 implementation
diff. Root runs Cargo serially with one build job and reuses `target`. The unrelated `vendor/`
tree and read-only reference repositories are preserved. No new Goal is created.

| Actual command family | Result | Nextest run / local evidence |
|---|---|---|
| `cargo +1.95.0 nextest run -p nara_project --locked --offline --test-threads=1` | 52 pass | `7c9e6f20-eebb-4481-956a-99094f9c63fb`; `target/two-level-project-content-manifest.log` |
| Root all-feature content/limits/boundary/runtime/check cohort | 68 of 69 pass; one new fixture assumption fails | `d120853d-b128-48ef-983b-ce3ef8c2a46b`; `target/two-level-content-public-final.log` |
| Root all-feature library plus selected Scene/delivery/Check cohort | 300 of 302 pass; two new test assumptions fail | `8f55f7fb-73c7-4712-b28b-d788bb6cce91`; `target/two-level-content-qualified-cohort.log` |
| Final affected delivery and export-owner cohort, after correcting those assumptions | 13 pass, including all nine delivery cases | `5923f68b-696c-419a-b051-5e005a74e678`; `target/two-level-content-final-corrections.log` |
| Real compiled custom-schema Check/Run/Repair/Export | 1 pass, 659.203 seconds including release building and product observations | `f4a88bd8-701e-4429-a30d-aed71953c2fb`; `target/two-level-content-real-cli-diagnosis.log` |
| Final real CLI, legacy-entry refusal and starter creation after the count/guide correction | 12 pass; real trajectory 131.608 seconds | `36a88483-8a62-4526-90ba-d531944e7f2d`; `target/two-level-content-real-cli-final.log` |
| Reference Game all-feature `first_wave`, `headless_snapshot`, `plugin_composition`, `prefab_startup`, `project_content_boot`, `ui_authoring_roundtrip`, `vec2_schema_migration` | 32 pass | `fb0b116b-573d-4517-b565-ec8fde2c59c6`; `target/two-level-reference-consumers.log` |
| Independent module-consumer workspace `check --all-targets --locked --offline` | pass | `target/two-level-independent-module-check.log` |
| Independent `nara-scene-module-consumer` component inventory | 3 pass, including public editing, Save/Reopen and strict Check | `41a488a6-6f3a-427a-a146-d613643dd3cd`; `target/two-level-independent-inventory.log` |
| Final `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline` | pass after the final code changes | `target/two-level-workspace-final-head-check.log` |
| Root, Reference Game and module-consumer workspace `cargo fmt --all -- --check` | pass | Root serial integration command |
| Independent workflow consumer `cargo clippy ... -p nara-project-workflow-consumer --all-targets --locked --offline -- -D warnings` | pass; dependency feature subsets retain existing unused/dead-code warnings | `target/two-level-independent-workflow-clippy.log` |

Engineering-memory render and validation pass with historical migration advisories. The standalone
HTML parses, has unique section IDs and resolves its local section links. No new native Editor
presentation qualification is required for this selected-content API/delivery change; its affected
Editor targets compile and the ordinary owner and independent product consumers above run.

The final CLI command selects `--test project_workflow_cli --test project_workflow_creation`,
`--run-ignored all` and the two expensive CLI cases plus the creation binary. The owner correction
command selects library `project_workflow::export::source_tests` and the delivery binary. Commands
use `-p nara --all-features --locked --offline --test-threads=1 --no-fail-fast` where applicable.
These bounded passing results are not presented as one clean final run of all 302 earlier cases.

# Result

Project manifests admit up to 64 explicit Scene roots plus implicit startup. Effective profile
directories apply to the project-wide root list. Selected loading preserves exact root/settings/
schema authority and binds the selected path; only actual startup selection can produce the
startup wrapper. Generic source facts, startup agreement and declared delivery have separate
types. The delivery owner loads roots sequentially with the original shared loader budget,
releases decoded snapshots and retains a sorted, bounded union of source facts.

Check and Export consume this union. Build receipts revalidate the current executable, manifest,
startup content, closure, bundle and schema rather than accepting unrelated fresh startup facts.
Source revalidation covers the entire delivery before staging and before publication. Protocol 2
wire messages, Cargo declarations, starter self-validation and actual consumers agree. Reported
`content_members` excludes the two executable members; the final real CLI test checks the count
against a delivery loaded from the independent published tree.

# Evidence

The compiled project owns a custom schema, upgrades a persisted predecessor and proves that
Check/Repair do not execute gameplay. Export obtains both exact product observations. The test
moves the development tree away, opens the published root under a different native identity,
loads its second Scene with expanded Prefab provenance and Image bytes, excludes an undeclared
invalid Scene, and runs the published headless executable from an unrelated CWD. TileSet and
Image/metadata transitive delivery is separately tested through the same public Check owner.

Three independent lanes implement or inspect content, manifest and consumers, with disjoint
live ownership. Final read-only review catches and closes the old protocol metadata declaration,
the build-observation retargeting gap and the export count label. Budget review preserves the
same loader and atomically transfers work to retained facts. A focused union Files limit proves
that individually admissible Scenes can still reject as a delivery union and refund all charges.

The failed new tests had three incorrect assumptions: bare `load` lacks the checked authoring
bundle member; a path Prefab's unused optional metadata is not a parsing dependency; and a fresh
receipt for changed B must pass while its old receipt rejects. Corrections retain precise member
comparisons, use a real stable-ID dependency, and test both old/fresh facts. One earlier CLI
attempt reports a creation staging orphan before any project build; ten ordinary creation tests,
a same-checkout-volume creation probe and both later complete trajectories pass. The first
attempt lacks the native cause and is not attributed to a product defect or hidden by retries.
A temporary failure-only CLI diagnostic used during investigation is removed before final tests.

Export-owner negative tests synthesize executable bytes and observation facts only to exercise
receipt matching. They do not claim those bytes ran; the real compiled CLI cases provide the
positive process and first-presentation evidence. The measured CLI times are cached observations
and are not comparable cold-build or speed-improvement claims.

# Follow-up

P2/P3 remain open: one committed source/instance owner, current-level Retry, acyclic decoded
generation retention, resource residency preflight/retirement, owned background preparation,
A -> B -> A and fresh-process game-owned checkpoint restoration. `scene_source()` still returns
the provisional startup-named type; current startup retention is deliberately corrected by P2,
not relabeled as complete unload by this delivery qualification. Existing wave CLI/layout remain.

Use ordinary gameplay/save modules or the established local Cargo crate pattern for the new
consumer. The closed five-file build observation is not a module resolver or complete build-input
snapshot; executable digests own the runtime bytes. After P1-P3, audit reported deprecations
against supported Rust versions and measure the enabled Bevy/Nara task dependency graph before
selecting scheduler or dependency changes.

Separate hosted evidence at `56840a4` now completes Windows Root: 2,670 default tests
(`f01b4046-9ffe-43ca-bd41-cd6a2a370e39`), 2,692 all-feature tests
(`75258dae-3cca-4eb1-9e27-d65ec0154ea0`) and 61 Dear-adapter tests
(`eb680c22-1178-4d2d-a14a-9ff71dfc8fff`) pass. Linux Root has the separately documented
single native-display failure; `08b94f9` corrects its default Xvfb profile, with successor hosted
qualification pending. Those old-source hosted results do not qualify this new P1 source.

# Citations

- [Plan](../../../../plans/2026-10-20-persistent-two-level-game-plan.md)
- [ADR 0110](../../../../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
- [Source-only delivery owner](../../../../../src/project_content/delivery.rs)
- [Public delivery tests](../../../../../tests/project_content_delivery.rs)
- [Real CLI trajectory](../../../../../tests/project_workflow_cli.rs)
- [Linux default display qualification](2026-10-20T163530Z-linux-default-workspace-suite-requires-the-native-display-profile-799d137d781d4786abfa72505101cdc0.md)
- Standalone HTML: `D:/Frankorz/Downloads/nara-module-goal-alignment-2026-10-20.html`
