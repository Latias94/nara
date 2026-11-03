//! Independent consumers compile the exact saved generation, including unpublished disk guards.
//! Physical source order is R (Scene), P, U and the independent Prefab guard.

#![cfg(feature = "serde")]

use std::{mem::size_of, sync::Arc};

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, AuthoringSceneCompilationOmissionReason,
        AuthoringSceneCompilationReport, CapturedExtractionSource, PrefabExtractionBinding,
        PrefabExtractionDestination, PrefabExtractionEnvironment, PrefabExtractionRequest,
        PrefabExtractionSourceOutput, PrefabExtractionWork, PrefabSourceExtractionInput,
        SceneSourceExtractionInput, try_compile_authoring_scene_with_loader,
    },
    AuthoringDocumentSource, AuthoringSchemaBasis, ExpandedSceneDocument, PrefabAddition,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay,
    ResolvedPrefabSource, SceneAuthoringSession, SceneCompilationFailureKind,
    SceneCompilationLimits, SceneCompilationOptions, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    SceneFileEncoding, SceneFileEncodingAdmissionError, SceneFileLimits, ScenePatchDocument,
    ScenePatchOperation, ScenePatchReport, ScenePlacement, ScenePlacementParent, SourceDocumentKey,
    SourceDocumentKind,
};

const R: usize = 0;
const P: usize = 1;
const U: usize = 2;
const GUARD: usize = 3;

#[path = "prefab_extraction_saved_generation/stored_outputs.rs"]
mod stored_outputs;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

#[test]
fn reviewed_working_encoding_uses_original_sources_and_preserves_fixed_outputs() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    let originals = prepared.original_sources().as_ptr();
    let successors = prepared.sources().as_ptr();
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        for source in [R, P, U, GUARD] {
            let before = prepared.usage();
            let bytes = prepared
                .encode_original_source(source, encoding, SceneFileLimits::default())
                .unwrap();
            assert!(prepared.usage().work_bytes > before.work_bytes);
            let expected = match (&prepared.original_sources()[source], encoding) {
                (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Json) => {
                    document.to_json_string().unwrap()
                }
                (CapturedExtractionSource::Scene { document, .. }, SceneFileEncoding::Ron) => {
                    document.to_ron_string().unwrap()
                }
                (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Json) => {
                    source.document().to_json_string().unwrap()
                }
                (CapturedExtractionSource::Prefab { source }, SceneFileEncoding::Ron) => {
                    source.document().to_ron_string().unwrap()
                }
            };
            assert_eq!(bytes, expected.as_bytes());
            if prepared.source_changed(source) == Some(true) {
                let output = prepared
                    .encode_source_output(
                        PrefabExtractionSourceOutput::Existing(source),
                        encoding,
                        SceneFileLimits::default(),
                    )
                    .unwrap();
                assert_ne!(bytes, output.bytes());
            }
            let limits =
                SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(bytes.len()).unwrap());
            assert_eq!(
                prepared
                    .encode_original_source(source, encoding, limits)
                    .unwrap(),
                bytes
            );
            let before = prepared.usage();
            assert!(matches!(
                prepared.encode_original_source(
                    source,
                    encoding,
                    limits.with_encoded_bytes(ByteLimit::new(bytes.len() - 1).unwrap())
                ),
                Err(SceneFileEncodingAdmissionError::Format(_))
            ));
            assert!(prepared.usage().work_bytes > before.work_bytes);
        }
    }
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    assert_eq!(prepared.sources().as_ptr(), successors);
    fixture.assert_live(&capture.scene, &capture.prefabs);
}

#[test]
fn canonical_source_outputs_retain_their_actual_operation_and_spend_failed_encoding_work() {
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let basis = basis();
        let mut fixture = Fixture::new(&basis);
        let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
        let capture = fixture.capture(vec![environment], Default::default());
        let mut prepared = capture.request.prepare(capture.work).unwrap();
        let original = prepared.original_sources().as_ptr();
        let q = prepared.extracted().document() as *const _;

        let mut foreign_fixture = Fixture::new(&basis);
        let environment = foreign_fixture.environment("desktop", &[R, P, U, GUARD]);
        let foreign_capture = foreign_fixture.capture(vec![environment], Default::default());
        let mut foreign = foreign_capture
            .request
            .prepare(foreign_capture.work)
            .unwrap();

        for selected in [
            PrefabExtractionSourceOutput::Existing(R),
            PrefabExtractionSourceOutput::Existing(P),
            PrefabExtractionSourceOutput::Extracted,
        ] {
            let before = prepared.usage();
            let output = prepared
                .encode_source_output(selected, encoding, SceneFileLimits::default())
                .unwrap();
            assert_eq!(output.source(), selected);
            prepared.require_encoded_source_output(&output).unwrap();
            assert!(prepared.usage().work_nodes > before.work_nodes);
            assert!(prepared.usage().work_bytes > before.work_bytes);
            let allocation = output.bytes().as_ptr();
            let limit = ByteLimit::new(output.bytes().len()).unwrap();
            let exact = prepared
                .encode_source_output(
                    selected,
                    encoding,
                    SceneFileLimits::default().with_encoded_bytes(limit),
                )
                .unwrap();
            assert_eq!(exact.bytes(), output.bytes());
            let other = foreign
                .encode_source_output(selected, encoding, SceneFileLimits::default())
                .unwrap();
            assert_eq!(other.bytes(), output.bytes());
            assert_code(
                &prepared.require_encoded_source_output(&other).unwrap_err(),
                "scene.prefab-extraction-stale",
            );

            let before = prepared.usage();
            let error = prepared
                .encode_source_output(
                    selected,
                    encoding,
                    SceneFileLimits::default()
                        .with_encoded_bytes(ByteLimit::new(limit.get() - 1).unwrap()),
                )
                .unwrap_err();
            assert!(matches!(error, SceneFileEncodingAdmissionError::Format(_)));
            let refused = prepared.usage();
            assert!(refused.work_nodes > before.work_nodes);
            assert!(refused.work_bytes > before.work_bytes);
            let retry = prepared
                .encode_source_output(selected, encoding, SceneFileLimits::default())
                .unwrap();
            assert_eq!(retry.bytes(), output.bytes());
            assert_eq!(output.bytes().as_ptr(), allocation);
            assert!(prepared.usage().work_bytes > refused.work_bytes);

            match selected {
                PrefabExtractionSourceOutput::Existing(R) => {
                    let decoded = match encoding {
                        SceneFileEncoding::Json => {
                            SceneDocumentCandidate::decode_json_bytes(output.bytes())
                        }
                        SceneFileEncoding::Ron => {
                            SceneDocumentCandidate::decode_ron_bytes(output.bytes())
                        }
                    }
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                    assert_eq!(&decoded, scene_source(&prepared.sources()[R]).as_ref());
                }
                _ => {
                    let decoded = match encoding {
                        SceneFileEncoding::Json => {
                            PrefabDocumentCandidate::decode_json_bytes(output.bytes())
                        }
                        SceneFileEncoding::Ron => {
                            PrefabDocumentCandidate::decode_ron_bytes(output.bytes())
                        }
                    }
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                    let expected = match selected {
                        PrefabExtractionSourceOutput::Existing(index) => {
                            prefab_source(&prepared.sources()[index]).document()
                        }
                        PrefabExtractionSourceOutput::Extracted => prepared.extracted().document(),
                    };
                    assert_eq!(&decoded, expected);
                }
            }
        }
        let before = prepared.usage();
        assert!(matches!(
            prepared.encode_source_output(
                PrefabExtractionSourceOutput::Existing(usize::MAX),
                encoding,
                SceneFileLimits::default()
            ),
            Err(SceneFileEncodingAdmissionError::Admission(_))
        ));
        assert!(prepared.usage().work_bytes > before.work_bytes);
        assert_eq!(prepared.original_sources().as_ptr(), original);
        assert_eq!(prepared.extracted().document() as *const _, q);
        fixture.assert_live(&capture.scene, &capture.prefabs);
    }
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("content/saved-generation/{name}.json")).unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "a076a6ec-f5c1-4a13-85d5-d670a9ee29be".parse().unwrap(),
        AssetSourcePath::new(format!("content/saved-generation/{name}.json")).unwrap(),
        kind,
    )
}

fn metadata(name: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(format!("content/saved-generation/{name}.json")).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("saved.Link")
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Saved link", ComponentSchemaVersion::ONE)
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
        PersistentComponentSchemaOwnerId::try_new("saved.schema").unwrap(),
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
    assert!(adapter.contribution().is_none());
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn routed(anchor: &str, source: &str, target: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: key(source, SourceDocumentKind::Prefab),
            }],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(target))),
        },
    }
}

fn link(target: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(target))]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("saved.schema").unwrap())
}

fn instance(name: &str, source: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(PrefabInstance::forest(path(source)));
    record
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

struct Fixture {
    basis: AuthoringSchemaBasis,
    scene: SceneAuthoringSession,
    prefabs: [PrefabAuthoringSession; 3],
    keys: [SourceDocumentKey; 4],
    assets: [AssetRecord; 3],
    q_key: SourceDocumentKey,
    q: AssetRecord,
}

impl Fixture {
    fn new(basis: &AuthoringSchemaBasis) -> Self {
        let scene = SceneDocument::from_ordered_roots([
            instance("actor", "p"),
            instance("utility", "u"),
            SceneEntityRecord::new(id("observer"))
                .with_component(component(), link(routed("utility", "u", "u"))),
            SceneEntityRecord::new(id("dependent"))
                .with_component(component(), link(routed("actor", "p", "b"))),
        ]);
        let published = SceneDocumentCandidate::decode_json_str(&scene.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(AuthoringDocumentSource::new(path("r")), basis)
            .unwrap()
            .publish();
        let p = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("k")),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(id("b")),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        id("k"),
                    ))),
                ),
                SceneEntityPlacement::root(id("x")),
            ],
            [
                SceneEntityRecord::new(id("k")),
                SceneEntityRecord::new(id("b")).with_component(
                    component(),
                    link(EntityReference::SceneLocal { entity: id("x") }),
                ),
                SceneEntityRecord::new(id("x")).with_component(
                    component(),
                    link(EntityReference::SceneLocal { entity: id("b") }),
                ),
            ],
        );
        Self {
            basis: basis.clone(),
            scene: SceneAuthoringSession::from_published_authoring_document(published),
            prefabs: [
                open_prefab(&p, "p", basis),
                open_prefab(
                    &PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("u"))]),
                    "u",
                    basis,
                ),
                open_prefab(
                    &PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("guard"))]),
                    "guard",
                    basis,
                ),
            ],
            keys: [
                key("r", SourceDocumentKind::Scene),
                key("p", SourceDocumentKind::Prefab),
                key("u", SourceDocumentKind::Prefab),
                key("guard", SourceDocumentKind::Prefab),
            ],
            assets: [
                metadata("p", "c5f566b8-930e-4774-b138-12ca69445f91"),
                metadata("u", "912e852a-5ac4-4c27-abf9-87dd895d9984"),
                metadata("guard", "5ea53dce-c588-4eae-83c8-39f8c9a2d835"),
            ],
            q_key: key("q", SourceDocumentKind::Prefab),
            q: metadata("q", "7688b7ac-6ed4-4814-b78a-0e7c7a68b454"),
        }
    }

    fn environment(&self, name: &str, members: &[usize]) -> PrefabExtractionEnvironment {
        let binding = |reference, source: &SourceDocumentKey| PrefabExtractionBinding {
            reference,
            source: source.clone(),
        };
        let before: Vec<_> = members
            .iter()
            .copied()
            .filter(|&index| index != R)
            .map(|index| binding(path(["r", "p", "u", "guard"][index]), &self.keys[index]))
            .collect();
        let mut after: Vec<_> = before
            .iter()
            .map(|entry| binding(entry.reference.clone(), &entry.source))
            .collect();
        after.push(binding(AssetRef::StableId(self.q.stable_id()), &self.q_key));
        PrefabExtractionEnvironment {
            name: name.into(),
            sources: members.into(),
            before,
            after,
            assets_before: None,
            assets_after: None,
        }
    }

    // The caller has admitted the wrappers before sharing any source bodies or keys.
    fn sources(
        &self,
        scene: &SceneSourceExtractionInput,
        prefabs: &[PrefabSourceExtractionInput; 3],
    ) -> Vec<CapturedExtractionSource> {
        let mut sources = Vec::with_capacity(4);
        sources.push(CapturedExtractionSource::Scene {
            key: self.keys[R].clone(),
            document: scene.shared_document(),
            basis: scene.schema_basis(),
        });
        for (index, input) in prefabs.iter().enumerate() {
            sources.push(CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    input.shared_document(),
                    input.schema_basis(),
                    self.keys[index + 1].clone(),
                )
                .with_source_asset(&self.assets[index])
                .unwrap(),
            });
        }
        sources
    }

    fn capture(
        &mut self,
        environments: Vec<PrefabExtractionEnvironment>,
        limits: SceneCompilationLimits,
    ) -> Capture {
        let mut work = PrefabExtractionWork::new(limits).unwrap();
        let disk_scene = self
            .scene
            .capture_prefab_extraction_input(&mut work)
            .unwrap();
        let disk_prefabs = self
            .prefabs
            .each_ref()
            .map(|session| session.capture_prefab_extraction_input(&mut work).unwrap());
        let wrapper_bytes = size_of::<Capture>()
            + size_of::<PrefabExtractionRequest>()
            + 8 * size_of::<CapturedExtractionSource>()
            + 2 * self
                .keys
                .iter()
                .map(|key| key.checked_retained_bytes().unwrap())
                .sum::<usize>()
            + 2 * self
                .assets
                .iter()
                .map(|asset| asset.path().as_str().len())
                .sum::<usize>()
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
            + self.keys[P].checked_retained_bytes().unwrap()
            + self.q_key.checked_retained_bytes().unwrap()
            + self.q.path().as_str().len()
            + "k".len();
        work.admit_capture_storage(Some(wrapper_bytes)).unwrap();
        let disk = self.sources(&disk_scene, &disk_prefabs);

        // These are ordinary authored edits between the saved snapshot and working capture.
        let u = &mut self.prefabs[U - 1];
        let mut placements = u.document().placements.clone();
        placements.push(SceneEntityPlacement::root(id("v")));
        let patch = ScenePatchDocument::new([
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(id("v")),
            },
            ScenePatchOperation::SetPlacements {
                expected: u.document().placements.clone(),
                value: placements,
            },
        ]);
        let report = u.apply_authoring_patch(&patch);
        assert!(report.applied, "{:?}", report.diagnostics);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: id("observer"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("target"),
            value: ComponentValue::EntityReference(routed("utility", "u", "v")),
        }]);
        let report = self.scene.apply_authoring_patch(&patch);
        assert!(report.applied, "{:?}", report.diagnostics);

        let scene = self
            .scene
            .capture_prefab_extraction_input(&mut work)
            .unwrap();
        let prefabs = self
            .prefabs
            .each_ref()
            .map(|session| session.capture_prefab_extraction_input(&mut work).unwrap());
        let request = PrefabExtractionRequest::new(
            self.sources(&scene, &prefabs),
            environments,
            0,
            self.basis.clone(),
            self.keys[P].clone(),
            id("k"),
            PrefabExtractionDestination {
                key: self.q_key.clone(),
                reference: AssetRef::StableId(self.q.stable_id()),
                metadata: self.q.clone(),
            },
        )
        .with_max_depth(16);
        self.scene
            .validate_prefab_extraction_input(&scene, &mut work)
            .unwrap();
        for (session, input) in self.prefabs.iter().zip(&prefabs) {
            session
                .validate_prefab_extraction_input(input, &mut work)
                .unwrap();
        }
        Capture {
            request,
            work,
            disk,
            scene,
            prefabs,
        }
    }

    fn assert_live(
        &self,
        scene: &SceneSourceExtractionInput,
        prefabs: &[PrefabSourceExtractionInput; 3],
    ) {
        assert!(Arc::ptr_eq(
            &self.scene.shared_document(),
            &scene.shared_document(),
        ));
        assert_eq!(self.scene.revision(), scene.revision());
        assert_eq!(self.scene.content_state(), scene.content_state());
        for (session, input) in self.prefabs.iter().zip(prefabs) {
            assert!(Arc::ptr_eq(
                &session.shared_document(),
                &input.shared_document(),
            ));
            assert_eq!(session.revision(), input.revision());
            assert_eq!(session.content_state(), input.content_state());
        }
    }
}

struct Capture {
    request: PrefabExtractionRequest,
    work: PrefabExtractionWork,
    disk: Vec<CapturedExtractionSource>,
    scene: SceneSourceExtractionInput,
    prefabs: [PrefabSourceExtractionInput; 3],
}

fn scene_source(source: &CapturedExtractionSource) -> &Arc<SceneDocument> {
    let CapturedExtractionSource::Scene { document, .. } = source else {
        panic!("expected the physical Scene");
    };
    document
}

fn prefab_source(source: &CapturedExtractionSource) -> &ResolvedPrefabSource {
    let CapturedExtractionSource::Prefab { source } = source else {
        panic!("expected a physical Prefab");
    };
    source
}

fn allocation(source: &CapturedExtractionSource) -> usize {
    match source {
        CapturedExtractionSource::Scene { document, .. } => Arc::as_ptr(document) as usize,
        CapturedExtractionSource::Prefab { source } => {
            source.document() as *const PrefabDocument as usize
        }
    }
}

fn assert_shared(actual: &CapturedExtractionSource, expected: &CapturedExtractionSource) {
    assert_eq!(actual.key(), expected.key());
    match (actual, expected) {
        (
            CapturedExtractionSource::Scene {
                document: actual,
                basis: actual_basis,
                ..
            },
            CapturedExtractionSource::Scene {
                document: expected,
                basis: expected_basis,
                ..
            },
        ) => {
            assert!(Arc::ptr_eq(actual, expected));
            assert_eq!(actual_basis, expected_basis);
        }
        (
            CapturedExtractionSource::Prefab { source: actual },
            CapturedExtractionSource::Prefab { source: expected },
        ) => assert!(std::ptr::eq(actual.document(), expected.document())),
        _ => panic!("saved generation must preserve the physical source kind"),
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
    sources: &[CapturedExtractionSource],
    environment: &PrefabExtractionEnvironment,
    extracted: Option<&ResolvedPrefabSource>,
    basis: &AuthoringSchemaBasis,
) -> AuthoringSceneCompilationReport {
    let bindings = if extracted.is_some() {
        &environment.after
    } else {
        &environment.before
    };
    try_compile_authoring_scene_with_loader(
        scene_source(&sources[R]),
        basis,
        SceneCompilationOptions::default(),
        None,
        |reference, _| {
            let source = bindings
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
            Ok::<_, std::convert::Infallible>(source.cloned())
        },
    )
    .unwrap()
}

fn successful(report: AuthoringSceneCompilationReport) -> ExpandedSceneDocument {
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics(),
    );
    assert!(report.failure().is_none(), "{:?}", report.failure());
    assert!(
        !report.omissions().is_empty(),
        "the real Data Adapter must participate",
    );
    assert!(report.omissions().iter().all(|omission| {
        omission.component == component()
            && omission.reason == AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
    }));
    report
        .into_parts()
        .candidate
        .expect("actual authoring compilation must produce a candidate")
}

fn assert_target(document: &ExpandedSceneDocument, owner: &str, anchor: &str, local: &str) {
    let (target, _) = document
        .provenance()
        .iter()
        .find(|(_, origin)| {
            origin.local_entity_id().as_str() == local
                && origin
                    .projection_path()
                    .is_some_and(|path| path.root_anchor().as_str() == anchor)
        })
        .expect("the actual compiler must project the routed target");
    let owner = document
        .entities()
        .iter()
        .find(|entity| entity.id() == &id(owner))
        .unwrap();
    assert_eq!(
        owner.components()[&component()].value.get("target"),
        Some(&ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: target.clone(),
            }
        )),
    );
}

#[test]
fn saved_scene_cannot_borrow_dirty_u_and_retry_publishes_the_exact_working_u() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    assert_eq!(prefab_source(&capture.disk[U]).document().entities.len(), 1);
    assert_eq!(capture.prefabs[U - 1].document().entities.len(), 2);
    let saved_before = successful(compile(
        &capture.disk,
        &capture.request.environments()[0],
        None,
        &basis,
    ));
    assert_target(&saved_before, "observer", "utility", "u");
    let working_before = successful(compile(
        capture.request.sources(),
        &capture.request.environments()[0],
        None,
        &basis,
    ));
    assert_target(&working_before, "observer", "utility", "v");
    assert_target(&working_before, "dependent", "actor", "b");
    let history = fixture.scene.history_status();
    let prefab_history = fixture
        .prefabs
        .each_ref()
        .map(|session| session.history_status());
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    assert_eq!(prepared.source_changed(R), Some(true));
    assert_eq!(prepared.source_changed(P), Some(true));
    assert_eq!(prepared.source_changed(U), Some(false));
    let originals = prepared.original_sources().as_ptr();
    let outputs = prepared.sources().as_ptr();
    let allocations: Vec<_> = prepared.sources().iter().map(allocation).collect();
    let before_failure = prepared.usage();

    let error = prepared
        .prepare_saved_generation(&capture.disk, &[true, true, false, false])
        .unwrap_err();
    assert!(!error.report().applied);
    assert!(error.report().diagnostics.has_errors());
    assert_eq!(error.environment_index(), Some(0));
    assert_eq!(error.source_index(), Some(R));
    assert_eq!(
        error.compilation_failure().unwrap().reason(),
        &SceneCompilationFailureKind::Reference,
    );
    let after_failure = prepared.usage();
    assert!(after_failure.work_bytes > before_failure.work_bytes);
    assert!(after_failure.work_nodes > before_failure.work_nodes);
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    assert_eq!(prepared.sources().as_ptr(), outputs);
    assert_eq!(
        prepared
            .sources()
            .iter()
            .map(allocation)
            .collect::<Vec<_>>(),
        allocations,
    );
    prepared.require_scene_input(R, &capture.scene).unwrap();
    prepared
        .require_prefab_input(U, &capture.prefabs[U - 1])
        .unwrap();

    // U is unchanged by extraction, but Review may explicitly save this exact current source.
    let published = [true, true, true, false];
    let proof = prepared
        .prepare_saved_generation(&capture.disk, &published)
        .unwrap();
    assert_eq!(proof.published(), published);
    for index in [R, P, U] {
        assert_shared(&proof.sources()[index], &prepared.sources()[index]);
    }
    assert_shared(&proof.sources()[GUARD], &capture.disk[GUARD]);
    assert!(std::ptr::eq(
        proof.extracted().document(),
        prepared.extracted().document(),
    ));
    let saved = successful(compile(
        proof.sources(),
        &prepared.environments()[0],
        Some(proof.extracted()),
        &basis,
    ));
    assert_target(&saved, "observer", "utility", "v");
    assert_target(&saved, "dependent", "actor", "b");
    assert_eq!(working_before.roots(), saved.roots());
    assert_eq!(working_before.entities().len(), saved.entities().len());
    assert!(prepared.usage().work_bytes > after_failure.work_bytes);
    assert!(prepared.usage().work_nodes > after_failure.work_nodes);
    prepared.require_saved_generation(&proof).unwrap();
    fixture.assert_live(&capture.scene, &capture.prefabs);
    assert_eq!(fixture.scene.history_status(), history);
    assert_eq!(
        fixture
            .prefabs
            .each_ref()
            .map(|session| session.history_status()),
        prefab_history,
    );
}

#[test]
fn every_changed_source_is_required_and_complete_publication_stays_with_its_work_owner() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    for omitted in [R, P] {
        assert_eq!(prepared.source_changed(omitted), Some(true));
        let mut published = [true; 4];
        published[omitted] = false;
        let error = prepared
            .prepare_saved_generation(&capture.disk, &published)
            .unwrap_err();
        assert_code(error.report(), "scene.prefab-extraction-invalid-source");
        assert_eq!(error.source_index(), Some(omitted));
        assert!(error.compilation_failure().is_none());
    }
    let proof = prepared
        .prepare_saved_generation(&capture.disk, &[true; 4])
        .unwrap();
    assert_eq!(proof.sources().len(), 4);
    assert_eq!(proof.published(), [true; 4]);
    for (actual, expected) in proof.sources().iter().zip(prepared.sources()) {
        assert_shared(actual, expected);
    }
    assert!(std::ptr::eq(
        proof.extracted().document(),
        prepared.extracted().document(),
    ));
    assert_eq!(proof.extracted().key(), prepared.extracted().key());
    successful(compile(
        proof.sources(),
        &prepared.environments()[0],
        Some(proof.extracted()),
        &basis,
    ));

    let mut foreign_fixture = Fixture::new(&basis);
    let environment = foreign_fixture.environment("desktop", &[R, P, U, GUARD]);
    let foreign_capture = foreign_fixture.capture(vec![environment], Default::default());
    let mut foreign = foreign_capture
        .request
        .prepare(foreign_capture.work)
        .unwrap();
    assert_eq!(foreign.schema_basis(), prepared.schema_basis());
    assert_eq!(
        foreign.extracted().document(),
        prepared.extracted().document(),
    );
    assert_eq!(
        scene_source(&foreign.sources()[R]),
        scene_source(&prepared.sources()[R]),
    );
    let before = foreign.usage();
    assert_code(
        &foreign.require_saved_generation(&proof).unwrap_err(),
        "scene.prefab-extraction-stale",
    );
    assert!(foreign.usage().work_bytes > before.work_bytes);
    let own = foreign
        .prepare_saved_generation(&foreign_capture.disk, &[true; 4])
        .unwrap();
    foreign.require_saved_generation(&own).unwrap();
    prepared.require_saved_generation(&proof).unwrap();
    fixture.assert_live(&capture.scene, &capture.prefabs);
    foreign_fixture.assert_live(&foreign_capture.scene, &foreign_capture.prefabs);
}

#[test]
fn disk_guards_require_original_physical_order_kind_key_basis_and_asset_association() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    let wrong_key = key("wrong", SourceDocumentKind::Scene);
    let empty_basis =
        AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let wrong_asset = metadata("u", "30a74431-cf2f-4c5c-bb41-f3999e88daa1");
    for defect in [
        "order",
        "key",
        "scene-basis",
        "prefab-basis",
        "kind",
        "asset",
    ] {
        let bytes = 4 * size_of::<CapturedExtractionSource>()
            + capture
                .disk
                .iter()
                .map(|source| source.key().checked_retained_bytes().unwrap())
                .sum::<usize>()
            + fixture
                .assets
                .iter()
                .map(|asset| asset.path().as_str().len())
                .sum::<usize>()
            + wrong_key.checked_retained_bytes().unwrap()
            + fixture.keys[U].checked_retained_bytes().unwrap()
            + wrong_asset.path().as_str().len();
        prepared.admit_binding_storage(Some(bytes)).unwrap();
        let mut disk: Vec<_> = capture
            .disk
            .iter()
            .map(|source| match source {
                CapturedExtractionSource::Scene {
                    key,
                    document,
                    basis,
                } => CapturedExtractionSource::Scene {
                    key: key.clone(),
                    document: Arc::clone(document),
                    basis: basis.clone(),
                },
                CapturedExtractionSource::Prefab { source } => CapturedExtractionSource::Prefab {
                    source: source.clone(),
                },
            })
            .collect();
        let expected_index = match defect {
            "order" => {
                disk.swap(P, U);
                P
            }
            "key" => {
                let CapturedExtractionSource::Scene { key, .. } = &mut disk[R] else {
                    unreachable!()
                };
                *key = wrong_key.clone();
                R
            }
            "scene-basis" => {
                let CapturedExtractionSource::Scene { basis, .. } = &mut disk[R] else {
                    unreachable!()
                };
                *basis = empty_basis.clone();
                R
            }
            "prefab-basis" | "asset" => {
                disk[U] = CapturedExtractionSource::Prefab {
                    source: ResolvedPrefabSource::bound_authoring_source(
                        capture.prefabs[U - 1].shared_document(),
                        if defect == "prefab-basis" {
                            empty_basis.clone()
                        } else {
                            basis.clone()
                        },
                        fixture.keys[U].clone(),
                    )
                    .with_source_asset(if defect == "asset" {
                        &wrong_asset
                    } else {
                        &fixture.assets[U - 1]
                    })
                    .unwrap(),
                };
                U
            }
            "kind" => {
                disk[U] = CapturedExtractionSource::Scene {
                    key: fixture.keys[U].clone(),
                    document: capture.scene.shared_document(),
                    basis: basis.clone(),
                };
                U
            }
            _ => unreachable!(),
        };
        let before = prepared.usage();
        let error = prepared
            .prepare_saved_generation(&disk, &[true; 4])
            .unwrap_err();
        assert_code(error.report(), "scene.prefab-extraction-invalid-source");
        assert_eq!(error.source_index(), Some(expected_index), "{defect}");
        assert!(error.compilation_failure().is_none(), "{defect}");
        assert!(prepared.usage().work_bytes > before.work_bytes);
    }
    for (disk, published) in [
        (&capture.disk[..3], &[true; 4][..]),
        (&capture.disk[..], &[true; 3][..]),
    ] {
        let error = prepared
            .prepare_saved_generation(disk, published)
            .unwrap_err();
        assert_code(error.report(), "scene.prefab-extraction-invalid-source");
        assert_eq!(error.source_index(), None);
        assert!(error.compilation_failure().is_none());
    }
    let proof = prepared
        .prepare_saved_generation(&capture.disk, &[true; 4])
        .unwrap();
    prepared.require_saved_generation(&proof).unwrap();
    successful(compile(
        proof.sources(),
        &prepared.environments()[0],
        Some(proof.extracted()),
        &basis,
    ));
    fixture.assert_live(&capture.scene, &capture.prefabs);
}

#[test]
fn the_second_environment_compiles_saved_u_instead_of_borrowing_its_dirty_working_source() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environments = vec![
        fixture.environment("desktop", &[P, GUARD]),
        fixture.environment("mobile", &[R, P, U, GUARD]),
    ];
    let capture = fixture.capture(environments, Default::default());
    successful(compile(
        capture.request.sources(),
        &capture.request.environments()[1],
        None,
        &basis,
    ));
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    let originals = prepared.original_sources().as_ptr();
    let before = prepared.usage();
    let error = prepared
        .prepare_saved_generation(&capture.disk, &[true, true, false, false])
        .unwrap_err();
    assert_eq!(error.environment_index(), Some(1));
    assert_eq!(error.source_index(), Some(R));
    assert_eq!(
        error.compilation_failure().unwrap().reason(),
        &SceneCompilationFailureKind::Reference,
    );
    assert!(error.report().diagnostics.has_errors());
    let refused = prepared.usage();
    assert!(refused.work_nodes > before.work_nodes);
    assert!(refused.work_bytes > before.work_bytes);
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    let proof = prepared
        .prepare_saved_generation(&capture.disk, &[true, true, true, false])
        .unwrap();
    let mobile = successful(compile(
        proof.sources(),
        &prepared.environments()[1],
        Some(proof.extracted()),
        &basis,
    ));
    assert_target(&mobile, "observer", "utility", "v");
    assert!(prepared.usage().work_bytes > refused.work_bytes);
    prepared.require_saved_generation(&proof).unwrap();
    fixture.assert_live(&capture.scene, &capture.prefabs);
}

fn unknown_type() -> ComponentTypeId {
    ComponentTypeId::new("missing.SavedPayload")
}

fn unknown_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "opaque",
            ComponentValue::String("retain the complete disk payload".into()),
        )]),
    )
}

#[test]
fn unknown_disk_data_is_preserved_only_when_its_complete_physical_context_is_independent() {
    let basis = basis();
    for context in ["independent", "direct", "dormant"] {
        let mut fixture = Fixture::new(&basis);
        let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
        let mut capture = fixture.capture(vec![environment], Default::default());
        let mut record = SceneEntityRecord::new(id("guard"));
        if context == "dormant" {
            let mut addition = PrefabAddition::new(id("hidden"));
            addition.components.insert(unknown_type(), unknown_record());
            addition.prefab = Some(PrefabInstance::forest(path("p")));
            let mut nested = PrefabInstance::forest(path("u"));
            let mut nested_overlay = PrefabOverlay::new(fixture.keys[U].clone());
            nested_overlay.additions.push(addition);
            nested.overrides = Some(nested_overlay);
            let mut outer = PrefabInstance::forest(path("u"));
            let mut overlay = PrefabOverlay::new(fixture.keys[U].clone());
            overlay.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("never-materialized")),
                value: Some(nested),
            });
            outer.overrides = Some(overlay);
            record.prefab = Some(outer);
        } else {
            record.components.insert(unknown_type(), unknown_record());
            if context == "direct" {
                record.prefab = Some(PrefabInstance::forest(path("p")));
            }
        }
        // An actual decoded disk source, independently opened for bounded capture. It is never
        // substituted for the working guard Session or its original input/history authority.
        let disk_guard = open_prefab(
            &PrefabDocument::from_ordered_roots([record]),
            "guard",
            &basis,
        );
        let input = disk_guard
            .capture_prefab_extraction_input(&mut capture.work)
            .unwrap();
        capture
            .work
            .admit_capture_storage(Some(
                size_of::<CapturedExtractionSource>()
                    + fixture.keys[GUARD].checked_retained_bytes().unwrap()
                    + fixture.assets[GUARD - 1].path().as_str().len(),
            ))
            .unwrap();
        capture.disk[GUARD] = CapturedExtractionSource::Prefab {
            source: ResolvedPrefabSource::bound_authoring_source(
                input.shared_document(),
                input.schema_basis(),
                fixture.keys[GUARD].clone(),
            )
            .with_source_asset(&fixture.assets[GUARD - 1])
            .unwrap(),
        };
        let mut prepared = capture.request.prepare(capture.work).unwrap();
        assert_eq!(prepared.source_changed(GUARD), Some(false));
        let before = prepared.usage();
        let result = prepared.prepare_saved_generation(&capture.disk, &[true, true, true, false]);
        assert!(prepared.usage().work_nodes > before.work_nodes);
        assert!(prepared.usage().work_bytes > before.work_bytes);
        if context == "independent" {
            let proof = result.unwrap();
            assert_shared(&proof.sources()[GUARD], &capture.disk[GUARD]);
            assert_eq!(
                prefab_source(&proof.sources()[GUARD]).document().entities[0].components
                    [&unknown_type()],
                unknown_record(),
            );
            successful(compile(
                proof.sources(),
                &prepared.environments()[0],
                Some(proof.extracted()),
                &basis,
            ));
            prepared.require_saved_generation(&proof).unwrap();
        } else {
            let error = result.unwrap_err();
            assert_code(error.report(), "scene.prefab-extraction-unknown-schema");
            assert_eq!(error.environment_index(), Some(0), "{context}");
            assert_eq!(error.source_index(), Some(GUARD), "{context}");
            assert!(error.compilation_failure().is_none());
            let refused = prepared.usage();
            let proof = prepared
                .prepare_saved_generation(&capture.disk, &[true; 4])
                .unwrap();
            assert_shared(&proof.sources()[GUARD], &prepared.sources()[GUARD]);
            assert!(
                prefab_source(&proof.sources()[GUARD]).document().entities[0]
                    .components
                    .is_empty(),
            );
            assert!(prepared.usage().work_bytes > refused.work_bytes);
            prepared.require_saved_generation(&proof).unwrap();
        }
        fixture.assert_live(&capture.scene, &capture.prefabs);
    }
}

#[test]
fn exact_and_one_short_allowances_include_original_capture_working_proof_and_saved_compilation() {
    let basis = basis();
    let mut fixture = Fixture::new(&basis);
    let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
    let capture = fixture.capture(vec![environment], Default::default());
    let mut prepared = capture.request.prepare(capture.work).unwrap();
    let working_usage = prepared.usage();
    // Measure preparation itself; requiring a proof is a separate charged operation.
    let proof = prepared
        .prepare_saved_generation(&capture.disk, &[true, true, true, false])
        .unwrap();
    let measured = prepared.usage();
    assert!(measured.work_bytes > working_usage.work_bytes);
    assert!(measured.work_nodes > working_usage.work_nodes);
    assert_shared(&proof.sources()[U], &prepared.sources()[U]);
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(measured.work_bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(measured.work_nodes).unwrap());
    for (limits, succeeds) in [
        (exact, true),
        (
            exact.with_working_bytes(ByteLimit::new(measured.work_bytes - 1).unwrap()),
            false,
        ),
        (
            exact.with_reference_work_nodes(ItemLimit::new(measured.work_nodes - 1).unwrap()),
            false,
        ),
    ] {
        let mut fixture = Fixture::new(&basis);
        let environment = fixture.environment("desktop", &[R, P, U, GUARD]);
        let capture = fixture.capture(vec![environment], limits);
        let mut prepared = capture
            .request
            .prepare(capture.work)
            .expect("working proof fits before the saved-generation boundary");
        assert_eq!(prepared.usage(), working_usage);
        let originals = prepared.original_sources().as_ptr();
        let outputs = prepared.sources().as_ptr();
        let result = prepared.prepare_saved_generation(&capture.disk, &[true, true, true, false]);
        if succeeds {
            let proof = result.unwrap();
            assert_eq!(prepared.usage(), measured);
            assert_shared(&proof.sources()[R], &prepared.sources()[R]);
            assert!(std::ptr::eq(
                proof.extracted().document(),
                prepared.extracted().document(),
            ));
        } else {
            let error = result.unwrap_err();
            assert_code(error.report(), "scene.prefab-extraction-capacity");
            assert!(prepared.usage().work_bytes >= working_usage.work_bytes);
            assert!(prepared.usage().work_nodes >= working_usage.work_nodes);
        }
        assert_eq!(prepared.original_sources().as_ptr(), originals);
        assert_eq!(prepared.sources().as_ptr(), outputs);
        fixture.assert_live(&capture.scene, &capture.prefabs);
    }
}
