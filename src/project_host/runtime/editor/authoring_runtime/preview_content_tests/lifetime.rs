//! Bounded, manual Transform-only attribution; not Image custody or native qualification.
//! Full-owner epochs are reconstruction-cost controls, not a continuity-safe production design.

use super::*;

use std::time::Instant;

use nara_identity::{EntityLookup, IdentityDomainStats, WorldIdentityDomain, resolve_in_world};
use nara_reflect::{ComponentFieldId, ComponentRegistry, ComponentTypeId, ComponentValue};
use nara_scene::{
    SceneComponentRecord, SceneEntityId, SceneEntityRecord, SceneEntitySource, ScenePatchDocument,
    ScenePatchOperation,
};
use nara_transform::Transform2d;
use serde_json::{Value, json};

const WARMUPS: usize = 2;
const SAMPLES: usize = 16;
// Includes the initial publication; rotate before a ninth publication, not on every edit.
const EPOCH_PUBLICATIONS: usize = 8;

struct RetiringOwner(EditorAuthoringRuntime);

impl RetiringOwner {
    fn new(limit: Option<usize>) -> Self {
        let (runtime, _, _) = match limit {
            None => authoring_runtime(),
            Some(_) => authoring_runtime_with_claim_limit(limit),
        };
        let mut owner = Self(runtime);
        owner.0.set_preview_extent(Extent2d::new(320, 240));
        owner
    }

    fn finish(self) -> u128 {
        let started = Instant::now();
        drop(self);
        started.elapsed().as_nanos()
    }
}

impl Drop for RetiringOwner {
    fn drop(&mut self) {
        retire(&mut self.0);
    }
}

fn fixture(population: usize) -> (ComponentRegistry, SceneAuthoringSession) {
    let registry = ComponentRegistry::from_snapshot(
        crate::project_host::authoring::first_party_authoring_preview_registry(
            crate::project_host::composition::built_in_schema_providers(),
        )
        .unwrap(),
    );
    let component = ComponentTypeId::new("nara.transform.Transform2d");
    let schema = registry.schema(&component).unwrap();
    let value = SceneComponentRecord::new(
        schema.version,
        schema.authoring_initial_value().unwrap().clone(),
    );
    let document = SceneDocument::from_ordered_roots((0..population).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("entity-{index}")).unwrap())
            .with_component(component.clone(), value.clone())
    }));
    (registry, SceneAuthoringSession::new(document))
}

fn edit_rotation(
    source: &mut SceneAuthoringSession,
    registry: &ComponentRegistry,
    angle: f32,
) -> bool {
    let before = source.revision();
    let component = ComponentTypeId::new("nara.transform.Transform2d");
    let report = source.apply_patch(
        &ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: SceneEntityId::new("entity-0").unwrap(),
            component_version: registry.schema(&component).unwrap().version,
            component,
            field: ComponentFieldId::new("rotation"),
            value: ComponentValue::f64(f64::from(angle)).unwrap(),
        }]),
        registry,
    );
    report.applied
        && !report.diagnostics.has_errors()
        && source.revision().source_id() == before.source_id()
        && Some(source.revision().generation()) == before.generation().checked_add(1)
}

fn exact_transforms(owner: &RetiringOwner, population: usize, angle: f32) -> bool {
    let world = owner.0.runtime.world();
    let mut count = 0;
    let mut edited_count = 0;
    let exact = world.iter_entities().all(|entity| {
        let Some(source) = entity.get::<SceneEntitySource>() else {
            return true;
        };
        count += 1;
        let edited = source.entity_id == SceneEntityId::new("entity-0").unwrap();
        edited_count += usize::from(edited);
        entity.get::<Transform2d>()
            == Some(&Transform2d {
                rotation: if edited { angle } else { 0.0 },
                ..Transform2d::IDENTITY
            })
    });
    exact && count == population && edited_count == 1
}

fn stats_json(stats: IdentityDomainStats) -> Value {
    json!({
        "lifetime_claims": stats.lifetime_claims,
        "claimed_scene_instances": stats.claimed_scene_instances,
        "active_scene_instances": stats.active_scene_instances,
        "active_scene_entities": stats.active_scene_entities,
        "active_persistent_entities": stats.active_persistent_entities,
        "persistent_logical_records": stats.persistent_logical_records,
        "unloaded_persistent_entities": stats.unloaded_persistent_entities,
        "deleted_persistent_entities": stats.deleted_persistent_entities,
        "registered_entities": stats.registered_entities,
        "recent_tombstones": stats.recent_tombstones,
    })
}

struct Observed {
    snapshot: WorldIdentitySnapshot,
    stats: IdentityDomainStats,
    render_source: nara_render::RenderFrameSourceGeneration,
    revision: SceneAuthoringRevision,
    exact: bool,
}

impl Observed {
    fn capture(
        owner: &RetiringOwner,
        candidate: EditorPreviewCandidate,
        source: &SceneAuthoringSession,
        population: usize,
        angle: f32,
    ) -> Self {
        let candidate_exact = candidate.document() == EditorDocumentId::from_raw(1)
            && candidate.revision() == source.revision()
            && candidate.preview_omissions().is_empty();
        let (_, revision, _, packet, _, _) = candidate.into_parts();
        let world = owner.0.runtime.world();
        let snapshot = WorldIdentitySnapshot::capture_default(world).unwrap();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let exact = candidate_exact
            && exact_transforms(owner, population, angle)
            && snapshot.domain_id.is_some()
            && snapshot.total_entity_count == population + 1
            && snapshot.locators.len() == population
            && snapshot.omitted_locator_count == 0
            && stats.active_scene_instances == 1
            && stats.active_scene_entities == population
            && stats.registered_entities == population;
        Self {
            snapshot,
            stats,
            render_source: packet.source_generation(),
            revision,
            exact,
        }
    }

    fn emit(
        &self,
        variant: &str,
        population: usize,
        phase: &str,
        index: usize,
        crossed_epoch: bool,
        elapsed_ns: [u128; 3],
        acceptance: Value,
    ) {
        println!(
            "{}",
            json!({
                "variant": variant, "population": population, "phase": phase,
                "sample": (phase == "sample").then_some(index.saturating_sub(WARMUPS)),
                "warmup": (phase == "warmup").then_some(index), "crossed_epoch": crossed_epoch,
                "epoch_publication_cadence": (variant == "full-owner-epoch-control").then_some(EPOCH_PUBLICATIONS),
                "setup_elapsed_ns": elapsed_ns[0], "publication_elapsed_ns": elapsed_ns[1],
                "retirement_elapsed_ns": elapsed_ns[2], "identity_domain_stats": stats_json(self.stats),
                "domain": format!("{:?}", self.snapshot.domain_id),
                "render_source": format!("{:?}", self.render_source),
                "source_incarnation": format!("{:?}", self.revision.source_id()),
                "revision": self.revision.generation(), "live_entities": self.snapshot.total_entity_count,
                "acceptance": acceptance, "pending_tasks_images_native_qualified": false,
            })
        );
    }
}

#[test]
#[ignore = "manual bounded full-owner reconstruction-cost control; not a production A winner"]
fn retained_owner_against_full_owner_epoch_control() {
    let mut all_exact = true;
    for population in [64, 1_024] {
        let (registry, mut source) = fixture(population);
        let original = source.document().clone();
        // Reuse the same source incarnation and identical cyclic edits for both variants.
        for epochs in [false, true] {
            let variant = if epochs {
                "full-owner-epoch-control"
            } else {
                "retained-default"
            };
            let cap = epochs.then_some((population + 1) * EPOCH_PUBLICATIONS);
            let started = Instant::now();
            let mut owner = RetiringOwner::new(cap);
            let setup_ns = started.elapsed().as_nanos();
            let started = Instant::now();
            let candidate = preview(&mut owner.0, &source);
            let publication_ns = started.elapsed().as_nanos();
            let mut previous = Observed::capture(&owner, candidate, &source, population, 0.0);
            previous.emit(
                variant,
                population,
                "initial",
                0,
                false,
                [setup_ns, publication_ns, 0],
                json!({"exact": previous.exact, "claim_cap": cap}),
            );
            all_exact &= previous.exact;
            let mut publications = 1;
            let mut previous_angle = 0.0;
            for index in 0..WARMUPS + SAMPLES {
                let angle = if index % 2 == 0 { 0.25 } else { 0.0 };
                let edited = edit_rotation(&mut source, &registry, angle);
                let crossed_epoch = epochs && publications == EPOCH_PUBLICATIONS;
                let started = Instant::now();
                let mut successor = crossed_epoch.then(|| RetiringOwner::new(cap));
                let setup_ns = if crossed_epoch {
                    started.elapsed().as_nanos()
                } else {
                    0
                };
                let active = successor.as_mut().unwrap_or(&mut owner);
                let started = Instant::now();
                let candidate = preview(&mut active.0, &source);
                let publication_ns = started.elapsed().as_nanos();
                let next = Observed::capture(active, candidate, &source, population, angle);
                let stale =
                    resolve_in_world(active.0.runtime.world(), &previous.snapshot.locators[0]);
                let separated = if crossed_epoch {
                    next.render_source != previous.render_source
                        && next.snapshot.domain_id != previous.snapshot.domain_id
                        && matches!(stale, EntityLookup::WrongDomain { .. })
                        && exact_transforms(&owner, population, previous_angle)
                        && matches!(
                            resolve_in_world(
                                owner.0.runtime.world(),
                                &previous.snapshot.locators[0]
                            ),
                            EntityLookup::Resolved(_)
                        )
                } else {
                    next.render_source == previous.render_source
                        && next.snapshot.domain_id == previous.snapshot.domain_id
                };
                let accepted = edited
                    && next.exact
                    && separated
                    && next.revision != previous.revision
                    && !matches!(stale, EntityLookup::Resolved(_));
                // The successor's first candidate is checked while its predecessor is still alive.
                let retirement_ns = match successor {
                    Some(successor) if accepted => {
                        publications = 0;
                        std::mem::replace(&mut owner, successor).finish()
                    }
                    Some(rejected) => rejected.finish(),
                    None => 0,
                };
                next.emit(
                    variant,
                    population,
                    if index < WARMUPS { "warmup" } else { "sample" },
                    index,
                    crossed_epoch,
                    [setup_ns, publication_ns, retirement_ns],
                    json!({"exact": accepted, "revision_increment": edited, "owner_axes": separated,
                        "old_locator": format!("{stale:?}"), "claim_cap": cap}),
                );
                all_exact &= accepted;
                if !accepted {
                    break;
                }
                publications += 1;
                previous = next;
                previous_angle = angle;
            }
            previous.stats = owner
                .0
                .runtime
                .world()
                .resource::<WorldIdentityDomain>()
                .stats();
            previous.snapshot =
                WorldIdentitySnapshot::capture_default(owner.0.runtime.world()).unwrap();
            let retirement_ns = owner.finish();
            previous.emit(
                variant,
                population,
                "retirement",
                0,
                false,
                [0, 0, retirement_ns],
                json!({"exact": all_exact}),
            );
            all_exact &= source.document() == &original;
        }
    }
    assert!(all_exact, "inspect the bounded JSON acceptance records");
}

#[test]
#[ignore = "manual tiny-cap witness; saturation is observed, not a permanent production invariant"]
fn tiny_claim_cap_keeps_last_good_and_fresh_full_owner_can_progress() {
    let (registry, mut source) = fixture(4);
    let started = Instant::now();
    let mut owner = RetiringOwner::new(Some(15));
    let setup_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    let candidate = preview(&mut owner.0, &source);
    let publication_ns = started.elapsed().as_nanos();
    let mut last = Observed::capture(&owner, candidate, &source, 4, 0.0);
    last.emit(
        "tiny-cap-incumbent",
        4,
        "initial",
        0,
        false,
        [setup_ns, publication_ns, 0],
        json!({"exact": last.exact, "claim_cap": 15}),
    );
    let mut exact = last.exact;
    let mut saturated = false;
    let mut last_angle = 0.0;
    // Four total incumbent attempts suffice for today's five-claims-per-publication behavior.
    // If production later reuses claims, record non-saturation instead of requiring exhaustion.
    for attempt in 1..=3 {
        let angle = if attempt % 2 == 1 { 0.25 } else { 0.0 };
        exact &= edit_rotation(&mut source, &registry, angle);
        let started = Instant::now();
        let report = owner.0.drive_preview(Some(EditorPreviewSource {
            document: EditorDocumentId::from_raw(1),
            revision: source.revision(),
            projection: Some(EditorPreviewProjection::new(
                source.document().clone(),
                EditorAuthoringPreviewOmissions::empty(
                    EditorDocumentId::from_raw(1),
                    source.revision(),
                ),
            )),
        }));
        let candidate = owner.0.take_preview_candidate();
        let publication_ns = started.elapsed().as_nanos();
        if let Some(candidate) = candidate {
            let next = Observed::capture(&owner, candidate, &source, 4, angle);
            exact &= report.is_none() && next.exact;
            next.emit(
                "tiny-cap-incumbent",
                4,
                "budget-publication",
                attempt,
                false,
                [0, publication_ns, 0],
                json!({"exact": exact, "claim_cap": 15}),
            );
            last = next;
            last_angle = angle;
            continue;
        }
        let before = owner
            .0
            .runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .stats();
        saturated = report.as_ref().is_some_and(|report| report.has_errors())
            && before.lifetime_claims == 15;
        let retained = saturated
            && before == last.stats
            && exact_transforms(&owner, 4, last_angle)
            && WorldIdentitySnapshot::capture_default(owner.0.runtime.world()).unwrap()
                == last.snapshot
            && matches!(
                resolve_in_world(owner.0.runtime.world(), &last.snapshot.locators[0]),
                EntityLookup::Resolved(_)
            )
            && source.revision() != last.revision;
        let started = Instant::now();
        let mut fresh = RetiringOwner::new(Some(15));
        let setup_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let candidate = preview(&mut fresh.0, &source);
        let fresh_publication_ns = started.elapsed().as_nanos();
        let next = Observed::capture(&fresh, candidate, &source, 4, angle);
        let old_ledger_unchanged = owner
            .0
            .runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .stats()
            == before;
        let progressed = next.exact
            && next.snapshot.domain_id != last.snapshot.domain_id
            && next.render_source != last.render_source
            && old_ledger_unchanged
            && matches!(
                resolve_in_world(fresh.0.runtime.world(), &last.snapshot.locators[0]),
                EntityLookup::WrongDomain { .. }
            );
        exact &= retained && progressed;
        let retirement_ns = std::mem::replace(&mut owner, fresh).finish();
        next.emit(
            "tiny-cap-full-owner-control",
            4,
            "budget-rejection-recovery",
            attempt,
            true,
            [setup_ns, fresh_publication_ns, retirement_ns],
            json!({"exact": exact, "last_good_retained": retained, "fresh_progress": progressed,
                "old_ledger_unchanged": old_ledger_unchanged, "old_stats": stats_json(before),
                "rejected_publication_elapsed_ns": publication_ns, "claim_cap": 15}),
        );
        last = next;
        break;
    }
    let retirement_ns = owner.finish();
    last.emit(
        "tiny-cap-summary",
        4,
        "retirement",
        0,
        false,
        [0, 0, retirement_ns],
        json!({"exact": exact, "saturation_observed": saturated, "claim_cap": 15}),
    );
    assert!(
        exact,
        "inspect tiny-cap last-good and fresh-owner acceptance records"
    );
}
