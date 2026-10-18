# Foundation Contract Audit and Hardening

Date: 2026-10-18

Baseline: `e9c147ffe2e1d85c4493a44940d86af2c6a1dc17`.

## Objective

Identify foundational contracts that would spread redesign cost into future engine domains,
separate them from missing domain/game features, and repair confirmed convergent defects.
The prior product-oriented review is background evidence, not an implementation backlog.

## Scope and sequence

1. Review execution/scheduling, identity/lifetime, persistent value/reference contracts,
   assets/tasks/filesystem, and render/input/presentation ownership against the local Bevy/Godot
   snapshots. Three read-only review lanes return concrete extension counterexamples, affected
   consumers, reasonable tradeoffs, and dependency order.
2. Reproduce and repair the loss of admitted asset-resolution context during advanced scene
   replacement. Preserve the existing source retention owner; migrate all affected consumers.
3. Reproduce the suspected collision between target-private glyph atlases in the shared GPU cache.
   If confirmed, give cache entries an identity that includes their actual atlas owner and preserve
   retirement/device behavior.
4. Consolidate a foundation-only review with prerequisite order, decisions to retain/revisit,
   downstream migration scope, and focused validation. Do not implement deferred Tilemap/gameplay
   features or speculative general frameworks.
5. Review actual diffs, run appropriate serial verification, and commit only intended changes.

## Verification

Use regression-first checks for confirmed defects, then focused nextest suites and affected
public consumers. Reuse `target` with `CARGO_BUILD_JOBS=1`; never run Cargo concurrently here.
Check the relevant native Editor qualification trajectory if the rendering change reaches
native target/device/presentation behavior. Record actual results and environment limits.

## Safety and authority

Preserve the pre-existing untracked `myeasylog.log` and `vendor/`. Reviewers are read-only.
No remote publication is part of this task. New durable design choices must be reconciled with
Accepted ADRs before implementation; a report does not silently accept a proposed contract.

## Completion

Confirmed fixes have meaningful passing regressions and affected-consumer evidence. The review
states which foundations are sound, which changes precede new domains, and which concerns require
real consumer pressure rather than premature implementation. No claim of a debt-free architecture
or completed domain functionality is made.
