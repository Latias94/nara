---
type: "Verification Evidence"
title: "Chat-informed architecture review probes and design constraints"
description: "Three public-interface probes distinguish a reproduced control defect from current Identity and Theme contract debt."
timestamp: 2026-10-23T10:57:19Z
record_id: "cc6bbbacdaac48eea3745bc7d36f8d9a"
resource: "docs/audits/2026-10-23-chat-informed-engine-architecture-review.md"
producer_id: "codex-chat-architecture"
run_id: "chat-architecture-20261023"
related_plan: "docs/plans/2026-10-23-chat-informed-foundation-redesign-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "f91466b"
---

# Verification

Source baseline `f91466b0626543688bd98923dbd04fb842028120`, Windows, Rust 1.95.0.
The user supplied a downloaded conversation after an unauthenticated proxy request returned 403.
The audit records its provenance/hash without storing the private conversation. Four independent
read-only Codex lanes reviewed runtime/tasks, content/schema, render/UI, and public extension/tool
interfaces. Integration challenged their claims against owning ADRs and ran three bounded probes.

All Cargo was serial, `CARGO_BUILD_JOBS=1`, `--locked`, with the existing root target. There were
no production Rust, Accepted ADR, dependency or lockfile changes. Temporary probe files were
removed after exact source was retained below. This is investigation evidence, not refactor or
product qualification. `vendor/` was neither read nor changed; both reference worktrees remained
clean at their pinned commits.

# Result

| Probe | Actual execution | Interpretation |
| --- | --- | --- |
| App Stop history | nextest `53031c35-16b6-4a3c-ad69-36811794be03`: one test, one failure at the final observable-ticket assertion | Reproduced defect: duplicate Stop is Accepted with a ticket whose status is None. The participant is physically retired before the assertion. |
| Scene lifetime claims | nextest `af2c854d-9227-4c61-b6a6-c38a0df4f486`: witness passed | One active entity consumes claims 2/4/6 through three replacements; the fourth is refused. This obeys ADR 0058 and exposes contract debt, not a fixed bug. |
| Theme identity | Same combined run: witness passed; two tests total, two passed | Revision 1 permits old red retained paint and new green current paint in one frame. This proves the public compiler/cache behavior, not the complete native Inspector/Preview chain. |

The first combined Scene/UI compile failed because the temporary probe used nonexistent
`UiLayer::Content`. It was corrected to `UiLayer::Base`, then the two witnesses actually executed.
That setup error is not counted as an engine failure. Existing unused-code warnings in Reflect
and Scene remain. No native, performance or cross-platform acceptance was performed.

# Evidence

Commands, run separately and serially while the corresponding temporary test existed:

```text
cargo +1.95.0 nextest run -p nara_app --test architecture_review_probe --locked --test-threads=1
cargo +1.95.0 nextest run -p nara_scene -p nara_ui_kernel --test architecture_review_probe --locked --test-threads=1
```

Exact probe hashes (UTF-8 bytes before removal):

- App: `16856872a606e4e41218400dcca9d59ddc941da04be198d5a70f6ee4545d660d`.
- Scene: `23884a87ea431fa33ab9e407184065f8cea74ceacd4330b8742c2f2d50230236`.
- UI kernel: `6481001fa152f72410af1b37e8fd0201cea6750aaeb5b646ee3dc706fd8db873`.

The four review lanes returned nine findings, with final classification and alternatives in the
audit. Important challenge corrections:

- Persistence currently exposes clear-latest acknowledgment; Accepted ADRs do not already promise
  result-specific acknowledgment for every Persistence action. Exact acknowledgment is Proposed.
- Exact arbitrary historical membership, bounded memory and unbounded identity churn require a
  real lifetime/knowledge tradeoff. Do not implement an assumed high-watermark compaction.
  Disposable preview epoch rollover is the first bounded alternative, subject to lifecycle proof.
- Theme's authoring path permits palette writes without a revision bump; the cache witness needs
  one retained compiler and a later root failure. No documented caller-bump promise was found.
- SafeSummary is checked static source text, not a universal redactor. A new wire projection must
  allowlist fields, use its own smaller budget and provide static fallbacks without arbitrary
  Error/Debug strings.
- No new counterexample justifies ECS replacement, universal execution/automation kernels, a new
  scripting ABI or public render graph. Asset clone cost and glyph preparation budgets remain
  explicitly measured/requirement-gated investigations.

A fifth independent Codex reviewer challenged the integrated audit/plan against the targeted
source and Accepted ADRs, especially the lifetime-knowledge tradeoff, no-UI/presented authority,
clear-current versus exact acknowledgment, and safe wire projection. It found no blocking factual
or unsafe design prescription. This does not approve unimplemented source changes. All five agents
were closed after their results were integrated. Local links, embedded probe hashes, owned probe
removal and whitespace checks pass; memory validation passes with pre-existing historical warnings.

# Follow-up

The linked plan is Proposed. No production unit was started, no new Goal/budget created, and no
PR or push performed. Implement only after selecting the affected contract and its scope.
The materializer witness does not prove preview epoch rollover; that needs actual Editor field
edits, stale locator/pick/async-result rejection, predecessor presentation and physical custody.

To reproduce a probe, restore its exact source below to its original temporary test path at the
recorded baseline, run the corresponding command, then remove that owned temporary file. The
Stop probe is deliberately red; the other witnesses deliberately assert current undesirable
behavior. Do not add these witnesses as permanent tests that enforce the defect.

# Citations

- [Audit and pinned-reference comparison](../../../../audits/2026-10-23-chat-informed-engine-architecture-review.md).
- [Proposed owner redesign plan](../../../../plans/2026-10-23-chat-informed-foundation-redesign-plan.md).

## Exact Probe Sources

### `crates/nara_app/tests/architecture_review_probe.rs`

```rust
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use nara_app::{
    App, RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
};

struct RetryUntilReleased(Arc<AtomicBool>);

impl RuntimeCloseParticipant for RetryUntilReleased {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if self.0.load(Ordering::SeqCst) {
            Ok(RuntimeCloseProgress::Complete)
        } else {
            Err(RuntimeCloseParticipantError::retryable("nara.test.review-retry"))
        }
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        unreachable!("begin either completes or requests a retry")
    }
}

#[test]
fn accepted_idempotent_stop_keeps_an_observable_result_after_many_retries() {
    let released = Arc::new(AtomicBool::new(false));
    let mut ledger = RuntimeObligationLedger::new();
    ledger.register(
        RuntimeCloseParticipantId::new("nara.test.review-history"),
        RetryUntilReleased(Arc::clone(&released)),
    ).unwrap();
    let mut runtime = RuntimeInstance::start_with_options(
        App::new(), ledger, RuntimeClosePolicy::new(Duration::ZERO),
    ).unwrap();
    let RuntimeControlRequestResult::Accepted(original) = runtime.request_control(RuntimeControl::Stop)
    else { panic!("Stop must be admitted"); };
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    for _ in 0..40 {
        assert!(matches!(runtime.request_control(RuntimeControl::RetryClose), RuntimeControlRequestResult::Accepted(_)));
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    }
    assert!(matches!(runtime.request_control(RuntimeControl::RetryClose), RuntimeControlRequestResult::Accepted(_)));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    let repeated = runtime.request_control(RuntimeControl::Stop);
    let observed = runtime.control_status(original);

    // Complete physical retirement before asserting the disputed observation contract.
    released.store(true, Ordering::SeqCst);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    runtime.finish().unwrap();

    assert_eq!(repeated, RuntimeControlRequestResult::Accepted(original));
    assert!(observed.is_some(), "an Accepted Stop returned a ticket with no observable result");
}
```

### `crates/nara_scene/tests/architecture_review_probe.rs`

```rust
use nara_core::ItemLimit;
use nara_ecs::World;
use nara_identity::{WorldIdentityDomain, WorldIdentityDomainSettings};
use nara_reflect::ComponentRegistry;
use nara_scene::{SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord};

#[test]
fn repeated_single_entity_projection_exhausts_lifetime_claims() {
    let mut world = World::new();
    let settings = WorldIdentityDomainSettings::new(
        ItemLimit::new(6).unwrap(), ItemLimit::new(4).unwrap(), ItemLimit::new(4).unwrap(),
    );
    let domain = WorldIdentityDomain::new(&world, settings).unwrap();
    world.insert_resource(domain);
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let document = || SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("only").unwrap()),
    ]);
    let mut session = SceneAuthoringSession::new(document());
    for publication in 1..=3 {
        session.replace_document(document()).unwrap();
        let report = session.sync_world(&mut world, &registry).unwrap();
        assert!(report.synced, "{:#?}", report.diagnostics);
        let stats = world.resource::<WorldIdentityDomain>().stats();
        assert_eq!(stats.active_scene_instances, 1);
        assert_eq!(stats.active_scene_entities, 1);
        assert_eq!(stats.lifetime_claims, publication * 2);
    }
    session.replace_document(document()).unwrap();
    let fourth = session.sync_world(&mut world, &registry).unwrap();
    assert!(!fourth.synced);
    assert!(fourth.diagnostics.has_errors());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats().active_scene_entities, 1);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats().lifetime_claims, 6);
}
```

### `crates/nara_ui_kernel/tests/architecture_review_probe.rs`

```rust
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiColorRole, UiCompiler, UiEnvironment, UiLayer,
    UiLocalKey, UiNode, UiPaintPrimitive, UiResolvedBrush, UiRoot, UiRootKey, UiRootStatus,
    UiStyle, UiSurfaceId, UiTheme, UiThemePalette, UiVisual,
};

fn themed_root(name: &str, duplicate: bool) -> UiRoot {
    let node = || UiNode::new(UiStyle::absolute(0.0, 0.0, 40.0, 40.0))
        .with_key(UiLocalKey::runtime("review", "node").unwrap())
        .with_visual(UiVisual::role(UiColorRole::Accent));
    let root = UiRoot::new(UiRootKey::runtime("review", name).unwrap(), UiLayer::Base, UiStyle::fill());
    if duplicate { root.with_children([node(), node()]) } else { root.with_child(node()) }
}

#[test]
fn equal_theme_revision_allows_red_retained_and_green_current_visuals() {
    let red = UiColor::rgba(1.0, 0.0, 0.0, 1.0);
    let green = UiColor::rgba(0.0, 1.0, 0.0, 1.0);
    let theme = |accent| UiTheme::new(1, UiThemePalette { accent, ..Default::default() }).unwrap();
    let environment = UiEnvironment::new(UiSurfaceId::new(9).unwrap(), 320.0, 180.0, 1.0, 1, 1, 1, 1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let initial = UiAuthoringSnapshotBuilder::new(1).with_root(themed_root("a", false)).build().unwrap();
    compiler.compile(&initial, environment, theme(red)).unwrap();
    let changed = UiAuthoringSnapshotBuilder::new(2)
        .with_root(themed_root("a", true)).with_root(themed_root("b", false)).build().unwrap();
    let mixed = compiler.compile(&changed, environment, theme(green)).unwrap();
    assert_eq!(mixed.root_outcomes()[0].status(), UiRootStatus::RetainedVisual);
    assert_eq!(mixed.root_outcomes()[1].status(), UiRootStatus::Published);
    let colors: Vec<_> = mixed.paint().iter().filter_map(|primitive| match primitive {
        UiPaintPrimitive::Rect { brush: Some(UiResolvedBrush::Solid(color)), .. } => Some(*color),
        _ => None,
    }).collect();
    assert!(colors.contains(&red), "the old theme unexpectedly lost its cache eligibility");
    assert!(colors.contains(&green), "the new theme did not publish in the valid root");
}
```
