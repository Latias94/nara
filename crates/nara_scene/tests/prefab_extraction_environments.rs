//! Independent Core consumers of one physical extraction corpus in multiple environments.
//! Catalogue capture, all-profile inventory completeness and file publication belong to the Host.

#![cfg(feature = "serde")]

use std::{collections::BTreeSet, convert::Infallible, mem::size_of, sync::Arc};

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
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
    SceneCompilationLimits, SceneCompilationOptions, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    ScenePatchReport, ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("content/extraction-environments/{name}.json")).unwrap()
}

fn actor_reference() -> AssetRef {
    AssetRef::path("actor.prefab.json").unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "a076a6ec-f5c1-4a13-85d5-d670a9ee29be".parse().unwrap(),
        AssetSourcePath::new(format!("content/extraction-environments/{name}.json")).unwrap(),
        kind,
    )
}

fn metadata(name: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(format!("content/extraction-environments/{name}.json")).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn sentinel_type() -> ComponentTypeId {
    ComponentTypeId::new("environments.Sentinel")
}

fn unknown_type() -> ComponentTypeId {
    ComponentTypeId::new("missing.EnvironmentPayload")
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        sentinel_type(),
        "Environment sentinel",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("environments.schema").unwrap(),
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

fn sentinel(value: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("value", ComponentValue::U64(value))]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("environments.schema").unwrap())
}

fn unknown_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "opaque",
            ComponentValue::String("retain the actual unavailable source payload".into()),
        )]),
    )
}

fn child(entity: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(entity)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn prefab_document(base: u64) -> PrefabDocument {
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("trunk")),
            child("branch", "trunk"),
            child("leaf", "branch"),
            SceneEntityPlacement::root(id("outside")),
        ],
        ["trunk", "branch", "leaf", "outside"]
            .into_iter()
            .enumerate()
            .map(|(offset, name)| {
                SceneEntityRecord::new(id(name))
                    .with_component(sentinel_type(), sentinel(base + offset as u64))
            }),
    )
}

fn open_scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(path("shared.scene")), basis)
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn open_prefab(
    document: &PrefabDocument,
    name: &str,
    basis: &AuthoringSchemaBasis,
) -> PrefabAuthoringSession {
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(path(name)), basis)
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

fn binding(reference: AssetRef, source: &SourceDocumentKey) -> PrefabExtractionBinding {
    PrefabExtractionBinding {
        reference,
        source: source.clone(),
    }
}

fn copy_bindings(bindings: &[PrefabExtractionBinding]) -> Vec<PrefabExtractionBinding> {
    bindings
        .iter()
        .map(|entry| binding(entry.reference.clone(), &entry.source))
        .collect()
}

fn copy_environment(environment: &PrefabExtractionEnvironment) -> PrefabExtractionEnvironment {
    assert!(environment.assets_before.is_none());
    assert!(environment.assets_after.is_none());
    PrefabExtractionEnvironment {
        name: environment.name.clone(),
        sources: environment.sources.clone(),
        before: copy_bindings(&environment.before),
        after: copy_bindings(&environment.after),
        assets_before: None,
        assets_after: None,
    }
}

struct Fixture {
    basis: AuthoringSchemaBasis,
    scene: SceneAuthoringSession,
    p: PrefabAuthoringSession,
    r: PrefabAuthoringSession,
    scene_key: SourceDocumentKey,
    p_key: SourceDocumentKey,
    r_key: SourceDocumentKey,
    q_key: SourceDocumentKey,
    p_metadata: AssetRecord,
    r_metadata: AssetRecord,
    q_metadata: AssetRecord,
}

impl Fixture {
    fn new(unknown_scene: bool) -> Self {
        let basis = basis();
        let mut actor = SceneEntityRecord::new(id("actor"));
        actor.prefab = Some(PrefabInstance::forest(actor_reference()));
        if unknown_scene {
            actor.components.insert(unknown_type(), unknown_record());
        }
        Self {
            scene: open_scene(&SceneDocument::from_ordered_roots([actor]), &basis),
            p: open_prefab(&prefab_document(100), "p.prefab", &basis),
            r: open_prefab(&prefab_document(200), "r.prefab", &basis),
            basis,
            scene_key: key("shared.scene", SourceDocumentKind::Scene),
            p_key: key("p.prefab", SourceDocumentKind::Prefab),
            r_key: key("r.prefab", SourceDocumentKind::Prefab),
            q_key: key("q.prefab", SourceDocumentKind::Prefab),
            p_metadata: metadata("p.prefab", "c5f566b8-930e-4774-b138-12ca69445f91"),
            r_metadata: metadata("r.prefab", "912e852a-5ac4-4c27-abf9-87dd895d9984"),
            q_metadata: metadata("q.prefab", "7688b7ac-6ed4-4814-b78a-0e7c7a68b454"),
        }
    }

    fn q_reference(&self) -> AssetRef {
        AssetRef::StableId(self.q_metadata.stable_id())
    }

    fn environments(&self) -> [PrefabExtractionEnvironment; 2] {
        [
            PrefabExtractionEnvironment {
                name: "desktop".into(),
                sources: vec![0, 1],
                before: vec![binding(actor_reference(), &self.p_key)],
                after: vec![
                    binding(actor_reference(), &self.p_key),
                    binding(self.q_reference(), &self.q_key),
                ],
                assets_before: None,
                assets_after: None,
            },
            PrefabExtractionEnvironment {
                name: "mobile".into(),
                sources: vec![0, 2],
                before: vec![binding(actor_reference(), &self.r_key)],
                after: vec![binding(actor_reference(), &self.r_key)],
                assets_before: None,
                assets_after: None,
            },
        ]
    }
}

struct Inputs {
    scene: SceneSourceExtractionInput,
    p: PrefabSourceExtractionInput,
    r: PrefabSourceExtractionInput,
}

impl Inputs {
    fn assert_sources(&self, sources: &[CapturedExtractionSource], fixture: &Fixture) {
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].key(), &fixture.scene_key);
        assert_eq!(sources[1].key(), &fixture.p_key);
        assert_eq!(sources[2].key(), &fixture.r_key);
        let CapturedExtractionSource::Scene {
            document, basis, ..
        } = &sources[0]
        else {
            panic!("the shared physical Scene must remain the first source");
        };
        assert!(Arc::ptr_eq(document, &self.scene.shared_document()));
        assert_eq!(basis, &fixture.basis);
        for (source, input) in [(&sources[1], &self.p), (&sources[2], &self.r)] {
            assert!(std::ptr::eq(
                prefab_source(source).document(),
                input.document(),
            ));
            assert_eq!(input.schema_basis(), fixture.basis);
        }
    }

    fn assert_live(&self, fixture: &Fixture) {
        assert!(Arc::ptr_eq(
            &fixture.scene.shared_document(),
            &self.scene.shared_document(),
        ));
        assert_eq!(fixture.scene.revision(), self.scene.revision());
        assert_eq!(fixture.scene.content_state(), self.scene.content_state());
        for (session, input) in [(&fixture.p, &self.p), (&fixture.r, &self.r)] {
            assert!(Arc::ptr_eq(
                &session.shared_document(),
                &input.shared_document(),
            ));
            assert_eq!(session.revision(), input.revision());
            assert_eq!(session.content_state(), input.content_state());
        }
    }

    fn validate(&self, fixture: &Fixture, work: &mut PrefabExtractionWork) {
        fixture
            .scene
            .validate_prefab_extraction_input(&self.scene, work)
            .unwrap();
        fixture
            .p
            .validate_prefab_extraction_input(&self.p, work)
            .unwrap();
        fixture
            .r
            .validate_prefab_extraction_input(&self.r, work)
            .unwrap();
    }
}

fn capture(
    fixture: &Fixture,
    environments: &[PrefabExtractionEnvironment],
    selected_environment: usize,
    limits: SceneCompilationLimits,
) -> (PrefabExtractionRequest, PrefabExtractionWork, Inputs) {
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let inputs = Inputs {
        scene: fixture
            .scene
            .capture_prefab_extraction_input(&mut work)
            .unwrap(),
        p: fixture
            .p
            .capture_prefab_extraction_input(&mut work)
            .unwrap(),
        r: fixture
            .r
            .capture_prefab_extraction_input(&mut work)
            .unwrap(),
    };
    // Session capture admitted the actual bodies/classifications/history. Admit our finite
    // source/environment tables, keys, names and metadata before allocating request copies.
    let wrapper_bytes = size_of::<PrefabExtractionRequest>()
        + size_of::<Inputs>()
        + 3 * size_of::<CapturedExtractionSource>()
        + environments.len() * size_of::<PrefabExtractionEnvironment>()
        + environments
            .iter()
            .map(|environment| {
                environment.name.len()
                    + environment.sources.len() * size_of::<usize>()
                    + (environment.before.len() + environment.after.len())
                        * size_of::<PrefabExtractionBinding>()
                    + environment
                        .before
                        .iter()
                        .chain(&environment.after)
                        .map(|entry| {
                            entry.source.checked_retained_bytes().unwrap()
                                + entry
                                    .reference
                                    .as_path()
                                    .map_or(0, |path| path.as_str().len())
                        })
                        .sum::<usize>()
            })
            .sum::<usize>()
        + fixture.scene_key.checked_retained_bytes().unwrap()
        + 2 * fixture.p_key.checked_retained_bytes().unwrap()
        + fixture.r_key.checked_retained_bytes().unwrap()
        + fixture.q_key.checked_retained_bytes().unwrap()
        + fixture.q_metadata.path().as_str().len()
        + "branch".len();
    work.admit_capture_storage(Some(wrapper_bytes)).unwrap();
    let p = ResolvedPrefabSource::bound_authoring_source(
        inputs.p.shared_document(),
        inputs.p.schema_basis(),
        fixture.p_key.clone(),
    )
    .with_source_asset(&fixture.p_metadata)
    .unwrap();
    let r = ResolvedPrefabSource::bound_authoring_source(
        inputs.r.shared_document(),
        inputs.r.schema_basis(),
        fixture.r_key.clone(),
    )
    .with_source_asset(&fixture.r_metadata)
    .unwrap();
    let request = PrefabExtractionRequest::new(
        vec![
            CapturedExtractionSource::Scene {
                key: fixture.scene_key.clone(),
                document: inputs.scene.shared_document(),
                basis: inputs.scene.schema_basis(),
            },
            CapturedExtractionSource::Prefab { source: p },
            CapturedExtractionSource::Prefab { source: r },
        ],
        environments.iter().map(copy_environment).collect(),
        selected_environment,
        fixture.basis.clone(),
        fixture.p_key.clone(),
        id("branch"),
        PrefabExtractionDestination {
            key: fixture.q_key.clone(),
            reference: fixture.q_reference(),
            metadata: fixture.q_metadata.clone(),
        },
    )
    .with_max_depth(16);
    inputs.validate(fixture, &mut work);
    (request, work, inputs)
}

fn prepare(
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
) -> Result<PreparedPrefabExtraction, PrefabExtractionFailure> {
    std::thread::spawn(move || request.prepare(work))
        .join()
        .expect("the actual immutable inputs and original Work must cross a worker thread")
}

fn scene_source(source: &CapturedExtractionSource) -> &Arc<SceneDocument> {
    let CapturedExtractionSource::Scene { document, .. } = source else {
        panic!("expected the shared physical Scene");
    };
    document
}

fn prefab_source(source: &CapturedExtractionSource) -> &ResolvedPrefabSource {
    let CapturedExtractionSource::Prefab { source } = source else {
        panic!("expected a physical Prefab source");
    };
    source
}

#[derive(Debug, PartialEq, Eq)]
struct RequestAllocations {
    sources: usize,
    environments: usize,
    tables: Vec<[usize; 4]>,
}

fn allocations(request: &PrefabExtractionRequest) -> RequestAllocations {
    RequestAllocations {
        sources: request.sources().as_ptr() as usize,
        environments: request.environments().as_ptr() as usize,
        tables: request
            .environments()
            .iter()
            .map(|environment| {
                [
                    environment.name.as_ptr() as usize,
                    environment.sources.as_ptr() as usize,
                    environment.before.as_ptr() as usize,
                    environment.after.as_ptr() as usize,
                ]
            })
            .collect(),
    }
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
    environment: &PrefabExtractionEnvironment,
    extracted: Option<&ResolvedPrefabSource>,
) -> ExpandedSceneDocument {
    let bindings = if extracted.is_some() {
        &environment.after
    } else {
        &environment.before
    };
    let report = try_compile_authoring_scene_with_loader(
        document,
        basis,
        SceneCompilationOptions::default(),
        None,
        |reference, _| {
            let resolved = bindings
                .iter()
                .find(|entry| &entry.reference == reference)
                .and_then(|entry| {
                    environment
                        .sources
                        .iter()
                        .find_map(|&index| match &sources[index] {
                            CapturedExtractionSource::Prefab { source }
                                if source.key() == &entry.source =>
                            {
                                Some(source)
                            }
                            _ => None,
                        })
                        .or_else(|| extracted.filter(|source| source.key() == &entry.source))
                });
            Ok::<_, Infallible>(resolved.cloned())
        },
    )
    .unwrap();
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics(),
    );
    assert!(report.failure().is_none(), "{:?}", report.failure());
    assert!(
        report
            .omissions()
            .iter()
            .all(|omission| match omission.reason {
                AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable => {
                    omission.component == sentinel_type()
                }
                AuthoringSceneCompilationOmissionReason::RecordUnavailable(_) => {
                    omission.component == unknown_type()
                }
            }),
        "{:?}",
        report.omissions(),
    );
    report
        .into_parts()
        .candidate
        .expect("the real authoring compiler must succeed")
}

fn projected<'a>(document: &'a ExpandedSceneDocument, local: &str) -> &'a ExpandedSceneEntity {
    let (entity, _) = document
        .provenance()
        .iter()
        .find(|(_, origin)| {
            origin.local_entity_id().as_str() == local
                && origin
                    .projection_path()
                    .is_some_and(|path| path.root_anchor().as_str() == "actor")
        })
        .expect("the compiler must retain the actual projected source entity");
    document
        .entities()
        .iter()
        .find(|record| record.id() == entity)
        .unwrap()
}

fn assert_projection(document: &ExpandedSceneDocument, base: u64) {
    assert_eq!(document.roots(), &[id("actor")]);
    assert_eq!(document.entities().len(), 5);
    let actor = document
        .entities()
        .iter()
        .find(|record| record.id() == &id("actor"))
        .unwrap();
    let trunk = projected(document, "trunk");
    let branch = projected(document, "branch");
    let leaf = projected(document, "leaf");
    let outside = projected(document, "outside");
    assert_eq!(
        actor.children(),
        &[trunk.id().clone(), outside.id().clone()]
    );
    assert_eq!(trunk.children(), std::slice::from_ref(branch.id()));
    assert_eq!(branch.children(), std::slice::from_ref(leaf.id()));
    assert!(leaf.children().is_empty());
    assert!(outside.children().is_empty());
    for (offset, entity) in [trunk, branch, leaf, outside].into_iter().enumerate() {
        assert_eq!(
            entity.components()[&sentinel_type()].value.get("value"),
            Some(&ComponentValue::U64(base + offset as u64)),
        );
    }
}

#[test]
fn one_shared_scene_resolves_the_same_alias_to_distinct_prefabs_and_keeps_one_physical_output() {
    let fixture = Fixture::new(false);
    let environments = fixture.environments();
    let history = (
        fixture.scene.history_status(),
        fixture.p.history_status(),
        fixture.r.history_status(),
    );
    let (request, work, inputs) = capture(&fixture, &environments, 0, Default::default());
    inputs.assert_sources(request.sources(), &fixture);
    assert_eq!(inputs.scene.schema_basis(), fixture.basis);
    assert_eq!(request.environments()[0].sources, [0, 1]);
    assert_eq!(request.environments()[1].sources, [0, 2]);
    assert_eq!(
        request.environments()[0].before[0].reference,
        actor_reference(),
    );
    assert_eq!(
        request.environments()[1].before[0].reference,
        actor_reference(),
    );
    assert_eq!(request.environments()[0].before[0].source, fixture.p_key);
    assert_eq!(request.environments()[1].before[0].source, fixture.r_key);
    assert!(
        request.environments()[1]
            .after
            .iter()
            .all(|entry| entry.source != fixture.q_key),
    );
    let originals = request.sources().as_ptr();
    let captured_usage = work.usage();
    let before = environments.each_ref().map(|environment| {
        compile(
            inputs.scene.document(),
            &fixture.basis,
            request.sources(),
            environment,
            None,
        )
    });
    assert_projection(&before[0], 100);
    assert_projection(&before[1], 200);

    let mut prepared = prepare(request, work).unwrap();
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    inputs.assert_sources(prepared.original_sources(), &fixture);
    assert!(prepared.usage().work_bytes > captured_usage.work_bytes);
    assert!(prepared.usage().work_nodes > captured_usage.work_nodes);
    assert_eq!(prepared.schema_basis(), &fixture.basis);
    assert_eq!(prepared.selected_source(), &fixture.p_key);
    assert_eq!(prepared.root(), &id("branch"));
    assert_eq!(prepared.sources().len(), 3);
    assert_eq!(
        prepared
            .sources()
            .iter()
            .map(|source| source.key())
            .collect::<BTreeSet<_>>()
            .len(),
        3,
    );
    for (original, output) in prepared.original_sources().iter().zip(prepared.sources()) {
        assert_eq!(original.key(), output.key());
        assert_ne!(output.key(), &fixture.q_key);
    }
    assert_eq!(prepared.source_changed(0), Some(false));
    assert_eq!(prepared.source_changed(1), Some(true));
    assert_eq!(prepared.source_changed(2), Some(false));
    assert_eq!(prepared.source_changed(3), None);
    assert!(Arc::ptr_eq(
        scene_source(&prepared.sources()[0]),
        &inputs.scene.shared_document(),
    ));
    assert!(std::ptr::eq(
        prefab_source(&prepared.sources()[2]).document(),
        inputs.r.document(),
    ));
    assert!(!std::ptr::eq(
        prefab_source(&prepared.sources()[1]).document(),
        inputs.p.document(),
    ));
    let p = prefab_source(&prepared.sources()[1]);
    let q = prepared.extracted();
    assert_eq!(q.key(), &fixture.q_key);
    assert!(q.clone().with_source_asset(&fixture.q_metadata).is_ok());
    assert_eq!(p.document().entities.len(), 3);
    assert_eq!(q.document().entities.len(), 2);
    let branch = p
        .document()
        .entities
        .iter()
        .find(|record| record.id == id("branch"))
        .unwrap();
    assert!(branch.components.is_empty());
    let instance = branch
        .prefab
        .as_ref()
        .expect("P.branch must really instantiate unopened Q");
    assert_eq!(instance.source, fixture.q_reference());
    assert_eq!(
        instance.binding,
        PrefabInstanceBinding::Root {
            base: fixture.q_key.clone(),
            root: PrefabEntityTarget::source(id("branch")),
        },
    );
    for (index, base) in [(0, 100), (1, 200)] {
        let after = compile(
            scene_source(&prepared.sources()[0]),
            prepared.schema_basis(),
            prepared.sources(),
            &environments[index],
            Some(q),
        );
        assert_projection(&after, base);
        assert_eq!(before[index].roots(), after.roots());
        for local in ["trunk", "branch", "leaf", "outside"] {
            assert_eq!(
                projected(&before[index], local).components(),
                projected(&after, local).components(),
            );
        }
    }
    prepared
        .validate_scene_input(0, &inputs.scene, &fixture.scene)
        .unwrap();
    prepared
        .validate_prefab_input(1, &inputs.p, &fixture.p)
        .unwrap();
    prepared
        .validate_prefab_input(2, &inputs.r, &fixture.r)
        .unwrap();
    inputs.assert_live(&fixture);
    assert_eq!(
        (
            fixture.scene.history_status(),
            fixture.p.history_status(),
            fixture.r.history_status(),
        ),
        history,
    );
}

#[test]
fn a_duplicate_alias_within_mobile_refuses_and_retry_retains_every_allocation_and_original_work() {
    let fixture = Fixture::new(false);
    let mut environments = fixture.environments();
    environments[1]
        .before
        .push(binding(actor_reference(), &fixture.r_key));
    let (mut request, mut work, inputs) = capture(&fixture, &environments, 0, Default::default());
    let original_allocations = allocations(&request);
    let mut previous = work.usage();
    for _ in 0..2 {
        let failure = prepare(request, work).unwrap_err();
        assert_eq!(failure.environment_index(), Some(1));
        assert_eq!(failure.environment_name(), Some("mobile"));
        assert_eq!(
            failure.environment_name().unwrap().as_ptr() as usize,
            original_allocations.tables[1][0],
        );
        assert_code(failure.report(), "scene.prefab-extraction-invalid-source");
        let (returned_request, mut spent_work, report) = failure.into_parts();
        assert_code(&report, "scene.prefab-extraction-invalid-source");
        assert_eq!(allocations(&returned_request), original_allocations);
        inputs.assert_sources(returned_request.sources(), &fixture);
        assert!(spent_work.usage().work_bytes > previous.work_bytes);
        assert!(spent_work.usage().work_nodes > previous.work_nodes);
        inputs.validate(&fixture, &mut spent_work);
        inputs.assert_live(&fixture);
        previous = spent_work.usage();
        request = returned_request;
        work = spent_work;
    }
}

#[test]
fn every_environment_containing_p_requires_the_exact_q_binding_even_when_its_scene_uses_r() {
    let fixture = Fixture::new(false);
    for wrong_q in [false, true] {
        let mut environments = fixture.environments();
        environments[1].sources = vec![0, 1, 2];
        environments[1]
            .before
            .push(binding(path("p.prefab"), &fixture.p_key));
        environments[1].after = copy_bindings(&environments[1].before);
        if wrong_q {
            environments[1]
                .after
                .push(binding(fixture.q_reference(), &fixture.r_key));
        }
        let (request, work, inputs) = capture(&fixture, &environments, 0, Default::default());
        let original_allocations = allocations(&request);
        let captured_usage = work.usage();
        let failure = prepare(request, work).unwrap_err();
        assert_eq!(failure.environment_index(), Some(1));
        assert_eq!(failure.environment_name(), Some("mobile"));
        assert_code(failure.report(), "scene.prefab-extraction-invalid-source");
        let (request, mut spent_work, _) = failure.into_parts();
        assert_eq!(allocations(&request), original_allocations);
        inputs.assert_sources(request.sources(), &fixture);
        assert!(
            request
                .sources()
                .iter()
                .all(|source| source.key() != &fixture.q_key),
        );
        assert!(spent_work.usage().work_bytes > captured_usage.work_bytes);
        inputs.validate(&fixture, &mut spent_work);
        inputs.assert_live(&fixture);

        // A fresh, corrected capture is the positive control. Refusal exposes only the
        // original request and spent Work, never a partially verified P/Q candidate.
        environments[1].after = copy_bindings(&environments[1].before);
        environments[1]
            .after
            .push(binding(fixture.q_reference(), &fixture.q_key));
        let (request, work, inputs) = capture(&fixture, &environments, 0, Default::default());
        let mut prepared = prepare(request, work).unwrap();
        inputs.assert_sources(prepared.original_sources(), &fixture);
        prepared
            .validate_prefab_input(1, &inputs.p, &fixture.p)
            .unwrap();
        let mobile = compile(
            scene_source(&prepared.sources()[0]),
            &fixture.basis,
            prepared.sources(),
            &environments[1],
            Some(prepared.extracted()),
        );
        assert_projection(&mobile, 200);
        inputs.assert_live(&fixture);
    }
}

#[test]
fn membership_is_sorted_unique_and_complete_and_after_preserves_every_original_binding() {
    for defect in [
        "duplicate-member",
        "unsorted-members",
        "uncovered-source",
        "retarget",
        "drop",
        "new-non-q",
    ] {
        let fixture = Fixture::new(false);
        let mut environments = fixture.environments();
        match defect {
            "duplicate-member" => environments[1].sources.push(2),
            "unsorted-members" => environments[1].sources.swap(0, 1),
            "uncovered-source" => {
                environments[1].sources = vec![0];
                environments[1].before.clear();
                environments[1].after.clear();
            }
            "retarget" => environments[1].after[0].source = fixture.q_key.clone(),
            "drop" => environments[1].after.clear(),
            "new-non-q" => environments[1]
                .after
                .push(binding(path("extra.prefab"), &fixture.r_key)),
            _ => unreachable!(),
        }
        let (request, work, inputs) = capture(&fixture, &environments, 0, Default::default());
        let original_allocations = allocations(&request);
        let failure = prepare(request, work).unwrap_err();
        assert_eq!(
            failure.environment_index(),
            if defect == "uncovered-source" {
                None
            } else {
                Some(1)
            },
            "{defect}",
        );
        assert_eq!(
            failure.environment_name(),
            if defect == "uncovered-source" {
                None
            } else {
                Some("mobile")
            },
            "{defect}",
        );
        assert_code(failure.report(), "scene.prefab-extraction-invalid-source");
        let (request, mut spent_work, _) = failure.into_parts();
        assert_eq!(allocations(&request), original_allocations, "{defect}");
        inputs.assert_sources(request.sources(), &fixture);
        inputs.validate(&fixture, &mut spent_work);
        inputs.assert_live(&fixture);
    }
}

#[test]
fn exact_and_one_short_byte_and_node_budgets_cover_both_environments_and_all_original_captures() {
    let fixture = Fixture::new(false);
    let environments = fixture.environments();
    let (request, work, _) = capture(&fixture, &environments, 0, Default::default());
    let capture_usage = work.usage();
    let measured = prepare(request, work).unwrap().usage();
    assert!(measured.work_bytes > capture_usage.work_bytes);
    assert!(measured.work_nodes > capture_usage.work_nodes);
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(measured.work_bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(measured.work_nodes).unwrap());
    let (request, work, inputs) = capture(&fixture, &environments, 0, exact);
    assert_eq!(work.usage(), capture_usage);
    let prepared = prepare(request, work).unwrap();
    assert_eq!(prepared.usage(), measured);
    inputs.assert_sources(prepared.original_sources(), &fixture);
    inputs.assert_live(&fixture);

    for limits in [
        exact.with_working_bytes(ByteLimit::new(measured.work_bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(measured.work_nodes - 1).unwrap()),
    ] {
        let (request, work, inputs) = capture(&fixture, &environments, 0, limits);
        assert_eq!(work.usage(), capture_usage);
        let original_allocations = allocations(&request);
        let failure = prepare(request, work).unwrap_err();
        assert_code(failure.report(), "scene.prefab-extraction-capacity");
        let (request, spent_work, _) = failure.into_parts();
        assert_eq!(allocations(&request), original_allocations);
        inputs.assert_sources(request.sources(), &fixture);
        assert!(spent_work.usage().work_bytes >= capture_usage.work_bytes);
        assert!(spent_work.usage().work_nodes >= capture_usage.work_nodes);
        inputs.assert_live(&fixture);
    }
}

#[test]
fn independent_preservation_in_mobile_cannot_authorize_unknown_scene_data_affected_in_desktop() {
    let fixture = Fixture::new(true);
    let mut independent = fixture.environments();
    independent[0].sources = vec![1];
    let (request, work, inputs) = capture(&fixture, &independent, 0, Default::default());
    let before = compile(
        inputs.scene.document(),
        &fixture.basis,
        request.sources(),
        &independent[1],
        None,
    );
    let prepared = prepare(request, work).unwrap();
    inputs.assert_sources(prepared.original_sources(), &fixture);
    assert_eq!(prepared.source_changed(0), Some(false));
    assert!(Arc::ptr_eq(
        scene_source(&prepared.sources()[0]),
        &inputs.scene.shared_document(),
    ));
    let after = compile(
        scene_source(&prepared.sources()[0]),
        &fixture.basis,
        prepared.sources(),
        &independent[1],
        Some(prepared.extracted()),
    );
    for document in [&before, &after] {
        assert_projection(document, 200);
        let actor = document
            .entities()
            .iter()
            .find(|record| record.id() == &id("actor"))
            .unwrap();
        assert!(!actor.components().contains_key(&unknown_type()));
    }
    for source in [
        inputs.scene.document(),
        scene_source(&prepared.sources()[0]).as_ref(),
    ] {
        let actor = source
            .entities
            .iter()
            .find(|record| record.id == id("actor"))
            .unwrap();
        assert_eq!(actor.components[&unknown_type()], unknown_record());
    }
    inputs.assert_live(&fixture);

    // The same physical Scene is preserved in mobile and depends on P in desktop.
    // Check both environment orders so a later independent result cannot overwrite Affected.
    for independent_first in [false, true] {
        let mut environments = fixture.environments();
        let selected_environment = usize::from(independent_first);
        if independent_first {
            environments.swap(0, 1);
        }
        let (request, work, inputs) = capture(
            &fixture,
            &environments,
            selected_environment,
            Default::default(),
        );
        let original_allocations = allocations(&request);
        let captured_usage = work.usage();
        let failure = prepare(request, work).unwrap_err();
        assert_eq!(failure.environment_index(), Some(selected_environment));
        assert_eq!(failure.environment_name(), Some("desktop"));
        assert_code(failure.report(), "scene.prefab-extraction-unknown-schema");
        let (request, mut spent_work, _) = failure.into_parts();
        assert_eq!(allocations(&request), original_allocations);
        inputs.assert_sources(request.sources(), &fixture);
        assert!(spent_work.usage().work_bytes > captured_usage.work_bytes);
        assert!(spent_work.usage().work_nodes > captured_usage.work_nodes);
        inputs.validate(&fixture, &mut spent_work);
        inputs.assert_live(&fixture);
    }
}
