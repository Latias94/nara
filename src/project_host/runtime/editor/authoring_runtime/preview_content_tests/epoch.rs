//! Real authoring Runtime coverage; pending Save and native presentation need Host qualification.

use super::*;

use std::{
    ops::{Deref, DerefMut},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use nara_app::{CoreStage, RuntimeGeneration};
use nara_ecs::bevy_ecs::world::WorldId;
use nara_identity::{
    EntityLookup, WorldEntityToken, WorldIdentityDomain, resolve_in_world, spawn_identity_entity,
};
use nara_reflect::{ComponentFieldId, ComponentRegistry, ComponentTypeId, ComponentValue};
use nara_render::RenderFrameSourceGeneration;
use nara_scene::{
    SceneComponentRecord, SceneEntityId, SceneEntityRecord, SceneEntitySource, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};
use nara_transform::Transform2d;

use crate::project_host::runtime::editor::preview::{
    EditorPreviewContentStamp, EditorPreviewParticipant,
};

const DOCUMENT: EditorDocumentId = EditorDocumentId::from_raw(1);

struct EpochRuntime(EditorAuthoringRuntime);

impl Deref for EpochRuntime {
    type Target = EditorAuthoringRuntime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EpochRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for EpochRuntime {
    fn drop(&mut self) {
        retire(&mut self.0);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RuntimeOwners {
    generation: RuntimeGeneration,
    world: WorldId,
    tasks: *const TaskPools,
    images: *const Assets<ImageAsset>,
    server: *const AssetServer,
    importer: *const ImageImporter,
}

impl RuntimeOwners {
    fn capture(runtime: &EditorAuthoringRuntime) -> Self {
        let world = runtime.runtime.world();
        Self {
            generation: runtime.runtime.generation(),
            world: world.id(),
            tasks: std::ptr::from_ref(runtime.persistence_tasks()),
            images: std::ptr::from_ref(world.resource::<Assets<ImageAsset>>()),
            server: std::ptr::from_ref(world.resource::<AssetServer>()),
            importer: std::ptr::from_ref(world.resource::<ImageImporter>()),
        }
    }
}

fn registry() -> ComponentRegistry {
    ComponentRegistry::from_snapshot(
        crate::project_host::authoring::first_party_authoring_preview_registry(
            crate::project_host::composition::built_in_schema_providers(),
        )
        .unwrap(),
    )
}

fn entity(registry: &ComponentRegistry, id: &str) -> SceneEntityRecord {
    let component = ComponentTypeId::new("nara.transform.Transform2d");
    let schema = registry.schema(&component).unwrap();
    SceneEntityRecord::new(SceneEntityId::new(id).unwrap()).with_component(
        component,
        SceneComponentRecord::new(
            schema.version,
            schema.authoring_initial_value().unwrap().clone(),
        ),
    )
}

fn source(registry: &ComponentRegistry) -> SceneAuthoringSession {
    SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
        entity(registry, "edited"),
        entity(registry, "retained"),
    ]))
}

fn edit_rotation(source: &mut SceneAuthoringSession, registry: &ComponentRegistry, rotation: f32) {
    let component = ComponentTypeId::new("nara.transform.Transform2d");
    let before = source.revision();
    let report = source.apply_patch(
        &ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: SceneEntityId::new("edited").unwrap(),
            component_version: registry.schema(&component).unwrap().version,
            component,
            field: ComponentFieldId::new("rotation"),
            value: ComponentValue::f64(f64::from(rotation)).unwrap(),
        }]),
        registry,
    );
    assert!(report.applied, "{:?}", report.diagnostics);
    assert!(!report.diagnostics.has_errors());
    assert_eq!(source.revision().source_id(), before.source_id());
    assert_eq!(source.revision().generation(), before.generation() + 1);
}

fn assert_rotation(runtime: &EditorAuthoringRuntime, rotation: f32) {
    let world = runtime.runtime.world();
    let edited = SceneEntityId::new("edited").unwrap();
    let mut edited_count = 0;
    for entity in world.iter_entities() {
        let Some(source) = entity.get::<SceneEntitySource>() else {
            continue;
        };
        let is_edited = source.entity_id == edited;
        edited_count += usize::from(is_edited);
        assert_eq!(
            entity.get::<Transform2d>(),
            Some(&Transform2d {
                rotation: if is_edited { rotation } else { 0.0 },
                ..Transform2d::IDENTITY
            })
        );
    }
    assert_eq!(edited_count, 1);
}

struct Epoch {
    snapshot: WorldIdentitySnapshot,
    producer: RenderFrameSourceGeneration,
    content: EditorPreviewContentStamp,
    geometry: u64,
}

fn observe(runtime: &mut EditorAuthoringRuntime, source: &SceneAuthoringSession) -> Epoch {
    observe_with_dependencies(runtime, source, &EditorPreviewDependencies::empty())
}

fn observe_with_dependencies(
    runtime: &mut EditorAuthoringRuntime,
    source: &SceneAuthoringSession,
    dependencies: &EditorPreviewDependencies,
) -> Epoch {
    let candidate = preview_with_dependencies(runtime, source, dependencies);
    assert_eq!(candidate.document(), DOCUMENT);
    assert_eq!(candidate.revision(), source.revision());
    assert!(candidate.preview_omissions().is_empty());
    let content = candidate.content_stamp();
    assert_eq!(
        runtime.current_preview_content_stamp().as_ref(),
        Some(&content)
    );
    assert!(runtime.take_preview_candidate().is_none());
    assert_eq!(
        runtime.current_preview_content_stamp().as_ref(),
        Some(&content)
    );
    let geometry = candidate.geometry_revision();
    let (_, _, _, packet, _, _) = candidate.into_parts();
    let world = runtime.runtime.world();
    let snapshot = runtime.preview_world_snapshot().unwrap().clone();
    assert_eq!(
        snapshot,
        WorldIdentitySnapshot::capture_default(world).unwrap()
    );
    let population = source.document().entities.len();
    let mut projected_ids = world
        .iter_entities()
        .filter_map(|entity| {
            entity
                .get::<SceneEntitySource>()
                .map(|source| source.entity_id.clone())
        })
        .collect::<Vec<_>>();
    let mut source_ids = source
        .document()
        .entities
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<Vec<_>>();
    projected_ids.sort();
    source_ids.sort();
    assert_eq!(projected_ids, source_ids);
    let domain = world.resource::<WorldIdentityDomain>();
    assert_eq!(snapshot.domain_id, Some(domain.id()));
    assert_eq!(snapshot.total_entity_count, population + 1);
    assert_eq!(snapshot.identified_entity_count, population);
    assert_eq!(snapshot.locators.len(), population);
    assert_eq!(snapshot.omitted_locator_count, 0);
    let stats = domain.stats();
    assert_eq!(stats.lifetime_claims, population + 1);
    assert_eq!(stats.claimed_scene_instances, 1);
    assert_eq!(stats.active_scene_instances, 1);
    assert_eq!(stats.active_scene_entities, population);
    assert_eq!(stats.registered_entities, population);
    assert_eq!(stats.persistent_logical_records, 0);
    assert_eq!(stats.recent_tombstones, 0);
    assert!(!runtime.preview_requires_source_projection(DOCUMENT, source.revision(), dependencies));
    Epoch {
        snapshot,
        producer: packet.source_generation(),
        content,
        geometry,
    }
}

fn assert_successor(runtime: &EditorAuthoringRuntime, before: &Epoch, after: &Epoch) {
    assert_ne!(after.snapshot.domain_id, before.snapshot.domain_id);
    assert_eq!(after.producer, before.producer);
    assert!(after.geometry > before.geometry);
    for locator in &before.snapshot.locators {
        assert_eq!(
            resolve_in_world(runtime.runtime.world(), locator),
            EntityLookup::WrongDomain {
                expected: after.snapshot.domain_id.unwrap(),
                actual: before.snapshot.domain_id.unwrap(),
            }
        );
    }
}

fn dependency_participant(revision: SceneAuthoringRevision) -> EditorPreviewParticipant {
    EditorPreviewParticipant {
        source_key: nara_scene::SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "dependent-prefab",
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap(),
        document: EditorDocumentId::from_raw(2),
        revision,
    }
}

fn captured_dependencies(
    generation: u64,
    revision: SceneAuthoringRevision,
) -> EditorPreviewDependencies {
    EditorPreviewDependencies::captured(generation, Arc::from([dependency_participant(revision)]))
}

#[test]
fn tiny_claim_budget_survives_repeated_edits_history_and_structural_publication() {
    let registry = registry();
    let mut source = source(&registry);
    let (runtime, record, handle) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let imported = imported_image(&runtime, &record, handle);
    let version = runtime.commit_image_import(imported).unwrap();
    let owners = RuntimeOwners::capture(&runtime);
    let pixels = runtime
        .image_import_asset(handle, version)
        .unwrap()
        .pixels()
        .as_ptr();
    let mut history = vec![observe(&mut runtime, &source)];
    let mut previous_rotation = 0.0;

    for index in 1..=8 {
        let rotation = index as f32 * 0.125;
        edit_rotation(&mut source, &registry, rotation);
        let edited = observe(&mut runtime, &source);
        assert_ne!(edited.content, history.last().unwrap().content);
        assert_rotation(&runtime, rotation);
        for predecessor in &history {
            assert_successor(&runtime, predecessor, &edited);
        }
        history.push(edited);

        for (redo, expected) in [(false, previous_rotation), (true, rotation)] {
            let before = source.revision();
            let report = if redo {
                source.redo(&registry)
            } else {
                source.undo(&registry)
            };
            assert!(report.applied, "{:?}", report.diagnostics);
            assert!(!report.diagnostics.has_errors());
            assert_eq!(source.revision().source_id(), before.source_id());
            assert_eq!(source.revision().generation(), before.generation() + 1);
            let next = observe(&mut runtime, &source);
            assert_ne!(next.content, history.last().unwrap().content);
            assert_rotation(&runtime, expected);
            for predecessor in &history {
                assert_successor(&runtime, predecessor, &next);
            }
            history.push(next);
        }

        let retained = SceneEntityId::new("retained").unwrap();
        let placement = ScenePlacement::current(source.document(), &retained).unwrap();
        for operation in [
            ScenePatchOperation::RemoveEntity {
                entity: retained,
                from: placement,
            },
            ScenePatchOperation::AddEntity {
                entity: entity(&registry, "retained"),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        ] {
            let report = source.apply_patch(&ScenePatchDocument::new([operation]), &registry);
            assert!(report.applied, "{:?}", report.diagnostics);
            assert!(!report.diagnostics.has_errors());
            let next = observe(&mut runtime, &source);
            assert_ne!(next.content, history.last().unwrap().content);
            assert_rotation(&runtime, rotation);
            for predecessor in &history {
                assert_successor(&runtime, predecessor, &next);
            }
            history.push(next);
        }

        assert_eq!(RuntimeOwners::capture(&runtime), owners);
        assert_eq!(
            runtime
                .image_import_asset(handle, version)
                .unwrap()
                .pixels()
                .as_ptr(),
            pixels
        );
        assert_eq!(
            runtime
                .runtime
                .world()
                .resource::<WorldIdentityDomain>()
                .settings()
                .lifetime_claims()
                .get(),
            3
        );
        for kind in TaskPoolKind::ALL {
            let stats = runtime.persistence_tasks().stats().for_kind(kind);
            assert_eq!(stats.admitted, 0);
            assert_eq!(stats.shutdowns, 0);
        }
        previous_rotation = rotation;
    }
}

#[test]
fn unchanged_source_retains_generation_but_source_switches_always_publish_fresh_epochs() {
    let registry = registry();
    let first_source = source(&registry);
    let second_source = SceneAuthoringSession::new(first_source.document().clone());
    assert_eq!(
        first_source.revision().generation(),
        second_source.revision().generation()
    );
    assert_ne!(
        first_source.revision().source_id(),
        second_source.revision().source_id()
    );
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let first = observe(&mut runtime, &first_source);
    for _ in 0..4 {
        let unchanged = observe(&mut runtime, &first_source);
        assert_eq!(unchanged.snapshot, first.snapshot);
        assert_eq!(unchanged.producer, first.producer);
        assert_eq!(unchanged.content, first.content);
        assert!(unchanged.geometry > first.geometry);
    }
    let second = observe(&mut runtime, &second_source);
    assert_ne!(second.content, first.content);
    assert_successor(&runtime, &first, &second);
    let selected_again = observe(&mut runtime, &first_source);
    assert_eq!(selected_again.content, first.content);
    assert_successor(&runtime, &first, &selected_again);
    assert_successor(&runtime, &second, &selected_again);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn dependency_changes_publish_fresh_epochs_without_changing_the_root_revision() {
    let registry = registry();
    let source = source(&registry);
    let root_revision = source.revision();
    let participant_revision = SceneAuthoringRevision::initial();
    let reopened_revision = SceneAuthoringRevision::initial().next();
    assert_eq!(
        participant_revision.next().generation(),
        reopened_revision.generation()
    );
    assert_ne!(
        participant_revision.source_id(),
        reopened_revision.source_id()
    );
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let mut previous: Option<Epoch> = None;

    for (generation, revision) in [
        (1, participant_revision),
        (2, participant_revision),
        (2, participant_revision.next()),
        (2, reopened_revision),
    ] {
        let dependencies = captured_dependencies(generation, revision);
        assert!(
            runtime.preview_requires_source_projection(DOCUMENT, root_revision, &dependencies,)
        );
        let published = observe_with_dependencies(&mut runtime, &source, &dependencies);
        if let Some(before) = &previous {
            assert_ne!(published.content, before.content);
            assert_successor(&runtime, before, &published);
        }
        for _ in 0..2 {
            let same_capture = captured_dependencies(generation, revision);
            let unchanged = observe_with_dependencies(&mut runtime, &source, &same_capture);
            assert_eq!(unchanged.snapshot, published.snapshot);
            assert_eq!(unchanged.producer, published.producer);
            assert_eq!(unchanged.content, published.content);
            assert!(unchanged.geometry > published.geometry);
        }
        assert_eq!(source.revision(), root_revision);
        assert_rotation(&runtime, 0.0);
        previous = Some(published);
    }
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn invalidating_pending_dependency_content_preserves_the_last_good_world_and_request() {
    let registry = registry();
    let source = source(&registry);
    let root_revision = source.revision();
    let participant_revision = SceneAuthoringRevision::initial();
    let dependencies = captured_dependencies(1, participant_revision);
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    let extent = Extent2d::new(320, 240);
    runtime.set_preview_extent(extent);
    assert_eq!(runtime.preview_extent(), extent);
    assert!(runtime.current_preview_content_stamp().is_none());
    let last_good = observe_with_dependencies(&mut runtime, &source, &dependencies);
    let owners = RuntimeOwners::capture(&runtime);
    assert!(
        runtime
            .drive_preview(Some(EditorPreviewSource {
                document: DOCUMENT,
                revision: root_revision,
                dependencies: dependencies.clone(),
                projection: None,
            }))
            .is_none()
    );

    runtime.invalidate_preview_content();
    assert!(runtime.current_preview_content_stamp().is_none());
    assert!(runtime.take_preview_candidate().is_none());
    assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
    assert!(!runtime.preview_requires_source_projection(DOCUMENT, root_revision, &dependencies));
    assert_rotation(&runtime, 0.0);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);

    let restored = observe_with_dependencies(&mut runtime, &source, &dependencies);
    assert_eq!(restored.content, last_good.content);
    assert_eq!(restored.snapshot, last_good.snapshot);
    runtime.set_preview_extent(extent);
    assert_eq!(
        runtime.current_preview_content_stamp().as_ref(),
        Some(&restored.content)
    );
    runtime.set_preview_extent(Extent2d::new(640, 480));
    assert!(runtime.current_preview_content_stamp().is_none());
    assert!(runtime.take_preview_candidate().is_none());
    assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
    let resized = observe_with_dependencies(&mut runtime, &source, &dependencies);
    assert_ne!(resized.content, restored.content);
    assert_eq!(resized.snapshot, restored.snapshot);

    let changed = captured_dependencies(2, participant_revision.next());
    let successor = observe_with_dependencies(&mut runtime, &source, &changed);
    assert_ne!(successor.content, resized.content);
    assert_eq!(source.revision(), root_revision);
    assert!(runtime.drive_preview(None).is_none());
    assert!(runtime.current_preview_content_stamp().is_none());
    assert!(runtime.take_preview_candidate().is_none());
    let _ = observe_with_dependencies(&mut runtime, &source, &changed);
    runtime.set_preview_extent(None);
    assert_eq!(runtime.preview_extent(), None);
    assert!(runtime.current_preview_content_stamp().is_none());
    assert!(runtime.take_preview_candidate().is_none());
}

#[test]
fn captured_dependencies_compare_canonical_participants_with_exact_source_and_document_identity() {
    let revision = SceneAuthoringRevision::initial();
    let first = dependency_participant(revision);
    let second = EditorPreviewParticipant {
        source_key: preview_source_key(),
        document: DOCUMENT,
        revision,
    };
    let captured =
        EditorPreviewDependencies::captured(1, Arc::from([first.clone(), second.clone()]));
    assert_eq!(
        captured,
        EditorPreviewDependencies::captured(1, Arc::from([second.clone(), first.clone()])),
    );
    let changed_source = EditorPreviewParticipant {
        source_key: preview_source_key(),
        ..first.clone()
    };
    let changed_document = EditorPreviewParticipant {
        document: EditorDocumentId::from_raw(3),
        ..first.clone()
    };
    for changed in [changed_source, changed_document] {
        assert_ne!(
            captured,
            EditorPreviewDependencies::captured(1, Arc::from([changed, second.clone()])),
        );
    }
    assert_ne!(
        captured,
        EditorPreviewDependencies::captured(1, Arc::from([first])),
    );
}

#[test]
fn rejected_capture_keeps_last_good_until_a_successor_closure_is_published() {
    let registry = registry();
    let source = source(&registry);
    let root_revision = source.revision();
    let participant_revision = SceneAuthoringRevision::initial();
    let accepted_dependencies = captured_dependencies(2, participant_revision);
    let rejected_dependencies = captured_dependencies(3, participant_revision);
    let repaired_dependencies = captured_dependencies(4, participant_revision);
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let mut diagnostics = single_error(
        "project.editor.capture-base-missing",
        "A demanded base Prefab could not be captured",
    );
    let _ = diagnostics.extend(single_error(
        "project.editor.capture-receipt-changed",
        "The admitted source receipt changed during capture",
    ));

    assert_eq!(
        runtime.drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: root_revision,
            dependencies: captured_dependencies(1, participant_revision),
            projection: Some(EditorPreviewProjection::rejected(diagnostics.clone())),
        })),
        Some(diagnostics.clone()),
    );
    assert!(runtime.take_preview_candidate().is_none());
    assert!(runtime.preview_world_snapshot().is_none());
    let last_good = observe_with_dependencies(&mut runtime, &source, &accepted_dependencies);
    let stats = runtime
        .runtime
        .world()
        .resource::<WorldIdentityDomain>()
        .stats();
    assert!(
        runtime
            .drive_preview(Some(EditorPreviewSource {
                document: DOCUMENT,
                revision: root_revision,
                dependencies: accepted_dependencies,
                projection: None,
            }))
            .is_none()
    );
    assert!(runtime.preview_requires_source_projection(
        DOCUMENT,
        root_revision,
        &rejected_dependencies,
    ));
    let report = runtime
        .drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: root_revision,
            dependencies: rejected_dependencies.clone(),
            projection: Some(EditorPreviewProjection::rejected(diagnostics.clone())),
        }))
        .expect("a capture rejection must report its complete diagnostics");
    assert_eq!(report, diagnostics);
    assert!(runtime.current_preview_content_stamp().is_none());
    for _ in 0..3 {
        assert!(runtime.take_preview_candidate().is_none());
        assert!(runtime.current_preview_content_stamp().is_none());
        assert!(!runtime.preview_requires_source_projection(
            DOCUMENT,
            root_revision,
            &rejected_dependencies,
        ));
        assert!(runtime.preview_requires_source_projection(
            DOCUMENT,
            root_revision,
            &repaired_dependencies,
        ));
        assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
        assert_eq!(
            WorldIdentitySnapshot::capture_default(runtime.runtime.world()).unwrap(),
            last_good.snapshot,
        );
        assert_eq!(
            runtime
                .runtime
                .world()
                .resource::<WorldIdentityDomain>()
                .stats(),
            stats,
        );
        for locator in &last_good.snapshot.locators {
            assert!(matches!(
                resolve_in_world(runtime.runtime.world(), locator),
                EntityLookup::Resolved(_)
            ));
        }
        assert_rotation(&runtime, 0.0);
        assert!(
            runtime
                .drive_preview(Some(EditorPreviewSource {
                    document: DOCUMENT,
                    revision: root_revision,
                    dependencies: rejected_dependencies.clone(),
                    projection: None,
                }))
                .is_none()
        );
    }
    assert!(runtime.take_preview_candidate().is_none());
    let recovered = observe_with_dependencies(&mut runtime, &source, &repaired_dependencies);
    assert_ne!(recovered.content, last_good.content);
    assert_successor(&runtime, &last_good, &recovered);
    assert_eq!(source.revision(), root_revision);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn rejected_dependencies_keep_last_good_without_authorizing_the_unchanged_root_revision() {
    let registry = registry();
    let source = source(&registry);
    let root_revision = source.revision();
    let participant_revision = SceneAuthoringRevision::initial();
    let accepted_dependencies = captured_dependencies(1, participant_revision);
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let last_good = observe_with_dependencies(&mut runtime, &source, &accepted_dependencies);
    let stats = runtime
        .runtime
        .world()
        .resource::<WorldIdentityDomain>()
        .stats();
    let mut invalid = source.document().clone();
    let component = ComponentTypeId::new("nara.transform.Transform2d");
    invalid.entities.last_mut().unwrap().components.insert(
        component.clone(),
        SceneComponentRecord::new(
            registry.schema(&component).unwrap().version,
            ComponentValue::String("not-a-transform".to_owned()),
        ),
    );

    let revised_dependencies = captured_dependencies(1, participant_revision.next());
    for rejected_dependencies in [
        captured_dependencies(2, participant_revision),
        revised_dependencies.clone(),
    ] {
        assert!(
            runtime
                .drive_preview(Some(EditorPreviewSource {
                    document: DOCUMENT,
                    revision: root_revision,
                    dependencies: accepted_dependencies.clone(),
                    projection: None,
                }))
                .is_none()
        );
        assert!(runtime.preview_requires_source_projection(
            DOCUMENT,
            root_revision,
            &rejected_dependencies,
        ));
        let database = preview_database(&runtime);
        let report = runtime
            .drive_preview(Some(EditorPreviewSource {
                document: DOCUMENT,
                revision: root_revision,
                dependencies: rejected_dependencies.clone(),
                projection: Some(EditorPreviewProjection::authoring(
                    ExpandedSceneCandidate::plain(preview_source_key(), Arc::new(invalid.clone())),
                    EditorAuthoringPreviewOmissions::empty(DOCUMENT, root_revision),
                    database,
                )),
            }))
            .expect("dependency changes must not reuse an accepted root-only stamp");
        assert!(report.has_errors());
        assert!(runtime.current_preview_content_stamp().is_none());
        assert!(runtime.take_preview_candidate().is_none());
        for _ in 0..2 {
            assert!(!runtime.preview_requires_source_projection(
                DOCUMENT,
                root_revision,
                &rejected_dependencies,
            ));
            assert!(
                runtime
                    .drive_preview(Some(EditorPreviewSource {
                        document: DOCUMENT,
                        revision: root_revision,
                        dependencies: rejected_dependencies.clone(),
                        projection: None,
                    }))
                    .is_none()
            );
            assert!(runtime.take_preview_candidate().is_none());
            assert!(runtime.current_preview_content_stamp().is_none());
            assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
            assert_eq!(
                runtime
                    .runtime
                    .world()
                    .resource::<WorldIdentityDomain>()
                    .stats(),
                stats,
            );
            assert_rotation(&runtime, 0.0);
        }
        let restored = observe_with_dependencies(&mut runtime, &source, &accepted_dependencies);
        assert_eq!(restored.snapshot, last_good.snapshot);
        assert_eq!(restored.producer, last_good.producer);
        assert_eq!(restored.content, last_good.content);
    }
    let recovered = observe_with_dependencies(&mut runtime, &source, &revised_dependencies);
    assert_ne!(recovered.content, last_good.content);
    assert_successor(&runtime, &last_good, &recovered);
    assert_eq!(source.revision(), root_revision);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn rejected_projection_keeps_last_good_without_authorizing_the_rejected_revision() {
    let registry = registry();
    let mut source = source(&registry);
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let last_good = observe(&mut runtime, &source);
    let stats = runtime
        .runtime
        .world()
        .resource::<WorldIdentityDomain>()
        .stats();
    let mut invalid = source.document().clone();
    invalid.entities.last_mut().unwrap().components.insert(
        ComponentTypeId::new("nara.transform.Transform2d"),
        SceneComponentRecord::new(
            registry
                .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
                .unwrap()
                .version,
            ComponentValue::String("not-a-transform".to_owned()),
        ),
    );
    let rejected = SceneAuthoringSession::new(invalid);

    // A failing drive must also discard a candidate left by a preceding successful drive.
    assert!(
        runtime
            .drive_preview(Some(EditorPreviewSource {
                document: DOCUMENT,
                revision: source.revision(),
                dependencies: EditorPreviewDependencies::empty(),
                projection: None,
            }))
            .is_none()
    );
    let database = preview_database(&runtime);
    let report = runtime
        .drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: rejected.revision(),
            dependencies: EditorPreviewDependencies::empty(),
            projection: Some(EditorPreviewProjection::authoring(
                ExpandedSceneCandidate::plain(
                    preview_source_key(),
                    Arc::new(rejected.document().clone()),
                ),
                EditorAuthoringPreviewOmissions::empty(DOCUMENT, rejected.revision()),
                database,
            )),
        }))
        .expect("the invalid projection must be reported");
    assert!(report.has_errors());
    for _ in 0..3 {
        assert!(runtime.take_preview_candidate().is_none());
        assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
        assert_eq!(
            runtime
                .runtime
                .world()
                .resource::<WorldIdentityDomain>()
                .stats(),
            stats
        );
        assert_rotation(&runtime, 0.0);
        for locator in &last_good.snapshot.locators {
            assert!(matches!(
                resolve_in_world(runtime.runtime.world(), locator),
                EntityLookup::Resolved(_)
            ));
        }
        assert!(!runtime.preview_requires_source_projection(
            DOCUMENT,
            rejected.revision(),
            &EditorPreviewDependencies::empty(),
        ));
        let _ = runtime.drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: rejected.revision(),
            dependencies: EditorPreviewDependencies::empty(),
            projection: None,
        }));
    }
    assert!(runtime.take_preview_candidate().is_none());
    let restored = observe(&mut runtime, &source);
    assert_eq!(restored.snapshot, last_good.snapshot);
    assert_eq!(restored.producer, last_good.producer);
    assert_eq!(restored.content, last_good.content);
    edit_rotation(&mut source, &registry, 0.5);
    let recovered = observe(&mut runtime, &source);
    assert_ne!(recovered.content, last_good.content);
    assert_successor(&runtime, &last_good, &recovered);
    assert_rotation(&runtime, 0.5);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[derive(Resource)]
struct ForeignIdentityInjection {
    requested: Arc<AtomicBool>,
    token: Option<WorldEntityToken>,
}

fn inject_foreign_identity(world: &mut World) {
    if world
        .resource::<ForeignIdentityInjection>()
        .requested
        .swap(false, Ordering::SeqCst)
    {
        let token = spawn_identity_entity(world).unwrap();
        world.resource_mut::<ForeignIdentityInjection>().token = Some(token);
    }
}

#[test]
fn foreign_unregistered_identity_rejects_preview_epoch_without_erasing_last_good() {
    // Install the fault-injection system before startup; no production mutation seam is added.
    let registry_snapshot = crate::project_host::authoring::first_party_authoring_preview_registry(
        crate::project_host::composition::built_in_schema_providers(),
    )
    .unwrap();
    let registry = ComponentRegistry::from_snapshot(registry_snapshot.clone());
    let mut source = source(&registry);
    let requested = Arc::new(AtomicBool::new(false));
    let mut app = build_authoring_app().unwrap();
    app.insert_resource(ForeignIdentityInjection {
        requested: Arc::clone(&requested),
        token: None,
    })
    .unwrap();
    app.add_systems(CoreStage::First, inject_foreign_identity)
        .unwrap();
    let preview = EditorPreviewSession::install_authoring(
        &mut app,
        &registry_snapshot,
        Arc::new(ProjectAssetDatabase::default()),
    )
    .unwrap();
    let mut runtime = EpochRuntime(EditorAuthoringRuntime {
        runtime: RuntimeInstance::start(app).unwrap(),
        tileset_records: Vec::new(),
        image_revocation_failure: None,
        preview,
        image_publication_revision: 0,
        next_image_import_gate: None,
        fault_after_next_image_publication: false,
    });
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let owners = RuntimeOwners::capture(&runtime);
    let last_good = observe(&mut runtime, &source);
    let stats = runtime
        .runtime
        .world()
        .resource::<WorldIdentityDomain>()
        .stats();
    requested.store(true, Ordering::SeqCst);
    runtime.runtime.drive(Duration::ZERO).unwrap();
    let foreign = runtime
        .runtime
        .world()
        .resource::<ForeignIdentityInjection>()
        .token
        .unwrap();
    assert_eq!(foreign.domain_id(), last_good.snapshot.domain_id.unwrap());
    assert_eq!(
        runtime
            .runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .stats(),
        stats
    );
    edit_rotation(&mut source, &registry, 0.75);
    let database = preview_database(&runtime);
    let report = runtime
        .drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: source.revision(),
            dependencies: EditorPreviewDependencies::empty(),
            projection: Some(EditorPreviewProjection::authoring(
                ExpandedSceneCandidate::plain(
                    preview_source_key(),
                    Arc::new(source.document().clone()),
                ),
                EditorAuthoringPreviewOmissions::empty(DOCUMENT, source.revision()),
                database,
            )),
        }))
        .expect("foreign identity responsibility must reject the epoch");
    assert!(report.has_errors());
    for _ in 0..2 {
        assert!(runtime.take_preview_candidate().is_none());
        let _ = runtime.drive_preview(Some(EditorPreviewSource {
            document: DOCUMENT,
            revision: source.revision(),
            dependencies: EditorPreviewDependencies::empty(),
            projection: None,
        }));
    }
    assert!(runtime.take_preview_candidate().is_none());
    assert_eq!(runtime.preview_world_snapshot(), Some(&last_good.snapshot));
    assert_eq!(
        runtime
            .runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .stats(),
        stats
    );
    assert!(runtime.runtime.world().get_entity(foreign.entity()).is_ok());
    for locator in &last_good.snapshot.locators {
        assert!(matches!(
            resolve_in_world(runtime.runtime.world(), locator),
            EntityLookup::Resolved(_)
        ));
    }
    assert_rotation(&runtime, 0.0);
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn source_none_clears_nonempty_and_empty_projection_epochs_without_restarting_owners() {
    let registry = registry();
    let populated = source(&registry);
    let empty = SceneAuthoringSession::new(SceneDocument::default());
    let (runtime, record, handle) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let imported = imported_image(&runtime, &record, handle);
    let version = runtime.commit_image_import(imported).unwrap();
    let owners = RuntimeOwners::capture(&runtime);
    for source in [&populated, &empty] {
        let active = observe(&mut runtime, source);
        assert!(runtime.drive_preview(None).is_none());
        for _ in 0..2 {
            assert!(runtime.take_preview_candidate().is_none());
            assert!(runtime.preview_world_snapshot().is_none());
            assert!(
                !runtime
                    .runtime
                    .world()
                    .contains_resource::<WorldIdentityDomain>()
            );
            let snapshot = WorldIdentitySnapshot::capture_default(runtime.runtime.world()).unwrap();
            assert_eq!(snapshot.domain_id, None);
            assert!(snapshot.locators.is_empty());
            assert_eq!(snapshot.total_entity_count, 1);
            assert!(
                runtime
                    .runtime
                    .world()
                    .iter_entities()
                    .all(|entity| entity.get::<SceneEntitySource>().is_none())
            );
            for locator in &active.snapshot.locators {
                assert_eq!(
                    resolve_in_world(runtime.runtime.world(), locator),
                    EntityLookup::DomainUnavailable
                );
            }
            assert_eq!(RuntimeOwners::capture(&runtime), owners);
            assert_eq!(
                runtime
                    .image_import_asset(handle, version)
                    .unwrap()
                    .pixels(),
                &[24, 120, 220, 255]
            );
            assert!(runtime.preview_requires_source_projection(
                DOCUMENT,
                source.revision(),
                &EditorPreviewDependencies::empty(),
            ));
            assert!(runtime.drive_preview(None).is_none());
        }
        let republished = observe(&mut runtime, source);
        assert_eq!(republished.content, active.content);
        assert_successor(&runtime, &active, &republished);
    }
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
}

#[test]
fn held_real_image_import_survives_preview_epochs_and_source_clear_then_commits() {
    let registry = registry();
    let mut source = source(&registry);
    let (runtime, record, handle) = authoring_runtime_with_claim_limit(Some(3));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));
    let imported = imported_image(&runtime, &record, handle);
    let initial_version = runtime.commit_image_import(imported).unwrap();
    let owners = RuntimeOwners::capture(&runtime);
    let mut previous = observe(&mut runtime, &source);
    let root = crate::project_host::open_explicit_product_project_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images"),
    )
    .unwrap();
    let path = RelativePath::new("valid-rgba-1x1.png").unwrap();
    let file = root.open_file(&path).unwrap();
    let identity = file.identity();
    let held = runtime.hold_next_image_import();
    let mut task = runtime
        .spawn_image_import(
            1,
            record,
            file,
            identity,
            root,
            path,
            ImageImportSettings::default(),
        )
        .unwrap();
    assert_eq!(task.handle, handle);
    assert!(held.wait_until_started());
    let task_id = task.task.id();
    let budget = runtime.image_import_budget_host();
    let held_budget = budget.snapshot();
    assert!(held_budget.active_reservations() > 0);

    for index in 1..=8 {
        edit_rotation(&mut source, &registry, index as f32 * 0.125);
        let next = observe(&mut runtime, &source);
        assert_ne!(next.content, previous.content);
        assert_successor(&runtime, &previous, &next);
        assert_eq!(RuntimeOwners::capture(&runtime), owners);
        assert_eq!(task.task.id(), task_id);
        assert!(!task.task.cancellation_token().is_cancelled());
        assert!(EditorAuthoringRuntime::poll_image_import(&mut task).is_none());
        assert_eq!(budget.snapshot(), held_budget);
        assert_eq!(runtime.image_import_budget_host().snapshot(), held_budget);
        assert_eq!(
            runtime
                .image_import_asset(handle, initial_version)
                .unwrap()
                .pixels(),
            &[24, 120, 220, 255]
        );
        previous = next;
    }
    assert!(runtime.drive_preview(None).is_none());
    assert!(runtime.take_preview_candidate().is_none());
    assert!(runtime.preview_world_snapshot().is_none());
    assert!(EditorAuthoringRuntime::poll_image_import(&mut task).is_none());
    assert!(!task.task.cancellation_token().is_cancelled());
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
    assert_eq!(budget.snapshot(), held_budget);
    let republished = observe(&mut runtime, &source);
    assert_eq!(republished.content, previous.content);
    assert_successor(&runtime, &previous, &republished);

    assert!(held.release());
    let deadline = Instant::now() + Duration::from_secs(5);
    let terminal = loop {
        if let Some(terminal) = EditorAuthoringRuntime::poll_image_import(&mut task) {
            break terminal;
        }
        assert!(
            Instant::now() < deadline,
            "the released image import must complete"
        );
        std::thread::yield_now();
    };
    let imported = match terminal {
        EditorImageRuntimeTerminal::Imported {
            handle: completed,
            imported,
            source,
        } => {
            assert_eq!(completed, handle);
            assert_eq!(source.identity(), identity);
            imported
        }
        EditorImageRuntimeTerminal::Rejected(reason) => {
            panic!("the retained import was rejected: {reason:?}")
        }
    };
    let version = runtime.commit_image_import(imported).unwrap();
    assert!(version > initial_version);
    let published = observe(&mut runtime, &source);
    assert_eq!(published.snapshot, republished.snapshot);
    assert_eq!(published.producer, republished.producer);
    assert_ne!(published.content, republished.content);
    assert_eq!(
        runtime
            .image_import_asset(handle, version)
            .unwrap()
            .pixels(),
        &[24, 120, 220, 255]
    );
    assert_eq!(RuntimeOwners::capture(&runtime), owners);
    drop(task);
}

#[cfg(feature = "serde")]
#[test]
fn nonempty_omissions_follow_only_the_accepted_epoch_and_preserve_unknown_source_records() {
    use nara_asset::AssetRef;
    use nara_reflect::{
        AuthoringSchemaBundle, ComponentSchemaVersion, PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{AuthoringDocumentSource, SceneDocumentCandidate};

    fn unavailable_source(id: &str) -> SceneAuthoringSession {
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new(id).unwrap(),
        )
        .with_component(
            ComponentTypeId::new("example.Missing"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("opaque retained source".to_owned()),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new("example.missing").unwrap()),
        )]);
        let published = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/source.scene.json").unwrap()),
            &AuthoringSchemaBundle::new([]).unwrap(),
            &[],
        )
        .unwrap()
        .publish();
        SceneAuthoringSession::from_published_authoring_document(published)
    }

    let first = unavailable_source("first");
    let second = unavailable_source("second");
    let first_bytes = first.document().to_json_string().unwrap();
    let second_bytes = second.document().to_json_string().unwrap();
    let project_omissions = |session: &SceneAuthoringSession| {
        let (scene, facts) =
            nara_scene::__private::project_authoring_scene_session_preview(session)
                .unwrap()
                .into_parts();
        (
            scene,
            EditorAuthoringPreviewOmissions::new(DOCUMENT, session.revision(), facts),
        )
    };
    let (first_projection, first_omissions) = project_omissions(&first);
    let (second_projection, second_omissions) = project_omissions(&second);
    assert_eq!(first_omissions.facts().len(), 1);
    assert_eq!(second_omissions.facts().len(), 1);
    assert_ne!(first_omissions.facts(), second_omissions.facts());
    let (runtime, _, _) = authoring_runtime_with_claim_limit(Some(2));
    let mut runtime = EpochRuntime(runtime);
    runtime.set_preview_extent(Extent2d::new(320, 240));

    let publish =
        |runtime: &mut EditorAuthoringRuntime, source: &SceneAuthoringSession, scene, omissions| {
            runtime.drive_preview(Some(EditorPreviewSource {
                document: DOCUMENT,
                revision: source.revision(),
                dependencies: EditorPreviewDependencies::empty(),
                projection: Some(EditorPreviewProjection::authoring(
                    ExpandedSceneCandidate::plain(preview_source_key(), Arc::new(scene)),
                    omissions,
                    preview_database(&runtime),
                )),
            }))
        };
    assert!(
        publish(
            &mut runtime,
            &first,
            first_projection.clone(),
            first_omissions.clone()
        )
        .is_none()
    );
    let candidate = runtime.take_preview_candidate().unwrap();
    assert_eq!(candidate.preview_omissions(), first_omissions.facts());
    let first_producer = candidate.into_parts().3.source_generation();
    let first_snapshot = runtime.preview_world_snapshot().unwrap().clone();

    let mut invalid = second_projection.clone();
    invalid.entities[0].components.insert(
        ComponentTypeId::new("example.NotInstalled"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Bool(true)),
    );
    assert!(
        publish(&mut runtime, &second, invalid, second_omissions.clone())
            .unwrap()
            .has_errors()
    );
    assert!(runtime.take_preview_candidate().is_none());
    assert_eq!(runtime.preview_world_snapshot(), Some(&first_snapshot));

    assert!(
        publish(
            &mut runtime,
            &first,
            first_projection,
            first_omissions.clone()
        )
        .is_none()
    );
    let candidate = runtime.take_preview_candidate().unwrap();
    assert_eq!(candidate.preview_omissions(), first_omissions.facts());
    assert_eq!(candidate.into_parts().3.source_generation(), first_producer);
    assert!(
        publish(
            &mut runtime,
            &second,
            second_projection,
            second_omissions.clone()
        )
        .is_none()
    );
    let candidate = runtime.take_preview_candidate().unwrap();
    assert_eq!(candidate.preview_omissions(), second_omissions.facts());
    assert_eq!(candidate.into_parts().3.source_generation(), first_producer);
    assert_ne!(
        runtime.preview_world_snapshot().unwrap().domain_id,
        first_snapshot.domain_id
    );
    assert_eq!(first.document().to_json_string().unwrap(), first_bytes);
    assert_eq!(second.document().to_json_string().unwrap(), second_bytes);
}
