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
#[ignore = "manual bounded Scene epoch comparison with the frozen incumbent and full-owner control"]
fn candidate_epoch_against_full_owner_epoch_control() {
    let mut all_exact = true;
    for population in [64, 1_024] {
        let (registry, mut source) = fixture(population);
        let original = source.document().clone();
        // Reuse the same source incarnation and identical cyclic edits for both variants.
        for epochs in [false, true] {
            let variant = if epochs {
                "full-owner-epoch-control"
            } else {
                "scene-projection-epoch"
            };
            let cap = Some((population + 1) * if epochs { EPOCH_PUBLICATIONS } else { 1 });
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
                        && next.snapshot.domain_id != previous.snapshot.domain_id
                        && matches!(stale, EntityLookup::WrongDomain { .. })
                };
                let accepted = edited
                    && next.exact
                    && separated
                    && next.revision != previous.revision
                    && next.stats.lifetime_claims == population + 1
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
