//! Captured CP2 inputs prepare real sources without acquiring live installation authority.

#![cfg(feature = "serde")]

use std::{collections::BTreeSet, convert::Infallible, mem::size_of, sync::Arc};

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredReferenceTarget, EntityReference, ReferenceInputId};
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
        PrefabExtractionFailure, PrefabExtractionRequest, PrefabExtractionWork,
        PrefabSourceExtractionInput, PreparedPrefabExtraction, SceneSourceExtractionInput,
        try_compile_authoring_scene_with_loader,
    },
    AuthoringDocumentSource, AuthoringSchemaBasis, ExpandedSceneDocument, ExpandedSceneEntity,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceBinding, ResolvedPrefabSource, SceneAuthoringSession,
    SceneCompilationFailureKind, SceneCompilationLimits, SceneCompilationOptions,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport, ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("content/captured-extraction/{name}.json")).unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "a076a6ec-f5c1-4a13-85d5-d670a9ee29be".parse().unwrap(),
        AssetSourcePath::new(format!("content/captured-extraction/{name}.json")).unwrap(),
        kind,
    )
}

fn metadata(name: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(format!("content/captured-extraction/{name}.json")).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("capture.Link")
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Capture link", ComponentSchemaVersion::ONE)
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
        PersistentComponentSchemaOwnerId::try_new("capture.schema").unwrap(),
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

fn empty_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn open_scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(path("scene")), basis)
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn open_prefab(document: &PrefabDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(path("p")), basis)
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

fn link(target: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(target) }),
        )]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("capture.schema").unwrap())
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
    scene_key: SourceDocumentKey,
    p_key: SourceDocumentKey,
    q_key: SourceDocumentKey,
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
                SceneEntityRecord::new(id("b")).with_component(component(), link("x")),
                SceneEntityRecord::new(id("x")).with_component(component(), link("b")),
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
            name: "capture-fixture".into(),
            sources: vec![0, 1],
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
        let scene = SceneDocument::from_ordered_roots([left, right]);
        Self {
            scene: open_scene(&scene, &basis),
            prefab: open_prefab(&prefab, &basis),
            basis,
            scene_key: key("scene", SourceDocumentKind::Scene),
            p_key: key("p", SourceDocumentKind::Prefab),
            q_key: key("q", SourceDocumentKind::Prefab),
            p_metadata,
            q_metadata,
            environment,
        }
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

fn capture(
    fixture: &Fixture,
    limits: SceneCompilationLimits,
    root: &str,
) -> (
    PrefabExtractionRequest,
    PrefabExtractionWork,
    SceneSourceExtractionInput,
    PrefabSourceExtractionInput,
) {
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let scene = fixture
        .scene
        .capture_prefab_extraction_input(&mut work)
        .unwrap();
    let prefab = fixture
        .prefab
        .capture_prefab_extraction_input(&mut work)
        .unwrap();

    // This caller owns the finite request/environment tables, cloned keys and destination metadata.
    // Admission precedes their allocation; the source bodies were charged by Session capture.
    let wrapper_bytes = size_of::<PrefabExtractionRequest>()
        + 2 * size_of::<CapturedExtractionSource>()
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
        + fixture.q_metadata.path().as_str().len()
        + root.len();
    work.admit_capture_storage(Some(wrapper_bytes)).unwrap();
    let source = ResolvedPrefabSource::bound_authoring_source(
        prefab.shared_document(),
        prefab.schema_basis(),
        fixture.p_key.clone(),
    )
    .with_source_asset(&fixture.p_metadata)
    .unwrap();
    let request = PrefabExtractionRequest::new(
        vec![
            CapturedExtractionSource::Scene {
                key: fixture.scene_key.clone(),
                document: scene.shared_document(),
                basis: scene.schema_basis(),
            },
            CapturedExtractionSource::Prefab { source },
        ],
        vec![fixture.request_environment()],
        0,
        fixture.basis.clone(),
        fixture.p_key.clone(),
        id(root),
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
    (request, work, scene, prefab)
}

fn worker(
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
) -> Result<PreparedPrefabExtraction, PrefabExtractionFailure> {
    std::thread::spawn(move || request.prepare(work))
        .join()
        .expect("the actual captured request must cross a normal worker thread")
}

fn scene_source(source: &CapturedExtractionSource) -> &Arc<SceneDocument> {
    let CapturedExtractionSource::Scene { document, .. } = source else {
        panic!("expected the original Scene corpus member");
    };
    document
}

fn prefab_source(source: &CapturedExtractionSource) -> &ResolvedPrefabSource {
    let CapturedExtractionSource::Prefab { source, .. } = source else {
        panic!("expected the original Prefab corpus member");
    };
    source
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
    sources: &[CapturedExtractionSource],
    bindings: &[PrefabExtractionBinding],
    extracted: Option<&ResolvedPrefabSource>,
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
                    sources
                        .iter()
                        .find_map(|source| match source {
                            CapturedExtractionSource::Prefab { source }
                                if source.key() == &binding.source =>
                            {
                                Some(source)
                            }
                            _ => None,
                        })
                        .or_else(|| extracted.filter(|source| source.key() == &binding.source))
                });
            Ok::<_, Infallible>(source.cloned())
        },
    )
    .unwrap();
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    assert!(report.failure().is_none(), "{:?}", report.failure());
    // Data authoring retains complete values even when no Native preview binding is installed.
    assert!(
        report.omissions().iter().all(|omission| {
            omission.component == component()
                && omission.reason
                    == AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
        }),
        "{:?}",
        report.omissions(),
    );
    report
        .into_parts()
        .candidate
        .expect("the real authoring compiler must succeed")
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
        .expect("each occurrence must retain its actual source entity");
    document
        .entities()
        .iter()
        .find(|record| record.id() == entity)
        .unwrap()
}

#[test]
fn worker_retains_original_captures_and_prepares_real_p_q_sources_and_input_mapping() {
    let fixture = Fixture::new();
    let scene_history = fixture.scene.history_status();
    let prefab_history = fixture.prefab.history_status();
    let (request, work, scene, prefab) = capture(&fixture, Default::default(), "k");
    let captured_usage = work.usage();
    assert!(Arc::ptr_eq(
        &scene.shared_document(),
        &fixture.scene.shared_document()
    ));
    assert!(Arc::ptr_eq(
        &prefab.shared_document(),
        &fixture.prefab.shared_document()
    ));
    assert_eq!(scene.schema_basis(), prefab.schema_basis());
    assert_eq!(
        request.environments()[0]
            .before
            .iter()
            .map(|binding| (&binding.reference, &binding.source))
            .collect::<Vec<_>>(),
        [
            (&path("p"), &fixture.p_key),
            (
                &AssetRef::StableId(fixture.p_metadata.stable_id()),
                &fixture.p_key,
            ),
        ]
    );
    let before = compile(
        scene.document(),
        &fixture.basis,
        request.sources(),
        &request.environments()[0].before,
        None,
    );

    let mut prepared = worker(request, work).unwrap();
    assert!(prepared.usage().work_bytes > captured_usage.work_bytes);
    assert!(prepared.usage().work_nodes > captured_usage.work_nodes);
    assert_eq!(prepared.schema_basis(), &fixture.basis);
    assert_eq!(prepared.original_sources().len(), 2);
    assert_eq!(prepared.sources().len(), 2);
    assert!(Arc::ptr_eq(
        scene_source(&prepared.original_sources()[0]),
        &scene.shared_document()
    ));
    assert!(std::ptr::eq(
        prefab_source(&prepared.original_sources()[1]).document(),
        prefab.document(),
    ));
    prepared.require_scene_input(0, &scene).unwrap();
    prepared.require_prefab_input(1, &prefab).unwrap();
    prepared
        .validate_scene_input(0, &scene, &fixture.scene)
        .unwrap();
    prepared
        .validate_prefab_input(1, &prefab, &fixture.prefab)
        .unwrap();

    let p = prefab_source(&prepared.sources()[1]);
    let q = prepared.extracted();
    assert_eq!(p.key(), &fixture.p_key);
    assert_eq!(q.key(), &fixture.q_key);
    assert_eq!(p.document().inputs, prefab.document().inputs);
    assert_eq!(
        p.document()
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["k", "x"]
    );
    assert_eq!(
        q.document()
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"]
    );
    let anchor = &p.document().entities[0];
    let instance = anchor
        .prefab
        .as_ref()
        .expect("P:k must actually instantiate Q");
    let q_reference = AssetRef::StableId(fixture.q_metadata.stable_id());
    assert_eq!(instance.source, q_reference);
    assert_eq!(
        instance.binding,
        PrefabInstanceBinding::Root {
            base: fixture.q_key.clone(),
            root: PrefabEntityTarget::source(id("k")),
        }
    );
    let arguments = instance
        .arguments
        .as_ref()
        .expect("Q:b still refers to P:x");
    assert_eq!(arguments.base, fixture.q_key);
    assert_eq!(arguments.values.len(), 1);
    assert_eq!(
        q.document().inputs,
        arguments.values.keys().cloned().collect::<BTreeSet<_>>()
    );
    let (input, target) = arguments.values.first_key_value().unwrap();
    assert_eq!(target, &EntityReference::SceneLocal { entity: id("x") });
    let moved = &q.document().entities[0].components[&component()];
    let Some(ComponentValue::EntityReference(EntityReference::Authored { reference })) =
        moved.value.get("target")
    else {
        panic!("the moved cross-cut reference must use an actual declared Q input");
    };
    assert!(reference.through.is_empty());
    assert_eq!(
        reference.target,
        AuthoredReferenceTarget::Input(input.clone())
    );

    let after = compile(
        scene_source(&prepared.sources()[0]),
        prepared.schema_basis(),
        prepared.sources(),
        &fixture.environment.after,
        Some(q),
    );
    let mut stable_scene = scene_source(&prepared.sources()[0]).as_ref().clone();
    for entity in &mut stable_scene.entities {
        if let Some(instance) = &mut entity.prefab {
            instance.source = AssetRef::StableId(fixture.p_metadata.stable_id());
        }
    }
    let stable_after = compile(
        &stable_scene,
        prepared.schema_basis(),
        prepared.sources(),
        &fixture.environment.after,
        Some(q),
    );
    assert_eq!(before.roots(), after.roots());
    assert_eq!(after.roots(), &[id("left"), id("right")]);
    assert_eq!(before.entities().len(), 8);
    assert_eq!(after.entities().len(), before.entities().len());
    assert_eq!(stable_after.roots(), after.roots());
    assert_eq!(stable_after.entities().len(), after.entities().len());
    for document in [&before, &after, &stable_after] {
        for anchor in ["left", "right"] {
            let k = projected(document, anchor, "k");
            let b = projected(document, anchor, "b");
            let x = projected(document, anchor, "x");
            let host = document
                .entities()
                .iter()
                .find(|entity| entity.id() == &id(anchor))
                .unwrap();
            assert_eq!(host.children(), &[k.id().clone(), x.id().clone()]);
            assert_eq!(k.children(), std::slice::from_ref(b.id()));
            assert!(b.children().is_empty());
            assert!(x.children().is_empty());
            for (record, target) in [(b, x), (x, b)] {
                assert_eq!(
                    record.components()[&component()].value.get("target"),
                    Some(&ComponentValue::EntityReference(
                        EntityReference::SceneLocal {
                            entity: target.id().clone()
                        }
                    ),)
                );
            }
        }
    }
    assert_ne!(
        projected(&after, "left", "b").id(),
        projected(&after, "right", "b").id()
    );
    assert_eq!(fixture.scene.history_status(), scene_history);
    assert_eq!(fixture.prefab.history_status(), prefab_history);
    assert_eq!(fixture.scene.revision(), scene.revision());
    assert_eq!(fixture.prefab.revision(), prefab.revision());
    assert!(Arc::ptr_eq(
        &fixture.scene.shared_document(),
        &scene.shared_document()
    ));
    assert!(Arc::ptr_eq(
        &fixture.prefab.shared_document(),
        &prefab.shared_document()
    ));
}

#[test]
fn equal_source_allocations_captured_under_foreign_work_do_not_match_prepared_inputs() {
    let fixture = Fixture::new();
    let (request, work, scene, prefab) = capture(&fixture, Default::default(), "k");
    let mut foreign_work = PrefabExtractionWork::new(Default::default()).unwrap();
    let foreign_scene = fixture
        .scene
        .capture_prefab_extraction_input(&mut foreign_work)
        .unwrap();
    let foreign_prefab = fixture
        .prefab
        .capture_prefab_extraction_input(&mut foreign_work)
        .unwrap();
    assert!(Arc::ptr_eq(
        &scene.shared_document(),
        &foreign_scene.shared_document()
    ));
    assert!(Arc::ptr_eq(
        &prefab.shared_document(),
        &foreign_prefab.shared_document()
    ));
    assert_eq!(scene.schema_basis(), foreign_scene.schema_basis());
    assert_eq!(prefab.schema_basis(), foreign_prefab.schema_basis());
    let mut prepared = worker(request, work).unwrap();
    for report in [
        prepared.require_scene_input(0, &foreign_scene).unwrap_err(),
        prepared
            .require_prefab_input(1, &foreign_prefab)
            .unwrap_err(),
        prepared.require_scene_input(1, &scene).unwrap_err(),
        prepared.require_prefab_input(0, &prefab).unwrap_err(),
        prepared.require_scene_input(2, &scene).unwrap_err(),
        prepared
            .validate_scene_input(0, &foreign_scene, &fixture.scene)
            .unwrap_err(),
        prepared
            .validate_prefab_input(1, &foreign_prefab, &fixture.prefab)
            .unwrap_err(),
    ] {
        assert_code(&report, "scene.prefab-extraction-stale");
    }
    prepared.require_scene_input(0, &scene).unwrap();
    prepared.require_prefab_input(1, &prefab).unwrap();
    assert_code(
        &fixture
            .scene
            .validate_prefab_extraction_input(&scene, &mut foreign_work)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
    assert_code(
        &fixture
            .prefab
            .validate_prefab_extraction_input(&prefab, &mut foreign_work)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
}

#[test]
fn refusal_returns_the_same_request_allocations_and_the_original_spent_work() {
    for unknown_schema in [false, true] {
        let mut fixture = Fixture::new();
        if unknown_schema {
            let mut document = fixture.prefab.document().clone();
            document
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("b"))
                .unwrap()
                .components
                .insert(
                    ComponentTypeId::new("capture.Unknown"),
                    SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7)),
                );
            fixture.prefab = open_prefab(&document, &fixture.basis);
        }
        let root = if unknown_schema { "k" } else { "missing-root" };
        let (request, work, scene, prefab) = capture(&fixture, Default::default(), root);
        let table = request.sources().as_ptr();
        let environments = request.environments().as_ptr();
        let before_allocation = request.environments()[0].before.as_ptr();
        let after_allocation = request.environments()[0].after.as_ptr();
        let name_allocation = request.environments()[0].name.as_ptr();
        let captured_usage = work.usage();
        let failure = worker(request, work).unwrap_err();
        if unknown_schema {
            assert_eq!(failure.environment_index(), Some(0));
            assert_eq!(failure.environment_name(), Some("capture-fixture"));
            assert_eq!(
                failure.environment_name().unwrap().as_ptr(),
                name_allocation
            );
            assert_code(failure.report(), "scene.prefab-extraction-unknown-schema");
        } else {
            assert_eq!(failure.environment_index(), Some(0));
            assert_eq!(failure.environment_name(), Some("capture-fixture"));
            assert_eq!(
                failure.environment_name().unwrap().as_ptr(),
                name_allocation
            );
            assert_eq!(
                failure
                    .compilation_failure()
                    .map(|failure| *failure.reason()),
                Some(SceneCompilationFailureKind::MissingTarget)
            );
            assert_code(failure.report(), "scene.prefab-compilation-failed");
        }
        let (request, mut spent_work, report) = failure.into_parts();
        assert!(report.diagnostics.has_errors());
        assert_eq!(request.sources().as_ptr(), table);
        assert_eq!(request.environments().as_ptr(), environments);
        assert_eq!(request.environments()[0].before.as_ptr(), before_allocation);
        assert_eq!(request.environments()[0].after.as_ptr(), after_allocation);
        assert_eq!(request.environments()[0].name.as_ptr(), name_allocation);
        assert!(Arc::ptr_eq(
            scene_source(&request.sources()[0]),
            &scene.shared_document()
        ));
        assert!(std::ptr::eq(
            prefab_source(&request.sources()[1]).document(),
            prefab.document()
        ));
        assert!(spent_work.usage().work_bytes > captured_usage.work_bytes);
        assert!(spent_work.usage().work_nodes > captured_usage.work_nodes);
        fixture
            .scene
            .validate_prefab_extraction_input(&scene, &mut spent_work)
            .unwrap();
        fixture
            .prefab
            .validate_prefab_extraction_input(&prefab, &mut spent_work)
            .unwrap();
    }
}

#[test]
fn one_byte_boundary_covers_capture_wrappers_validation_and_full_preparation() {
    let fixture = Fixture::new();
    let (request, work, _, _) = capture(&fixture, Default::default(), "k");
    let captured_bytes = work.usage().work_bytes;
    let measured = worker(request, work).unwrap().usage();
    assert!(measured.work_bytes > captured_bytes);
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(measured.work_bytes).unwrap());
    let fixture = Fixture::new();
    let (request, work, _, _) = capture(&fixture, exact, "k");
    assert_eq!(worker(request, work).unwrap().usage(), measured);

    let one_short = exact.with_working_bytes(ByteLimit::new(measured.work_bytes - 1).unwrap());
    let fixture = Fixture::new();
    let (request, work, scene, prefab) = capture(&fixture, one_short, "k");
    let failure = worker(request, work).unwrap_err();
    assert_code(failure.report(), "scene.prefab-extraction-capacity");
    let (request, spent_work, _) = failure.into_parts();
    assert!(spent_work.usage().work_bytes > captured_bytes);
    assert!(Arc::ptr_eq(
        scene_source(&request.sources()[0]),
        &scene.shared_document()
    ));
    assert!(std::ptr::eq(
        prefab_source(&request.sources()[1]).document(),
        prefab.document()
    ));
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

#[test]
fn actual_undo_and_redo_invalidate_captures_while_retaining_both_history_branches() {
    let mut fixture = Fixture::new();
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
    }
    assert!(fixture.scene.undo_authoring().applied);
    assert!(fixture.prefab.undo_authoring().applied);
    assert_eq!(fixture.scene.history_status().undo_depth, 1);
    assert_eq!(fixture.scene.history_status().redo_depth, 1);
    assert_eq!(fixture.prefab.history_status().undo_depth, 1);
    assert_eq!(fixture.prefab.history_status().redo_depth, 1);
    let (request, work, scene, prefab) = capture(&fixture, Default::default(), "k");
    assert_eq!(scene.source(), fixture.scene.authoring_source().unwrap());
    assert_eq!(prefab.source(), fixture.prefab.authoring_source().unwrap());
    assert_eq!(scene.content_state(), fixture.scene.content_state());
    assert_eq!(prefab.content_state(), fixture.prefab.content_state());
    assert_eq!(scene.file_limits(), prefab.file_limits());
    assert_eq!(scene.history_limits(), prefab.history_limits());
    assert_eq!(fixture.scene.history_status().redo_depth, 1);
    assert_eq!(fixture.prefab.history_status().redo_depth, 1);
    let mut prepared = worker(request, work).unwrap();
    let prepared_usage = prepared.usage();
    prepared
        .validate_scene_input(0, &scene, &fixture.scene)
        .unwrap();
    prepared
        .validate_prefab_input(1, &prefab, &fixture.prefab)
        .unwrap();
    assert!(prepared.usage().work_bytes > prepared_usage.work_bytes);
    assert!(prepared.usage().work_nodes > prepared_usage.work_nodes);

    assert!(fixture.scene.undo_authoring().applied);
    assert!(fixture.prefab.undo_authoring().applied);
    // Matching retained originals still succeeds; exact live validation observes the real Undo.
    prepared.require_scene_input(0, &scene).unwrap();
    prepared.require_prefab_input(1, &prefab).unwrap();
    assert_code(
        &prepared
            .validate_scene_input(0, &scene, &fixture.scene)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
    assert_code(
        &prepared
            .validate_prefab_input(1, &prefab, &fixture.prefab)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
    assert!(fixture.scene.redo_authoring().applied);
    assert!(fixture.prefab.redo_authoring().applied);
    assert_eq!(fixture.scene.document(), scene.document());
    assert_eq!(fixture.prefab.document(), prefab.document());
    assert_eq!(fixture.scene.content_state(), scene.content_state());
    assert_eq!(fixture.prefab.content_state(), prefab.content_state());
    assert_ne!(fixture.scene.revision(), scene.revision());
    assert_ne!(fixture.prefab.revision(), prefab.revision());
    assert_code(
        &prepared
            .validate_scene_input(0, &scene, &fixture.scene)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
    assert_code(
        &prepared
            .validate_prefab_input(1, &prefab, &fixture.prefab)
            .unwrap_err(),
        "scene.prefab-extraction-stale",
    );
}

#[test]
fn native_source_sessions_cannot_supply_published_extraction_inputs() {
    let fixture = Fixture::new();
    let scene = SceneAuthoringSession::new(fixture.scene.document().clone());
    let prefab = PrefabAuthoringSession::new(fixture.prefab.document().clone());
    let mut work = PrefabExtractionWork::new(Default::default()).unwrap();
    assert_code(
        &scene
            .capture_prefab_extraction_input(&mut work)
            .unwrap_err(),
        "scene.prefab-extraction-authority",
    );
    assert_code(
        &prefab
            .capture_prefab_extraction_input(&mut work)
            .unwrap_err(),
        "scene.prefab-extraction-authority",
    );
}

fn input_prefab(names: impl IntoIterator<Item = String>) -> PrefabAuthoringSession {
    let mut document = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))]);
    document.inputs = names
        .into_iter()
        .map(|name| ReferenceInputId::new(name).unwrap())
        .collect();
    open_prefab(&document, &empty_basis())
}

#[test]
fn prefab_input_retention_charges_actual_names_and_has_a_capture_byte_boundary() {
    let empty = input_prefab([]);
    let short = input_prefab(["a".into(), "b".into()]);
    let long_names = [
        format!("a{}", "x".repeat(100)),
        format!("b{}", "y".repeat(100)),
    ];
    let long = input_prefab(long_names.clone());
    let mut measured = Vec::new();
    for session in [&empty, &short, &long] {
        let mut work = PrefabExtractionWork::new(Default::default()).unwrap();
        let capture = session.capture_prefab_extraction_input(&mut work).unwrap();
        assert!(Arc::ptr_eq(
            &capture.shared_document(),
            &session.shared_document()
        ));
        assert_eq!(capture.document().inputs, session.document().inputs);
        measured.push(work.usage());
    }
    assert!(measured[1].work_bytes > measured[0].work_bytes);
    assert!(measured[2].work_bytes > measured[1].work_bytes);
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(measured[2].work_bytes).unwrap());
    let session = input_prefab(long_names.clone());
    let mut work = PrefabExtractionWork::new(exact).unwrap();
    let _capture = session.capture_prefab_extraction_input(&mut work).unwrap();
    assert_eq!(work.usage(), measured[2]);
    let session = input_prefab(long_names);
    let original = session.shared_document();
    let revision = session.revision();
    let one_short = exact.with_working_bytes(ByteLimit::new(measured[2].work_bytes - 1).unwrap());
    let mut work = PrefabExtractionWork::new(one_short).unwrap();
    assert_code(
        &session
            .capture_prefab_extraction_input(&mut work)
            .unwrap_err(),
        "scene.prefab-extraction-capacity",
    );
    assert!(Arc::ptr_eq(&original, &session.shared_document()));
    assert_eq!(session.revision(), revision);
}

#[test]
fn large_prefab_input_set_refuses_capture_with_ample_bytes_and_sixteen_work_nodes() {
    let limits =
        SceneCompilationLimits::default().with_reference_work_nodes(ItemLimit::new(16).unwrap());
    let empty = input_prefab([]);
    let mut empty_work = PrefabExtractionWork::new(limits).unwrap();
    let _empty_capture = empty
        .capture_prefab_extraction_input(&mut empty_work)
        .unwrap();

    let session = input_prefab((0..256).map(|index| format!("declared-input-{index:03}")));
    let original = session.shared_document();
    let revision = session.revision();
    let content = session.content_state();
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let report = session
        .capture_prefab_extraction_input(&mut work)
        .unwrap_err();
    assert_code(&report, "scene.prefab-extraction-capacity");
    assert!(work.usage().work_nodes <= 16);
    assert!(work.usage().work_bytes < limits.working_bytes().get());
    assert!(Arc::ptr_eq(&original, &session.shared_document()));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.document().inputs.len(), 256);

    let mut ample_work = PrefabExtractionWork::new(Default::default()).unwrap();
    let capture = session
        .capture_prefab_extraction_input(&mut ample_work)
        .unwrap();
    assert_eq!(capture.document().inputs, original.inputs);
    session
        .validate_prefab_extraction_input(&capture, &mut ample_work)
        .unwrap();
}
