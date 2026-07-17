---
type: "Decision"
title: "Treat ignore-deferred scheduling as an explicit compatibility opt-out"
description: "Keep Bevy's typed scheduler surface while excluding explicit ignore-deferred relations from Nara's public semantic-anchor compatibility guarantee."
timestamp: 2026-07-17T00:39:47Z
record_id: "5cbaef069ee343789b4414771fa516e7"
producer_id: "codex-architecture-review"
run_id: "session-2026-07-16-architecture-review"
---

# Decision

Nara keeps Bevy's typed scheduling substrate and does not add a scheduler wrapper solely to police
`before_ignore_deferred`, `after_ignore_deferred`, or equivalent chaining. These relations remain
available to trusted advanced engine/plugin code, but using one is an explicit opt-out from Nara's
public semantic-anchor deferred-visibility guarantee. Such code may not claim conformance merely
because the owning schedule seals or runs.

Before `App::seal` succeeds, Nara still validates the owning schedule and required set graph,
requires automatic deferred insertion, and reasserts final deferred application. The first-playable
public inventory remains one schedule participation point, `CoreStage::FixedUpdate`, plus three
joinable membership/ordering phases: `FixedUpdateSet::Simulate`,
`GameplayCommandSet::Consume`, and `GameplayCommandSet::Capture`. This is not a total-order promise.

# Context

Bevy provides explicit ignore-deferred ordering escape hatches, but its public schedule
introspection does not expose a complete reliable inventory that lets Nara reject every such edge
at seal time. The focused Bevy finding correctly established the deferred-policy bypass and
initially recommended rejection. Subsequent disposition found that enforcing that recommendation
would require a Nara-owned scheduler wrapper or hiding useful Bevy scheduling access.

That extra authority would enlarge ordinary and advanced plugin concepts before a production
extension has demonstrated a need for it. It would also reduce the Bevy-like freedom Nara intends
to preserve without actually proving a stronger global ordering model. The compatibility claim can
instead be made falsifiable at the semantic boundary: standard anchor fixtures must observe the
documented deferred result, while an explicit ignore-deferred fixture is outside the claim and must
fail the conformance oracle.

# Alternatives

## Reject every ignore-deferred relation through a Nara scheduler wrapper

Rejected for the first playable. It offers a stronger closed surface, but duplicates Bevy
scheduler concepts, cannot remain a thin adapter if raw schedule access is also retained, and adds
user/plugin-author complexity without current product evidence.

## Hide raw scheduling and expose only Nara-owned phase methods

Rejected. It would make the policy easy to enforce but would unnecessarily reduce advanced plugin
freedom and make custom domain schedules harder than in Bevy.

## Preserve typed scheduling and define explicit compatibility opt-out

Chosen. It keeps the ordinary path small, preserves advanced access, and limits Nara's guarantee to
behavior its external fixtures can actually prove.

# Consequences

- Rustdoc and ADR text must distinguish schedule participation from set membership/ordering.
- A sealed App with an explicit ignore-deferred relation is not thereby public-anchor conformant.
- Public conformance fixtures cover deferred visibility, final application, skip/fault/cleanup
  behavior, absent/cross-schedule targets, and unordered peers.
- Nara does not promise to detect every advanced opt-out through schedule introspection.
- Reconsider a wrapper only after a production extension requires untrusted multi-party schedule
  composition, a real correctness incident shows the opt-out boundary is insufficient, or Bevy
  exposes complete stable introspection that makes enforcement materially simpler.

# Validation

- The renamed-dependency extension registers work in `CoreStage::FixedUpdate`, joins/orders only
  against the three documented set anchors, and observes the documented deferred result.
- Disabling automatic deferred insertion or invalidating the required set graph prevents sealing;
  final deferred application remains visible at the documented completion edge.
- An explicit ignore-deferred fixture does not pass the public compatibility oracle.
- Source review finds no Nara scheduler wrapper, dynamic string registry, or total-order claim added
  solely for this policy.

# Risks and Mitigations

- **Risk:** Advanced code assumes sealing implies anchor conformance. **Mitigation:** Rustdoc names
  the opt-out and the conformance fixture tests behavior rather than graph construction alone.
- **Risk:** A future Bevy change alters deferred semantics. **Mitigation:** The renamed-dependency
  fixture and per-anchor lifecycle tests run before expanding third-party scheduling use.
- **Risk:** Multiple untrusted plugins make voluntary opt-out labeling insufficient. **Mitigation:**
  Reopen the wrapper decision only with that concrete production tracer and compare enforcement,
  capability-scoped schedule access, and a closed scheduler surface.

# Citations

- `docs/architecture/adr/0003-own-app-plugin-and-schedule-lifecycle.md`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`, RGF-U28
- `docs/knowledge/engineering/subagents/2026-07/2026-07-16T164357Z-bevy-lifecycle-observer-and-deferred-schedule-verification-027c48803a9442d8930a3d0f558bafd3.md`
- `repo-ref/bevy/crates/bevy_ecs/src/schedule/config.rs`
