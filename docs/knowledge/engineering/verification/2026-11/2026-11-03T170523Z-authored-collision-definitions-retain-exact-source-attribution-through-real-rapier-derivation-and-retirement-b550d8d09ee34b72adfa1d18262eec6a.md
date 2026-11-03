---
type: "Verification Evidence"
title: "Authored collision definitions retain exact source attribution through real Rapier derivation and retirement"
description: "Public exact Schema and Scene editing, native collider/debug observations, full membership, mixed-input refusal and managed close qualify the finite collision workflow."
timestamp: 2026-11-03T17:05:23Z
record_id: "b550d8d09ee34b72adfa1d18262eec6a"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-04-authored-collision-derivation-plan.md"
git_branch: "refactor/foundation"
git_commit: "425f141"
---

# Verification

Windows 11, Rust 1.95.0, `RUSTUP_TOOLCHAIN=1.95.0`, `CARGO_BUILD_JOBS=1`, root `target` and
serial Cargo invocations. The base is `425f141`; this record covers its independent collision
authoring successor. Production engine source and native rendering/window paths are unchanged.

| Gate | Result |
| --- | --- |
| Final default motion consumer | 30/30, nextest `31ba1e3d-36b4-4ec8-a153-65dd51e29fd7` |
| Final Bevy feature-unified motion consumer | 31/31, `cdb0176e-8522-4bbf-9814-14b2f08b574a` |
| Focused reviewed collision controls before error-layout cleanup | 8/8, `13488f02-cc69-4341-bfc6-4aa04f19ebda`; all remain in the final cohorts |
| Fixture workspace correctness Clippy, all targets/features | Passes; the fixture adds no warnings, existing feature-specific Scene dead-code warnings remain |
| Physics fixture without optional features | Locked `cargo check -p motion-physics --no-default-features` passes with its existing Reflect dead-code warning; authoring dependencies remain optional |
| Formatting and documents | Fixture `cargo fmt --all --check`, `git diff --check` and 1,232 local Markdown targets pass |
| Independent adversarial review | No confirmed P1/P2 in native ownership, input validation, geometry capture or close |
| Independent testing review | Four concrete P2 evidence gaps corrected; limited re-review confirms no remaining P1/P2 or testing gaps in the unit's stated scope |

Reproduction commands:

```text
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --test-threads 1
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --features unified-multi-threaded --test-threads 1
cargo +1.95.0 clippy --locked --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target --workspace --all-targets --all-features -- -D clippy::correctness
```

Raw logs stay in untracked `target/motion-collision-*.log`. Reviewers did not run Cargo. No GPU,
Editor widget, physical audio device or other-platform qualification is added by these runs.

# Result

The exact derived `CollisionDefinition` Schema/codec stores only center, bounded half width/height
and fixed/dynamic intent. Its public App provider is admitted through the existing registry owner.
The independent consumer uses actual Scene/Prefab materialization and ordinary authoring patches,
Undo/Redo and disk Save/Reopen. Each successful path checks the actual native collider dimensions,
numeric debug geometry and exact accepted source revision rather than only ECS values or counts.
Missing native binding refuses at registry freeze; the returned original builder can be repaired
with the exact derive codec, frozen, materialized and derived successfully.

The private Rapier owner receives complete current Scene receipts and validates the whole batch
before native mutation. Body, collider and debug roles retain the existing source, occurrence,
structured Prefab provenance, stable component/property selectors and caller-accepted revision.
Two projections have different legal widths; tests resolve actual receipt members, compare each
generated origin to its own `SceneEntitySource`, and check the corresponding native geometry.
No provenance is reconstructed from generated entity-ID spelling.

Dynamic boxes actually fall onto a generated floor during 120 managed fixed steps. Tests observe
the resulting resting position, native contact and all numeric corners from the actual collider.
Source edits retire the replaced activation's original body/collider handles. An unchanged other
occurrence retains its exact pair, position and nonzero velocity. Duplicate, component removal,
entity removal and occurrence unload compare the complete current receipt-derived Entity set to
both native records and debug shapes. Unloaded pairs are individually absent; remaining pairs
are unchanged. Retained immutable debug captures survive edit/removal/close while old native
handles stay retired. Managed Runtime close empties the concrete native owner and refuses step.

The mixed-input control changes an earlier receipt legally and makes a later center invalid.
Refusal preserves every original pair, actual collider extent, body position/velocity/sleep state
and snapshot. Diagnostics identify the later source, input index, occurrence, entity, component,
property and revision. A valid receipt with missing `SceneEntitySource` separately refuses.
After restoring that context, retrying the same receipts rebuilds only the changed live Entity's
pair; all unaffected native state remains exact. Stale, duplicate and foreign receipts also refuse.
Invalid source patches preserve document/revision, then ordinary synchronization proves no hidden
dirty transition replaced the original pair or dynamics.

## Review and comparison

The first targeted compile exposed two consumer assumptions that Rapier translation was borrowed;
0.33.0 returns it by value. Removing those dereferences repaired only the consumer. The first
executed cohort then found that missing binding refused at the earlier registry freeze. The test
now observes that actual typed boundary and repairs the original candidate; engine admission was
not weakened. These are integration failures, not claimed production RED/GREEN fixes.

Testing review rejected count-only or ECS-only evidence for Prefab attribution, history/Reopen,
unload and complete-batch refusal. The final controls above address those four findings and the
same-Entity, source-mismatch, nonzero-velocity and rejected-patch synchronization gaps. Limited
re-review closed all findings. Native ownership review found no confirmed P1/P2. Every short-lived
research, implementation and review agent was closed before delivery.

Pinned reference inspection informed the choice without promising equivalence:

- Bevy `0.20.0-dev`, `85503941e488152711a4695bbe248e9ed9f453fc`: `ResolvedSceneRoot::apply`
  uses one reference map per spawn; `apply_scene_replaces_and_orphans_children` retains former
  child entities. RequiredComponents/add hooks alone cannot prove ongoing cleanup. Nara therefore
  uses its own current Scene receipts plus the concrete output owner's retained inventory.
- Godot `4.8.0-beta`, `81ab961f91066a63bcbf3ec340962bb1a1ada558`: `CollisionShape2D`
  attaches/removes shape owners and distinguishes resource changes from transform changes.
  Resources are shared unless made scene-local. Nara tests both separate occurrences and an
  explicit instance override instead of assuming native bodies imply isolated source values.
- Published Rapier `0.33.0`: native pair removal is explicitly owned; `remove_body` cascades its
  owned collider. Collider shape/pose capture is stronger evidence than authored-only drawing,
  but contact and physical synchronization still need a real step. The local package metadata
  names `7e72d13716814717e65e613c20fb6568e61ad282` with `dirty: true`; checked files match the
  cached release archive, so this evidence pins the package rather than a clean Git snapshot.

# Follow-up

The existing public Scene synchronization replaces the selected occurrence's activation. This
consumer deliberately derives fresh output for it; it does not infer native continuity from equal
source IDs. Only unchanged live entities retain solver state. An in-place definition change also
rebuilds its pair and resets dynamics. Center bounds are native admission, while scalar dimension
bounds also participate in Schema validation. Caller-provided revision explains input provenance;
it is not a new authorization token or general source-version resolver.

This completes the finite R10 collision-definition workflow under ADR0095 and the existing
ADR0117 separation of persistent configuration and runtime reconciliation. No production Physics
backend, domain cook, generic baker/provider, affine hierarchy integration, Editor widgets or
native rollback is selected. There is no measured cook/startup pressure requiring that alternative.
Broader R10 input/repeatability/affine work, R11's editable Nara-drawn Inspector and R12's exact
capture/rebuild/restore integration remain open in the overarching foundation Goal. Keep that Goal
active; this consumer is not the complete product rebaseline.

# Sources

- [Owning unit](../../../../plans/2026-11-04-authored-collision-derivation-plan.md)
- [Plugin-owned domain contract](../../../../architecture/adr/0095-plugin-owned-specialized-domains-and-project-configuration.md)
- [Persistent/native reconciliation](../../../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md)
- [Independent consumer](../../../../../tests/fixtures/motion-execution/README.md)
