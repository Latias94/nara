//! Actual verified source successors establish selective, source-only history checkpoints.

#![cfg(feature = "serde")]

use std::{collections::BTreeSet, convert::Infallible, fmt::Debug, mem::size_of, sync::Arc};

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, AuthoringSceneCompilationOmissionReason, CapturedExtractionSource,
        PrefabExtractionBinding, PrefabExtractionDestination, PrefabExtractionEnvironment,
        PrefabExtractionRequest, PrefabExtractionUsage, PrefabExtractionWork,
        PrefabSourceExtractionInput, PreparedPrefabExtraction, PreparedPrefabExtractionSource,
        PreparedSceneExtractionSource, SceneSourceExtractionInput,
        fork_published_authoring_prefab_document, fork_published_authoring_scene_document,
        try_compile_authoring_scene_with_loader,
    },
    AuthoringDocumentSource, AuthoringSchemaBasis, ExpandedSceneDocument, ExpandedSceneEntity,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding, PrefabInstanceHop,
    ResolvedPrefabSource, SceneAuthoringContentState, SceneAuthoringHistoryLimits,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
    SceneAuthoringSession, SceneCompilationLimits, SceneCompilationOptions, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    SceneFileLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport, ScenePlacement,
    ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("content/extraction-checkpoint/{name}.json")).unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "a076a6ec-f5c1-4a13-85d5-d670a9ee29be".parse().unwrap(),
        AssetSourcePath::new(format!("content/extraction-checkpoint/{name}.json")).unwrap(),
        kind,
    )
}

fn metadata(name: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(format!("content/extraction-checkpoint/{name}.json")).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("checkpoint.Link")
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Checkpoint link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("checkpoint.schema").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn file_limits() -> SceneFileLimits {
    SceneFileLimits::default()
        .with_entities(ItemLimit::new(64).unwrap())
        .with_encoded_bytes(ByteLimit::new(128 * 1024).unwrap())
}

fn history_limits() -> SceneAuthoringHistoryLimits {
    SceneAuthoringHistoryLimits::new(8, 256, 4096, 256 * 1024, 64, 1024, 64 * 1024).unwrap()
}

fn open_scene(
    document: &SceneDocument,
    name: &str,
    basis: &AuthoringSchemaBasis,
) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        file_limits(),
    )
    .unwrap()
    .admit_authoring_with_basis(AuthoringDocumentSource::new(path(name)), basis)
    .unwrap()
    .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        published,
        history_limits(),
    )
}

fn open_prefab(document: &PrefabDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let published = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        file_limits(),
    )
    .unwrap()
    .admit_authoring_with_basis(AuthoringDocumentSource::new(path("p")), basis)
    .unwrap()
    .publish();
    PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        published,
        history_limits(),
    )
}

fn local(target: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(target) }
}

fn reference(route: &[(&str, &str)], target: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: route
                .iter()
                .map(|(anchor, source)| PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                    expected_base: key(source, SourceDocumentKind::Prefab),
                })
                .collect(),
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(target))),
        },
    }
}

fn link(target: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(target))]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("checkpoint.schema").unwrap())
}

fn add_root(placements: &[SceneEntityPlacement], name: &str) -> ScenePatchDocument {
    let mut value = placements.to_vec();
    value.push(SceneEntityPlacement::root(id(name)));
    ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id(name)),
        },
        ScenePatchOperation::SetPlacements {
            expected: placements.to_vec(),
            value,
        },
    ])
}

fn copy_bindings(bindings: &[PrefabExtractionBinding]) -> Vec<PrefabExtractionBinding> {
    bindings
        .iter()
        .map(|binding| PrefabExtractionBinding {
            reference: binding.reference.clone(),
            source: binding.source.clone(),
        })
        .collect()
}

struct Fixture {
    basis: AuthoringSchemaBasis,
    scene: SceneAuthoringSession,
    prefab: PrefabAuthoringSession,
    guard: SceneAuthoringSession,
    scene_key: SourceDocumentKey,
    p_key: SourceDocumentKey,
    q_key: SourceDocumentKey,
    guard_key: SourceDocumentKey,
    p_metadata: AssetRecord,
    q_metadata: AssetRecord,
    environment: PrefabExtractionEnvironment,
}

impl Fixture {
    fn new() -> Self {
        let basis = basis();
        let p_metadata = metadata("p", "c5f566b8-930e-4774-b138-12ca69445f91");
        let q_metadata = metadata("q", "7688b7ac-6ed4-4814-b78a-0e7c7a68b454");
        let mut prefab = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("k")),
                SceneEntityPlacement::root(id("x")),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(id("b")),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        id("k"),
                    ))),
                ),
            ],
            [
                SceneEntityRecord::new(id("k")),
                SceneEntityRecord::new(id("b")).with_component(component(), link(local("x"))),
                SceneEntityRecord::new(id("x")).with_component(component(), link(local("b"))),
            ],
        );
        prefab
            .inputs
            .insert(ReferenceInputId::new("original-unused-input").unwrap());
        let before = vec![
            PrefabExtractionBinding {
                reference: path("p"),
                source: key("p", SourceDocumentKind::Prefab),
            },
            PrefabExtractionBinding {
                reference: AssetRef::StableId(p_metadata.stable_id()),
                source: key("p", SourceDocumentKind::Prefab),
            },
        ];
        let mut after = copy_bindings(&before);
        after.push(PrefabExtractionBinding {
            reference: AssetRef::StableId(q_metadata.stable_id()),
            source: key("q", SourceDocumentKind::Prefab),
        });
        let environment = PrefabExtractionEnvironment {
            name: "checkpoint-fixture".into(),
            sources: vec![0, 1, 2],
            before,
            after,
            assets_before: None,
            assets_after: None,
        };
        let mut left = SceneEntityRecord::new(id("left"));
        left.prefab = Some(PrefabInstance::forest(
            environment.before[0].reference.clone(),
        ));
        let mut right = SceneEntityRecord::new(id("right"));
        right.prefab = Some(PrefabInstance::forest(
            environment.before[1].reference.clone(),
        ));
        let scene = SceneDocument::from_ordered_roots([
            left,
            right,
            SceneEntityRecord::new(id("watcher"))
                .with_component(component(), link(reference(&[("left", "p")], "b"))),
        ]);
        let guard = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("opaque"))
            .with_component(
                ComponentTypeId::new("missing.Preserved"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "raw",
                        ComponentValue::String("unavailable but unrelated".into()),
                    )]),
                ),
            )]);
        let mut fixture = Self {
            scene: open_scene(&scene, "scene", &basis),
            prefab: open_prefab(&prefab, &basis),
            guard: open_scene(&guard, "u", &basis),
            basis,
            scene_key: key("scene", SourceDocumentKind::Scene),
            p_key: key("p", SourceDocumentKind::Prefab),
            q_key: key("q", SourceDocumentKind::Prefab),
            guard_key: key("u", SourceDocumentKind::Scene),
            p_metadata,
            q_metadata,
            environment,
        };
        for name in ["kept-edit", "redo-edit"] {
            assert!(
                fixture
                    .scene
                    .apply_authoring_patch(&add_root(&fixture.scene.document().placements, name))
                    .applied
            );
            assert!(
                fixture
                    .prefab
                    .apply_authoring_patch(&add_root(&fixture.prefab.document().placements, name))
                    .applied
            );
            assert!(
                fixture
                    .guard
                    .apply_authoring_patch(&add_root(&fixture.guard.document().placements, name))
                    .applied
            );
        }
        assert!(fixture.scene.undo_authoring().applied);
        assert!(fixture.prefab.undo_authoring().applied);
        assert!(fixture.guard.undo_authoring().applied);
        fixture
    }

    fn p_source(&self, document: Arc<PrefabDocument>) -> ResolvedPrefabSource {
        ResolvedPrefabSource::bound_authoring_source(
            document,
            self.basis.clone(),
            self.p_key.clone(),
        )
        .with_source_asset(&self.p_metadata)
        .unwrap()
    }

    fn request_environment(&self) -> PrefabExtractionEnvironment {
        PrefabExtractionEnvironment {
            name: self.environment.name.clone(),
            sources: self.environment.sources.clone(),
            before: copy_bindings(&self.environment.before),
            after: copy_bindings(&self.environment.after),
            assets_before: None,
            assets_after: None,
        }
    }
}

#[derive(Debug, PartialEq)]
struct Observation<D> {
    document: Arc<D>,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    limits: SceneAuthoringHistoryLimits,
    source: AuthoringDocumentSource,
    basis: AuthoringSchemaBasis,
    upgrade_required: bool,
    live_dirty: Option<bool>,
}

impl<D: Debug + PartialEq> Observation<D> {
    fn assert_unchanged(&self, current: Self) {
        assert!(Arc::ptr_eq(&self.document, &current.document));
        assert_eq!(self, &current);
    }
}

fn observe_scene(session: &SceneAuthoringSession) -> Observation<SceneDocument> {
    Observation {
        document: session.shared_document(),
        revision: session.revision(),
        content: session.content_state(),
        history: session.history_status(),
        usage: session.history_usage(),
        limits: session.history_limits(),
        source: session.authoring_source().unwrap().clone(),
        basis: session.authoring_schema_basis().unwrap(),
        upgrade_required: session.source_upgrade_required(),
        live_dirty: Some(session.is_live_dirty()),
    }
}

fn observe_prefab(session: &PrefabAuthoringSession) -> Observation<PrefabDocument> {
    Observation {
        document: session.shared_document(),
        revision: session.revision(),
        content: session.content_state(),
        history: session.history_status(),
        usage: session.history_usage(),
        limits: session.history_limits(),
        source: session.authoring_source().unwrap().clone(),
        basis: session.authoring_schema_basis().unwrap(),
        upgrade_required: session.source_upgrade_required(),
        live_dirty: None,
    }
}

struct Capture {
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
    scene: SceneSourceExtractionInput,
    prefab: PrefabSourceExtractionInput,
    guard: SceneSourceExtractionInput,
}

fn capture(fixture: &Fixture, limits: SceneCompilationLimits) -> Capture {
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let scene = fixture
        .scene
        .capture_prefab_extraction_input(&mut work)
        .unwrap();
    let prefab = fixture
        .prefab
        .capture_prefab_extraction_input(&mut work)
        .unwrap();
    let guard = fixture
        .guard
        .capture_prefab_extraction_input(&mut work)
        .unwrap();
    // Admit the caller's finite request/environment tables before cloning their owned data.
    let wrapper_bytes = size_of::<PrefabExtractionRequest>()
        + 3 * size_of::<CapturedExtractionSource>()
        + size_of::<PrefabExtractionEnvironment>()
        + fixture.environment.name.len()
        + fixture.environment.sources.len() * size_of::<usize>()
        + (fixture.environment.before.len() + fixture.environment.after.len())
            * size_of::<PrefabExtractionBinding>()
        + fixture
            .environment
            .before
            .iter()
            .chain(&fixture.environment.after)
            .map(|binding| {
                binding.source.checked_retained_bytes().unwrap()
                    + binding
                        .reference
                        .as_path()
                        .map_or(0, |path| path.as_str().len())
            })
            .sum::<usize>()
        + fixture.scene_key.checked_retained_bytes().unwrap()
        + 2 * fixture.p_key.checked_retained_bytes().unwrap()
        + fixture.q_key.checked_retained_bytes().unwrap()
        + fixture.guard_key.checked_retained_bytes().unwrap()
        + fixture.q_metadata.path().as_str().len()
        + "k".len();
    work.admit_capture_storage(Some(wrapper_bytes)).unwrap();
    let request = PrefabExtractionRequest::new(
        vec![
            CapturedExtractionSource::Scene {
                key: fixture.scene_key.clone(),
                document: scene.shared_document(),
                basis: scene.schema_basis(),
            },
            CapturedExtractionSource::Prefab {
                source: fixture.p_source(prefab.shared_document()),
            },
            CapturedExtractionSource::Scene {
                key: fixture.guard_key.clone(),
                document: guard.shared_document(),
                basis: guard.schema_basis(),
            },
        ],
        vec![fixture.request_environment()],
        0,
        fixture.basis.clone(),
        fixture.p_key.clone(),
        id("k"),
        PrefabExtractionDestination {
            key: fixture.q_key.clone(),
            reference: AssetRef::StableId(fixture.q_metadata.stable_id()),
            metadata: fixture.q_metadata.clone(),
        },
    )
    .with_max_depth(16);
    fixture
        .scene
        .validate_prefab_extraction_input(&scene, &mut work)
        .unwrap();
    fixture
        .prefab
        .validate_prefab_extraction_input(&prefab, &mut work)
        .unwrap();
    fixture
        .guard
        .validate_prefab_extraction_input(&guard, &mut work)
        .unwrap();
    Capture {
        request,
        work,
        scene,
        prefab,
        guard,
    }
}

struct WorkerPrepared {
    extraction: PreparedPrefabExtraction,
    inputs: (
        SceneSourceExtractionInput,
        PrefabSourceExtractionInput,
        SceneSourceExtractionInput,
    ),
    scene: PreparedSceneExtractionSource,
    prefab: PreparedPrefabExtractionSource,
    corpus_usage: PrefabExtractionUsage,
    scene_usage: PrefabExtractionUsage,
}

fn worker(captured: Capture) -> WorkerPrepared {
    std::thread::spawn(move || {
        let Capture {
            request,
            work,
            scene,
            prefab,
            guard,
        } = captured;
        let mut extraction = request.prepare(work).unwrap();
        let corpus_usage = extraction.usage();
        let next_scene = extraction.prepare_scene_source(0, &scene).unwrap();
        let scene_usage = extraction.usage();
        let next_prefab = extraction.prepare_prefab_source(1, &prefab).unwrap();
        WorkerPrepared {
            extraction,
            inputs: (scene, prefab, guard),
            scene: next_scene,
            prefab: next_prefab,
            corpus_usage,
            scene_usage,
        }
    })
    .join()
    .expect("actual captures and both move-only source successors must cross a normal worker")
}

fn assert_code(report: &ScenePatchReport, code: &str) {
    assert!(!report.applied);
    assert!(report.diagnostics.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "expected {code}: {:?}",
        report.diagnostics,
    );
}

fn compile(
    document: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    p: &ResolvedPrefabSource,
    bindings: &[PrefabExtractionBinding],
    q: Option<&ResolvedPrefabSource>,
) -> ExpandedSceneDocument {
    let report = try_compile_authoring_scene_with_loader(
        document,
        basis,
        SceneCompilationOptions::default(),
        None,
        |reference, _| {
            let source = bindings
                .iter()
                .find(|binding| &binding.reference == reference)
                .and_then(|binding| {
                    if p.key() == &binding.source {
                        Some(p)
                    } else {
                        q.filter(|source| source.key() == &binding.source)
                    }
                });
            Ok::<_, Infallible>(source.cloned())
        },
    )
    .unwrap();
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics(),
    );
    assert!(report.failure().is_none(), "{:?}", report.failure());
    assert!(!report.budget_exceeded());
    assert!(
        report.omissions().iter().all(|omission| {
            omission.component == component()
                && omission.reason
                    == AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
        }),
        "{:?}",
        report.omissions(),
    );
    report.into_parts().candidate.unwrap()
}

fn projected<'a>(
    document: &'a ExpandedSceneDocument,
    anchor: &str,
    local: &str,
) -> &'a ExpandedSceneEntity {
    let (entity, _) = document
        .provenance()
        .iter()
        .find(|(_, origin)| {
            origin.local_entity_id().as_str() == local
                && origin
                    .projection_path()
                    .is_some_and(|path| path.root_anchor().as_str() == anchor)
        })
        .expect("each repeated occurrence must retain its real source entity");
    document
        .entities()
        .iter()
        .find(|record| record.id() == entity)
        .unwrap()
}

fn assert_references_and_hierarchy(document: &ExpandedSceneDocument) {
    for anchor in ["left", "right"] {
        let k = projected(document, anchor, "k");
        let b = projected(document, anchor, "b");
        let x = projected(document, anchor, "x");
        let host = document
            .entities()
            .iter()
            .find(|entity| entity.id() == &id(anchor))
            .unwrap();
        assert!(
            host.children()
                .starts_with(&[k.id().clone(), x.id().clone()])
        );
        assert_eq!(k.children(), std::slice::from_ref(b.id()));
        assert!(b.children().is_empty());
        assert!(x.children().is_empty());
        for (record, target) in [(b, x), (x, b)] {
            assert_eq!(
                record.components()[&component()].value.get("target"),
                Some(&ComponentValue::EntityReference(
                    EntityReference::SceneLocal {
                        entity: target.id().clone(),
                    }
                )),
            );
        }
    }
    let watcher = document
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("watcher"))
        .unwrap();
    assert_eq!(
        watcher.components()[&component()].value.get("target"),
        Some(&ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: projected(document, "left", "b").id().clone(),
            }
        )),
    );
    assert_ne!(
        projected(document, "left", "b").id(),
        projected(document, "right", "b").id(),
    );
}

#[test]
fn only_actual_changed_outputs_bind_together_and_install_exact_empty_history_checkpoints() {
    let mut fixture = Fixture::new();
    let scene_before = observe_scene(&fixture.scene);
    let prefab_before = observe_prefab(&fixture.prefab);
    let guard_before = observe_scene(&fixture.guard);
    for (history, usage) in [
        (scene_before.history, scene_before.usage),
        (prefab_before.history, prefab_before.usage),
        (guard_before.history, guard_before.usage),
    ] {
        assert_eq!(
            history,
            SceneAuthoringHistoryStatus {
                undo_depth: 1,
                redo_depth: 1,
            },
        );
        assert_eq!(usage.records, 2);
        assert!(usage.operations > 0 && usage.logical_bytes > 0);
    }
    assert!(fixture.scene.is_live_dirty() && fixture.guard.is_live_dirty());
    let guard_manifest = fixture.guard.unavailable_record_manifest().unwrap().clone();
    assert_eq!(guard_manifest.total_count(), 1);
    let before = compile(
        fixture.scene.document(),
        &fixture.basis,
        &fixture.p_source(fixture.prefab.shared_document()),
        &fixture.environment.before,
        None,
    );
    let captured = capture(&fixture, Default::default());
    let capture_usage = captured.work.usage();
    let WorkerPrepared {
        mut extraction,
        inputs: (scene_input, prefab_input, guard_input),
        scene,
        prefab,
        corpus_usage,
        scene_usage,
    } = worker(captured);
    assert!(corpus_usage.work_bytes > capture_usage.work_bytes);
    assert!(corpus_usage.work_nodes > capture_usage.work_nodes);
    assert!(scene_usage.work_bytes > corpus_usage.work_bytes);
    assert!(extraction.usage().work_bytes > scene_usage.work_bytes);
    assert_eq!(extraction.source_changed(0), Some(true));
    assert_eq!(extraction.source_changed(1), Some(true));
    assert_eq!(extraction.source_changed(2), Some(false));
    assert_eq!(extraction.source_changed(3), None);
    assert_code(
        &extraction
            .prepare_scene_source(2, &guard_input)
            .unwrap_err(),
        "scene.prefab-extraction-unchanged-source",
    );
    extraction
        .validate_scene_input(2, &guard_input, &fixture.guard)
        .unwrap();
    fixture.scene.validate_extracted_source(&scene).unwrap();
    fixture.prefab.validate_extracted_source(&prefab).unwrap();
    for (source, input_source, basis, limits, files) in [
        (
            scene.source(),
            scene_input.source(),
            scene.schema_basis(),
            scene.history_limits(),
            scene.file_limits(),
        ),
        (
            prefab.source(),
            prefab_input.source(),
            prefab.schema_basis(),
            prefab.history_limits(),
            prefab.file_limits(),
        ),
    ] {
        assert_eq!(source, input_source);
        assert_eq!(basis, fixture.basis);
        assert_eq!(limits, history_limits());
        assert_eq!(files, file_limits());
    }
    assert_eq!(scene.source_key(), &key("scene", SourceDocumentKind::Scene));
    assert_eq!(prefab.source_key(), &key("p", SourceDocumentKind::Prefab));
    assert_eq!(scene.base_revision(), scene_input.revision());
    assert_eq!(prefab.base_revision(), prefab_input.revision());
    assert_eq!(scene.base_content_state(), scene_input.content_state());
    assert_eq!(prefab.base_content_state(), prefab_input.content_state());
    let future_scene = scene.shared_document();
    let future_prefab = prefab.shared_document();
    let scene_revision = scene.revision();
    let prefab_revision = prefab.revision();
    let scene_content = scene.content_state();
    let prefab_content = prefab.content_state();
    assert_ne!(
        scene_revision.source_id(),
        scene_before.revision.source_id()
    );
    assert_ne!(
        prefab_revision.source_id(),
        prefab_before.revision.source_id(),
    );
    assert_eq!(scene_revision.generation(), 0);
    assert_eq!(prefab_revision.generation(), 0);
    assert_eq!(scene_content, scene_revision.content_state());
    assert_eq!(prefab_content, prefab_revision.content_state());
    assert!(!Arc::ptr_eq(&future_scene, &scene_input.shared_document()));
    assert!(!Arc::ptr_eq(
        &future_prefab,
        &prefab_input.shared_document()
    ));
    let CapturedExtractionSource::Scene { document, .. } = &extraction.sources()[0] else {
        panic!("the changed Scene must remain the actual corpus output");
    };
    assert!(Arc::ptr_eq(&future_scene, document));
    let CapturedExtractionSource::Prefab { source, .. } = &extraction.sources()[1] else {
        panic!("the changed Prefab must remain the actual corpus output");
    };
    assert!(std::ptr::eq(prefab.document(), source.document()));
    let watcher = scene
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == id("watcher"))
        .unwrap();
    assert_eq!(
        watcher.components[&component()].value.get("target"),
        Some(&ComponentValue::EntityReference(reference(
            &[("left", "p"), ("k", "q")],
            "b",
        ))),
    );
    assert_eq!(prefab.document().inputs, prefab_input.document().inputs);
    assert_eq!(
        prefab
            .document()
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["k", "x", "kept-edit"]),
    );
    assert_eq!(
        extraction
            .extracted()
            .document()
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["b", "k"]),
    );
    let anchor = prefab
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == id("k"))
        .unwrap();
    let instance = anchor.prefab.as_ref().unwrap();
    let q_reference = AssetRef::StableId(fixture.q_metadata.stable_id());
    assert_eq!(instance.source, q_reference);
    assert_eq!(
        instance.binding,
        PrefabInstanceBinding::Root {
            base: fixture.q_key.clone(),
            root: PrefabEntityTarget::source(id("k")),
        },
    );
    let arguments = instance.arguments.as_ref().unwrap();
    assert_eq!(arguments.base, key("q", SourceDocumentKind::Prefab));
    assert_eq!(arguments.values.len(), 1);
    assert_eq!(
        extraction.extracted().document().inputs,
        arguments.values.keys().cloned().collect::<BTreeSet<_>>(),
    );
    let (input, target) = arguments.values.first_key_value().unwrap();
    assert_eq!(target, &local("x"));
    let moved = extraction
        .extracted()
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == id("b"))
        .unwrap();
    assert_eq!(
        moved.components[&component()].value.get("target"),
        Some(&ComponentValue::EntityReference(
            EntityReference::Authored {
                reference: AuthoredEntityReference::input(input.clone()),
            }
        )),
    );
    let outside = prefab
        .document()
        .entities
        .iter()
        .find(|entity| entity.id == id("x"))
        .unwrap();
    assert_eq!(
        outside.components[&component()].value.get("target"),
        Some(&ComponentValue::EntityReference(reference(
            &[("k", "q")],
            "b"
        ))),
    );
    let after = compile(
        scene.document(),
        &fixture.basis,
        &fixture.p_source(prefab.shared_document()),
        &fixture.environment.after,
        Some(extraction.extracted()),
    );
    assert_eq!(
        before.roots(),
        &[id("left"), id("right"), id("watcher"), id("kept-edit")],
    );
    assert_eq!(after.roots(), before.roots());
    assert_eq!(before.entities().len(), 12);
    assert_eq!(after.entities().len(), before.entities().len());
    assert_references_and_hierarchy(&before);
    assert_references_and_hierarchy(&after);
    scene_before.assert_unchanged(observe_scene(&fixture.scene));
    prefab_before.assert_unchanged(observe_prefab(&fixture.prefab));
    guard_before.assert_unchanged(observe_scene(&fixture.guard));

    let bound_scene = fixture.scene.bind_extracted_source(scene).unwrap();
    assert!(Arc::ptr_eq(
        &bound_scene.prepared().shared_document(),
        &future_scene,
    ));
    let scene = bound_scene.into_prepared();
    let bound_prefab = fixture.prefab.bind_extracted_source(prefab).unwrap();
    assert!(Arc::ptr_eq(
        &bound_prefab.prepared().shared_document(),
        &future_prefab,
    ));
    let prefab = bound_prefab.into_prepared();
    assert_eq!(scene.revision(), scene_revision);
    assert_eq!(prefab.revision(), prefab_revision);
    assert_eq!(scene.content_state(), scene_content);
    assert_eq!(prefab.content_state(), prefab_content);
    assert!(Arc::ptr_eq(&scene.shared_document(), &future_scene));
    assert!(Arc::ptr_eq(&prefab.shared_document(), &future_prefab));
    scene_before.assert_unchanged(observe_scene(&fixture.scene));
    prefab_before.assert_unchanged(observe_prefab(&fixture.prefab));

    // Bind every changed live owner before the first commit; U remains an unborrowed guard.
    let bound_scene = fixture.scene.bind_extracted_source(scene).unwrap();
    let bound_prefab = fixture.prefab.bind_extracted_source(prefab).unwrap();
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
    let scene = bound_scene.commit();
    let prefab = bound_prefab.commit();
    assert!(Arc::ptr_eq(&scene.shared_document(), &future_scene));
    assert!(Arc::ptr_eq(&prefab.shared_document(), &future_prefab));
    assert_eq!(scene.revision(), scene_revision);
    assert_eq!(prefab.revision(), prefab_revision);
    assert_eq!(scene.content_state(), scene_content);
    assert_eq!(prefab.content_state(), prefab_content);
    assert_eq!(
        scene.history_status(),
        SceneAuthoringHistoryStatus::default()
    );
    assert_eq!(
        prefab.history_status(),
        SceneAuthoringHistoryStatus::default()
    );
    assert_eq!(scene.history_usage(), SceneAuthoringHistoryUsage::default());
    assert_eq!(
        prefab.history_usage(),
        SceneAuthoringHistoryUsage::default()
    );
    assert!(!scene.owns_content_state(scene_before.content));
    assert!(!prefab.owns_content_state(prefab_before.content));
    assert_eq!(scene.authoring_source(), Some(scene_input.source()));
    assert_eq!(prefab.authoring_source(), Some(prefab_input.source()));
    assert_eq!(scene.authoring_schema_basis(), Some(fixture.basis.clone()));
    assert_eq!(prefab.authoring_schema_basis(), Some(fixture.basis.clone()));
    assert_eq!(scene.history_limits(), scene_input.history_limits());
    assert_eq!(prefab.history_limits(), prefab_input.history_limits());
    assert!(scene.is_live_dirty());
    let mut fresh_work = PrefabExtractionWork::new(Default::default()).unwrap();
    assert_eq!(
        scene
            .capture_prefab_extraction_input(&mut fresh_work)
            .unwrap()
            .file_limits(),
        scene_input.file_limits(),
    );
    assert_eq!(
        prefab
            .capture_prefab_extraction_input(&mut fresh_work)
            .unwrap()
            .file_limits(),
        prefab_input.file_limits(),
    );
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
    assert_eq!(
        fixture.guard.unavailable_record_manifest(),
        Some(&guard_manifest),
    );
    let live = compile(
        fixture.scene.document(),
        &fixture.basis,
        &fixture.p_source(fixture.prefab.shared_document()),
        &fixture.environment.after,
        Some(extraction.extracted()),
    );
    assert_eq!(live.roots(), after.roots());
    assert_eq!(live.entities().len(), after.entities().len());
    assert_references_and_hierarchy(&live);
}

#[test]
fn dropping_both_exclusive_source_guards_preserves_the_actual_documents_and_both_history_branches()
{
    let mut fixture = Fixture::new();
    let scene_before = observe_scene(&fixture.scene);
    let prefab_before = observe_prefab(&fixture.prefab);
    let guard_before = observe_scene(&fixture.guard);
    let prepared = worker(capture(&fixture, Default::default()));
    let scene = fixture.scene.bind_extracted_source(prepared.scene).unwrap();
    let prefab = fixture
        .prefab
        .bind_extracted_source(prepared.prefab)
        .unwrap();
    assert_ne!(scene.prepared().revision(), scene_before.revision);
    assert_ne!(prefab.prepared().revision(), prefab_before.revision);
    drop(scene);
    drop(prefab);
    scene_before.assert_unchanged(observe_scene(&fixture.scene));
    prefab_before.assert_unchanged(observe_prefab(&fixture.prefab));
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
}

#[test]
fn checkpointed_sources_support_real_edits_undo_redo_and_json_ron_readmission_with_q() {
    let mut fixture = Fixture::new();
    let guard_before = observe_scene(&fixture.guard);
    let prepared = worker(capture(&fixture, Default::default()));
    let scene = fixture.scene.bind_extracted_source(prepared.scene).unwrap();
    let prefab = fixture
        .prefab
        .bind_extracted_source(prepared.prefab)
        .unwrap();
    scene.commit();
    prefab.commit();
    let checkpoint_scene = fixture.scene.shared_document();
    let checkpoint_prefab = fixture.prefab.shared_document();
    let scene_state = fixture.scene.content_state();
    let prefab_state = fixture.prefab.content_state();
    let scene_source = fixture.scene.revision().source_id();
    let prefab_source = fixture.prefab.revision().source_id();
    assert!(
        fixture
            .scene
            .apply_authoring_patch(&add_root(
                &fixture.scene.document().placements,
                "post-scene"
            ))
            .applied
    );
    assert!(
        fixture
            .prefab
            .apply_authoring_patch(&add_root(
                &fixture.prefab.document().placements,
                "post-prefab"
            ))
            .applied
    );
    let edited_scene = fixture.scene.shared_document();
    let edited_prefab = fixture.prefab.shared_document();
    let edited_scene_state = fixture.scene.content_state();
    let edited_prefab_state = fixture.prefab.content_state();
    assert_eq!(fixture.scene.history_status().undo_depth, 1);
    assert_eq!(fixture.prefab.history_status().undo_depth, 1);
    assert_eq!(fixture.scene.history_usage().records, 1);
    assert_eq!(fixture.prefab.history_usage().records, 1);
    assert!(fixture.scene.undo_authoring().applied);
    assert!(fixture.prefab.undo_authoring().applied);
    assert_eq!(fixture.scene.document(), checkpoint_scene.as_ref());
    assert_eq!(fixture.prefab.document(), checkpoint_prefab.as_ref());
    assert_eq!(fixture.scene.content_state(), scene_state);
    assert_eq!(fixture.prefab.content_state(), prefab_state);
    assert_eq!(
        fixture.scene.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 1
        }
    );
    assert_eq!(
        fixture.prefab.history_status(),
        SceneAuthoringHistoryStatus {
            undo_depth: 0,
            redo_depth: 1
        }
    );
    let undone = compile(
        fixture.scene.document(),
        &fixture.basis,
        &fixture.p_source(fixture.prefab.shared_document()),
        &fixture.environment.after,
        Some(prepared.extraction.extracted()),
    );
    assert_eq!(undone.entities().len(), 12);
    assert_references_and_hierarchy(&undone);
    assert!(fixture.scene.redo_authoring().applied);
    assert!(fixture.prefab.redo_authoring().applied);
    assert_eq!(fixture.scene.document(), edited_scene.as_ref());
    assert_eq!(fixture.prefab.document(), edited_prefab.as_ref());
    assert_eq!(fixture.scene.content_state(), edited_scene_state);
    assert_eq!(fixture.prefab.content_state(), edited_prefab_state);
    assert_eq!(fixture.scene.revision().source_id(), scene_source);
    assert_eq!(fixture.prefab.revision().source_id(), prefab_source);
    let scene_before_save = observe_scene(&fixture.scene);
    let prefab_before_save = observe_prefab(&fixture.prefab);
    let q = prepared.extraction.extracted().document();
    // Exercise the real bounded persistent formats and the original nominal admission basis.
    for (scene_candidate, prefab_candidate, q_candidate) in [
        (
            SceneDocumentCandidate::decode_json_bytes_with_limits(
                fixture
                    .scene
                    .document()
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
                file_limits(),
            )
            .unwrap(),
            PrefabDocumentCandidate::decode_json_bytes_with_limits(
                fixture
                    .prefab
                    .document()
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
                file_limits(),
            )
            .unwrap(),
            PrefabDocumentCandidate::decode_json_bytes_with_limits(
                q.to_json_string().unwrap().as_bytes(),
                file_limits(),
            )
            .unwrap(),
        ),
        (
            SceneDocumentCandidate::decode_ron_bytes_with_limits(
                fixture.scene.document().to_ron_string().unwrap().as_bytes(),
                file_limits(),
            )
            .unwrap(),
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                fixture
                    .prefab
                    .document()
                    .to_ron_string()
                    .unwrap()
                    .as_bytes(),
                file_limits(),
            )
            .unwrap(),
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                q.to_ron_string().unwrap().as_bytes(),
                file_limits(),
            )
            .unwrap(),
        ),
    ] {
        let scene = fixture
            .scene
            .try_reopen_authoring_candidate(scene_candidate)
            .unwrap();
        let prefab = fixture
            .prefab
            .try_reopen_authoring_candidate(prefab_candidate)
            .unwrap();
        let q = PrefabAuthoringSession::from_published_authoring_document(
            q_candidate
                .admit_authoring_with_basis(AuthoringDocumentSource::new(path("q")), &fixture.basis)
                .unwrap()
                .publish(),
        );
        assert_eq!(scene.document(), fixture.scene.document());
        assert_eq!(prefab.document(), fixture.prefab.document());
        assert_eq!(q.document(), prepared.extraction.extracted().document());
        assert_eq!(scene.authoring_schema_basis(), Some(fixture.basis.clone()));
        assert_eq!(prefab.authoring_schema_basis(), Some(fixture.basis.clone()));
        assert_eq!(q.authoring_schema_basis(), Some(fixture.basis.clone()));
        assert_eq!(scene.authoring_source(), fixture.scene.authoring_source());
        assert_eq!(prefab.authoring_source(), fixture.prefab.authoring_source());
        assert_eq!(scene.history_limits(), history_limits());
        assert_eq!(prefab.history_limits(), history_limits());
        let q = ResolvedPrefabSource::bound_authoring_source(
            q.shared_document(),
            fixture.basis.clone(),
            key("q", SourceDocumentKind::Prefab),
        )
        .with_source_asset(&fixture.q_metadata)
        .unwrap();
        let compiled = compile(
            scene.document(),
            &fixture.basis,
            &fixture.p_source(prefab.shared_document()),
            &fixture.environment.after,
            Some(&q),
        );
        assert_eq!(
            compiled.roots(),
            &[
                id("left"),
                id("right"),
                id("watcher"),
                id("kept-edit"),
                id("post-scene")
            ]
        );
        assert_eq!(compiled.entities().len(), 15);
        for anchor in ["left", "right"] {
            let extra = projected(&compiled, anchor, "post-prefab");
            let host = compiled
                .entities()
                .iter()
                .find(|entity| entity.id() == &id(anchor))
                .unwrap();
            assert_eq!(host.children().last(), Some(extra.id()));
        }
        assert_references_and_hierarchy(&compiled);
    }
    scene_before_save.assert_unchanged(observe_scene(&fixture.scene));
    prefab_before_save.assert_unchanged(observe_prefab(&fixture.prefab));
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
}

#[test]
fn real_undo_and_equal_content_redo_refuse_binding_and_return_the_exact_future_incarnations() {
    let mut fixture = Fixture::new();
    let guard_before = observe_scene(&fixture.guard);
    let prepared = worker(capture(&fixture, Default::default()));
    let future_scene = prepared.scene.shared_document();
    let future_prefab = prepared.prefab.shared_document();
    let scene_revision = prepared.scene.revision();
    let prefab_revision = prepared.prefab.revision();
    let scene_content = prepared.scene.content_state();
    let prefab_content = prepared.prefab.content_state();
    let mut scene = prepared.scene;
    let mut prefab = prepared.prefab;
    assert!(fixture.scene.undo_authoring().applied);
    assert!(fixture.prefab.undo_authoring().applied);
    for redo in [false, true] {
        if redo {
            assert!(fixture.scene.redo_authoring().applied);
            assert!(fixture.prefab.redo_authoring().applied);
            assert_eq!(fixture.scene.document(), prepared.inputs.0.document());
            assert_eq!(fixture.prefab.document(), prepared.inputs.1.document());
            assert_eq!(
                fixture.scene.content_state(),
                prepared.inputs.0.content_state()
            );
            assert_eq!(
                fixture.prefab.content_state(),
                prepared.inputs.1.content_state()
            );
            assert_ne!(fixture.scene.revision(), prepared.inputs.0.revision());
            assert_ne!(fixture.prefab.revision(), prepared.inputs.1.revision());
        }
        let scene_before = observe_scene(&fixture.scene);
        let prefab_before = observe_prefab(&fixture.prefab);
        assert_code(
            &fixture.scene.validate_extracted_source(&scene).unwrap_err(),
            "scene.prefab-extraction-stale",
        );
        assert_code(
            &fixture
                .prefab
                .validate_extracted_source(&prefab)
                .unwrap_err(),
            "scene.prefab-extraction-stale",
        );
        let scene_failure = fixture.scene.bind_extracted_source(scene).unwrap_err();
        let prefab_failure = fixture.prefab.bind_extracted_source(prefab).unwrap_err();
        assert_code(scene_failure.report(), "scene.prefab-extraction-stale");
        assert_code(prefab_failure.report(), "scene.prefab-extraction-stale");
        let (returned_scene, scene_report) = scene_failure.into_parts();
        let (returned_prefab, prefab_report) = prefab_failure.into_parts();
        assert_code(&scene_report, "scene.prefab-extraction-stale");
        assert_code(&prefab_report, "scene.prefab-extraction-stale");
        assert!(Arc::ptr_eq(
            &returned_scene.shared_document(),
            &future_scene
        ));
        assert!(Arc::ptr_eq(
            &returned_prefab.shared_document(),
            &future_prefab
        ));
        assert_eq!(returned_scene.revision(), scene_revision);
        assert_eq!(returned_prefab.revision(), prefab_revision);
        assert_eq!(returned_scene.content_state(), scene_content);
        assert_eq!(returned_prefab.content_state(), prefab_content);
        scene_before.assert_unchanged(observe_scene(&fixture.scene));
        prefab_before.assert_unchanged(observe_prefab(&fixture.prefab));
        scene = returned_scene;
        prefab = returned_prefab;
    }
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
}

#[test]
fn real_source_reopen_refuses_old_tokens_without_changing_either_reopened_owner() {
    let mut fixture = Fixture::new();
    let prepared = worker(capture(&fixture, Default::default()));
    let future_scene = prepared.scene.shared_document();
    let future_prefab = prepared.prefab.shared_document();
    let scene_revision = prepared.scene.revision();
    let prefab_revision = prepared.prefab.revision();
    fixture.scene = fixture
        .scene
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_json_bytes_with_limits(
                prepared
                    .inputs
                    .0
                    .document()
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
                file_limits(),
            )
            .unwrap(),
        )
        .unwrap();
    fixture.prefab = fixture
        .prefab
        .try_reopen_authoring_candidate(
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                prepared
                    .inputs
                    .1
                    .document()
                    .to_ron_string()
                    .unwrap()
                    .as_bytes(),
                file_limits(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(fixture.scene.document(), prepared.inputs.0.document());
    assert_eq!(fixture.prefab.document(), prepared.inputs.1.document());
    assert_eq!(
        fixture.scene.authoring_schema_basis(),
        Some(fixture.basis.clone())
    );
    assert_eq!(
        fixture.prefab.authoring_schema_basis(),
        Some(fixture.basis.clone())
    );
    assert_ne!(
        fixture.scene.revision().source_id(),
        prepared.inputs.0.revision().source_id()
    );
    assert_ne!(
        fixture.prefab.revision().source_id(),
        prepared.inputs.1.revision().source_id()
    );
    let scene_before = observe_scene(&fixture.scene);
    let prefab_before = observe_prefab(&fixture.prefab);
    let guard_before = observe_scene(&fixture.guard);
    let scene_failure = fixture
        .scene
        .bind_extracted_source(prepared.scene)
        .unwrap_err();
    let prefab_failure = fixture
        .prefab
        .bind_extracted_source(prepared.prefab)
        .unwrap_err();
    let (scene, scene_report) = scene_failure.into_parts();
    let (prefab, prefab_report) = prefab_failure.into_parts();
    assert_code(&scene_report, "scene.prefab-extraction-stale");
    assert_code(&prefab_report, "scene.prefab-extraction-stale");
    assert!(Arc::ptr_eq(&scene.shared_document(), &future_scene));
    assert!(Arc::ptr_eq(&prefab.shared_document(), &future_prefab));
    assert_eq!(scene.revision(), scene_revision);
    assert_eq!(prefab.revision(), prefab_revision);
    scene_before.assert_unchanged(observe_scene(&fixture.scene));
    prefab_before.assert_unchanged(observe_prefab(&fixture.prefab));
    guard_before.assert_unchanged(observe_scene(&fixture.guard));
}

#[test]
fn foreign_owners_sharing_original_allocations_cannot_consume_the_original_owners_successors() {
    let mut fixture = Fixture::new();
    let scene_publication = SceneDocumentCandidate::decode_json_bytes_with_limits(
        fixture
            .scene
            .document()
            .to_json_string()
            .unwrap()
            .as_bytes(),
        file_limits(),
    )
    .unwrap()
    .admit_authoring_with_basis(AuthoringDocumentSource::new(path("scene")), &fixture.basis)
    .unwrap()
    .publish();
    let prefab_publication = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        fixture
            .prefab
            .document()
            .to_json_string()
            .unwrap()
            .as_bytes(),
        file_limits(),
    )
    .unwrap()
    .admit_authoring_with_basis(AuthoringDocumentSource::new(path("p")), &fixture.basis)
    .unwrap()
    .publish();
    let mut foreign_scene =
        SceneAuthoringSession::from_published_authoring_document_with_history_limits(
            fork_published_authoring_scene_document(&scene_publication),
            history_limits(),
        );
    let mut foreign_prefab =
        PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
            fork_published_authoring_prefab_document(&prefab_publication),
            history_limits(),
        );
    fixture.scene = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        scene_publication,
        history_limits(),
    );
    fixture.prefab = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        prefab_publication,
        history_limits(),
    );
    assert!(Arc::ptr_eq(
        &fixture.scene.shared_document(),
        &foreign_scene.shared_document()
    ));
    assert!(Arc::ptr_eq(
        &fixture.prefab.shared_document(),
        &foreign_prefab.shared_document()
    ));
    assert_eq!(
        fixture.scene.authoring_schema_basis(),
        foreign_scene.authoring_schema_basis()
    );
    assert_eq!(
        fixture.prefab.authoring_schema_basis(),
        foreign_prefab.authoring_schema_basis()
    );
    let original_scene = observe_scene(&fixture.scene);
    let original_prefab = observe_prefab(&fixture.prefab);
    let foreign_scene_before = observe_scene(&foreign_scene);
    let foreign_prefab_before = observe_prefab(&foreign_prefab);
    let prepared = worker(capture(&fixture, Default::default()));
    let future_scene = prepared.scene.shared_document();
    let future_prefab = prepared.prefab.shared_document();
    let scene_revision = prepared.scene.revision();
    let prefab_revision = prepared.prefab.revision();
    let (scene, scene_report) = foreign_scene
        .bind_extracted_source(prepared.scene)
        .unwrap_err()
        .into_parts();
    let (prefab, prefab_report) = foreign_prefab
        .bind_extracted_source(prepared.prefab)
        .unwrap_err()
        .into_parts();
    assert_code(&scene_report, "scene.prefab-extraction-stale");
    assert_code(&prefab_report, "scene.prefab-extraction-stale");
    assert!(Arc::ptr_eq(&scene.shared_document(), &future_scene));
    assert!(Arc::ptr_eq(&prefab.shared_document(), &future_prefab));
    assert_eq!(scene.revision(), scene_revision);
    assert_eq!(prefab.revision(), prefab_revision);
    foreign_scene_before.assert_unchanged(observe_scene(&foreign_scene));
    foreign_prefab_before.assert_unchanged(observe_prefab(&foreign_prefab));
    fixture.scene.validate_extracted_source(&scene).unwrap();
    fixture.prefab.validate_extracted_source(&prefab).unwrap();
    let bound_scene = fixture.scene.bind_extracted_source(scene).unwrap();
    let bound_prefab = fixture.prefab.bind_extracted_source(prefab).unwrap();
    let scene = bound_scene.into_prepared();
    let prefab = bound_prefab.into_prepared();
    assert_eq!(scene.revision(), scene_revision);
    assert_eq!(prefab.revision(), prefab_revision);
    original_scene.assert_unchanged(observe_scene(&fixture.scene));
    original_prefab.assert_unchanged(observe_prefab(&fixture.prefab));
}

#[test]
fn exact_and_one_short_scene_token_budgets_include_capture_corpus_and_classification() {
    let fixture = Fixture::new();
    let captured = capture(&fixture, Default::default());
    let capture_usage = captured.work.usage();
    let measured = worker(captured);
    assert!(measured.corpus_usage.work_bytes > capture_usage.work_bytes);
    assert!(measured.scene_usage.work_bytes > measured.corpus_usage.work_bytes);
    let token_bytes = measured.scene_usage.work_bytes - measured.corpus_usage.work_bytes;
    assert!(token_bytes > 0);
    for exact in [true, false] {
        let fixture = Fixture::new();
        let before_scene = observe_scene(&fixture.scene);
        let before_prefab = observe_prefab(&fixture.prefab);
        let before_guard = observe_scene(&fixture.guard);
        let boundary = measured.corpus_usage.work_bytes + token_bytes - usize::from(!exact);
        let limits =
            SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(boundary).unwrap());
        let captured = capture(&fixture, limits);
        assert_eq!(captured.work.usage(), capture_usage);
        let (extraction, input, result) = std::thread::spawn(move || {
            let mut extraction = captured.request.prepare(captured.work).unwrap();
            let result = extraction.prepare_scene_source(0, &captured.scene);
            (extraction, captured.scene, result)
        })
        .join()
        .expect("token admission runs on the real captured worker");
        if exact {
            let scene = result.unwrap();
            assert_eq!(extraction.usage(), measured.scene_usage);
            fixture.scene.validate_extracted_source(&scene).unwrap();
            assert_eq!(scene.document(), measured.scene.document());
            assert_ne!(scene.revision().source_id(), input.revision().source_id());
        } else {
            assert_code(&result.unwrap_err(), "scene.prefab-extraction-capacity");
            assert!(extraction.usage().work_bytes >= measured.corpus_usage.work_bytes);
            assert!(extraction.usage().work_bytes <= boundary);
        }
        let CapturedExtractionSource::Scene { document, .. } = &extraction.original_sources()[0]
        else {
            panic!("original Scene");
        };
        assert!(Arc::ptr_eq(document, &input.shared_document()));
        before_scene.assert_unchanged(observe_scene(&fixture.scene));
        before_prefab.assert_unchanged(observe_prefab(&fixture.prefab));
        before_guard.assert_unchanged(observe_scene(&fixture.guard));
    }
}

#[test]
fn exact_and_one_short_prefab_token_budgets_keep_the_prepared_scene_and_original_live_sources() {
    let fixture = Fixture::new();
    let measured = worker(capture(&fixture, Default::default()));
    let total = measured.extraction.usage();
    let token_bytes = total.work_bytes - measured.scene_usage.work_bytes;
    assert!(token_bytes > 0);
    for exact in [true, false] {
        let fixture = Fixture::new();
        let before_scene = observe_scene(&fixture.scene);
        let before_prefab = observe_prefab(&fixture.prefab);
        let before_guard = observe_scene(&fixture.guard);
        let boundary = measured.scene_usage.work_bytes + token_bytes - usize::from(!exact);
        let limits =
            SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(boundary).unwrap());
        let captured = capture(&fixture, limits);
        let (extraction, input, scene, result) = std::thread::spawn(move || {
            let mut extraction = captured.request.prepare(captured.work).unwrap();
            let scene = extraction.prepare_scene_source(0, &captured.scene).unwrap();
            let result = extraction.prepare_prefab_source(1, &captured.prefab);
            (extraction, captured.prefab, scene, result)
        })
        .join()
        .expect("both token preparations retain the original worker allowance");
        fixture.scene.validate_extracted_source(&scene).unwrap();
        assert_eq!(scene.document(), measured.scene.document());
        if exact {
            let prefab = result.unwrap();
            assert_eq!(extraction.usage(), total);
            fixture.prefab.validate_extracted_source(&prefab).unwrap();
            assert_eq!(prefab.document(), measured.prefab.document());
            assert_ne!(prefab.revision().source_id(), input.revision().source_id());
        } else {
            assert_code(&result.unwrap_err(), "scene.prefab-extraction-capacity");
            assert!(extraction.usage().work_bytes >= measured.scene_usage.work_bytes);
            assert!(extraction.usage().work_bytes <= boundary);
        }
        let CapturedExtractionSource::Prefab { source, .. } = &extraction.original_sources()[1]
        else {
            panic!("original Prefab");
        };
        assert!(std::ptr::eq(source.document(), input.document()));
        before_scene.assert_unchanged(observe_scene(&fixture.scene));
        before_prefab.assert_unchanged(observe_prefab(&fixture.prefab));
        before_guard.assert_unchanged(observe_scene(&fixture.guard));
    }
}
