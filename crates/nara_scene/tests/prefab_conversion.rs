#![cfg(feature = "serde")]

#[path = "support/prefab_source.rs"]
mod prefab_source;

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetRef;
use nara_identity::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringSchemaBasis,
    LocalizedPrefabEntity, PrefabAddition, PrefabAuthoringSession, PrefabConversionError,
    PrefabConversionReceipt, PrefabConversionRequest, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityTarget, PrefabExpansionWorkingSet, PrefabInstance, PrefabOverlay, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabProjectionStep, PrefabReferenceBinding,
    PreparedPrefabConversion, PreparedScenePrefabConversion, ResolvedPrefabSource,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityProvenance, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePatchReport, ScenePlacement, ScenePlacementOwner,
    ScenePrefabConversionRequest,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/conversion/{name}.prefab.json")).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ConversionLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.conversion").unwrap()
}

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.external").unwrap(),
            PersistentRuntimeId::parse_str("a51d6a7c-3376-4de6-b846-7737431d6085").unwrap(),
        ),
    }
}

fn basis() -> AuthoringSchemaBasis {
    let reference_field = |name: &str| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::EntityRef,
        )
        .with_capabilities([
            ComponentCapability::Scene,
            ComponentCapability::Inspect,
            ComponentCapability::EntityRef,
        ])
    };
    let mut external = reference_field("external");
    external.default_value = Some(ComponentValue::EntityReference(persistent()));
    // Conversion copies Scene-capable data without requiring Edit, a template or a native codec.
    let schema = ComponentSchema::new(component(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
        .with_fields([
            reference_field("target"),
            external,
            ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
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

fn link(target: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(target) }),
            ),
            ("amount", ComponentValue::U64(7)),
        ]),
    )
    .with_owner(owner())
}

fn instance(name: &str) -> PrefabInstance {
    PrefabInstance {
        source: source(name),
        overrides: None,
    }
}

fn anchor(name: &str, prefab: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(name));
    record.prefab = Some(instance(prefab));
    record
}

fn scene_document() -> SceneDocument {
    let unknown = ComponentTypeId::new("test.Unavailable");
    let mut selected = anchor("A", "base")
        .with_children([id("local-child"), id("local-peer")])
        .with_component(
            unknown.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(73)),
        );
    let mut overlay = PrefabOverlay::new(prefab_source::key("base"));
    let mut addition = PrefabAddition::new(id("leaf"));
    addition.components.insert(component(), link("leaf"));
    addition.bindings.insert(
        component(),
        vec![PrefabReferenceBinding {
            slot: ComponentFieldPath::from_fields(["target"]),
            expected: id("leaf"),
            target: PrefabEntityTarget::source(id("root-a")),
        }],
    );
    overlay.additions.push(addition);
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("leaf")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    selected.prefab.as_mut().unwrap().overrides = Some(overlay);
    SceneDocument::new(
        [id("A"), id("B"), id("C")],
        [
            selected,
            anchor("local-child", "missing-child").with_component(
                unknown,
                SceneComponentRecord::new(
                    ComponentSchemaVersion(9),
                    ComponentValue::String("preserve this child".into()),
                ),
            ),
            SceneEntityRecord::new(id("local-peer")),
            anchor("B", "base"),
            anchor("C", "unrelated-c"),
        ],
    )
}

fn prefab_document() -> PrefabDocument {
    let scene = scene_document();
    PrefabDocument::new(scene.roots, scene.entities)
}

fn open_scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/conversion.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn open_prefab(document: &PrefabDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(source("container")), basis)
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

#[derive(Debug, PartialEq, Eq)]
struct LoadFailure(&'static str);

struct Sources {
    documents: HashMap<AssetRef, ResolvedPrefabSource>,
    reads: Vec<AssetRef>,
    fail: Option<AssetRef>,
}

impl Sources {
    fn new(basis: &AuthoringSchemaBasis) -> Self {
        let base = PrefabDocument::new(
            [id("root-a"), id("root-b"), id("root-c")],
            [
                SceneEntityRecord::new(id("root-a"))
                    .with_children([id("leaf")])
                    .with_component(component(), link("leaf")),
                SceneEntityRecord::new(id("leaf")).with_component(component(), link("root-a")),
                anchor("root-b", "nested"),
                anchor("root-c", "nested-alias"),
            ],
        );
        let mut unclaimed = link("leaf");
        unclaimed.owner = None;
        let nested = ResolvedPrefabSource::bound_authoring_source(
            Arc::new(PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("leaf")).with_component(component(), unclaimed),
            ])),
            basis.clone(),
            prefab_source::key("nested"),
        );
        Self {
            documents: [
                (
                    source("base"),
                    ResolvedPrefabSource::bound_authoring_source(
                        Arc::new(base),
                        basis.clone(),
                        prefab_source::key("base"),
                    ),
                ),
                (source("nested"), nested.clone()),
                (source("nested-alias"), nested),
            ]
            .into(),
            reads: Vec::new(),
            fail: None,
        }
    }

    fn resolve(&mut self, request: &AssetRef) -> Result<Option<ResolvedPrefabSource>, LoadFailure> {
        self.reads.push(request.clone());
        if self.fail.as_ref() == Some(request) {
            return Err(LoadFailure("offline"));
        }
        Ok(self.documents.get(request).cloned())
    }

    fn load(
        &mut self,
        request: &AssetRef,
        _: &mut PrefabExpansionWorkingSet,
    ) -> Result<Option<ResolvedPrefabSource>, LoadFailure> {
        self.resolve(request)
    }
}

fn assert_actual_reads(reads: &[AssetRef]) {
    let actual = [source("base"), source("nested"), source("nested-alias")];
    assert!(
        actual.iter().all(|request| reads.contains(request)),
        "{reads:?}"
    );
    assert!(
        reads.iter().all(|request| actual.contains(request)),
        "{reads:?}"
    );
}

fn step(lookup: &str, canonical: &str, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: source(lookup),
        canonical_source: prefab_source::key(canonical),
        entity: id(entity),
    }
}

fn mapped<'a>(
    mappings: &'a [LocalizedPrefabEntity],
    steps: &[PrefabProjectionStep],
) -> &'a SceneEntityId {
    let mut found = mappings.iter().filter(|mapping| {
        mapping
            .provenance()
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &id("A") && path.steps() == steps)
    });
    let mapping = found
        .next()
        .expect("the complete typed occurrence must be mapped");
    assert!(
        found.next().is_none(),
        "the mapped occurrence must be unique"
    );
    assert_eq!(
        mapping.provenance().local_entity_id(),
        steps.last().unwrap().entity_id()
    );
    mapping.entity()
}

fn entity<'a>(records: &'a [SceneEntityRecord], id: &SceneEntityId) -> &'a SceneEntityRecord {
    records
        .iter()
        .find(|record| &record.id == id)
        .expect("source entity")
}

fn mapping_snapshot(
    mappings: &[LocalizedPrefabEntity],
) -> Vec<(SceneEntityProvenance, SceneEntityId)> {
    mappings
        .iter()
        .map(|mapping| (mapping.provenance().clone(), mapping.entity().clone()))
        .collect()
}

fn assert_review(
    original: &[SceneEntityRecord],
    reviewed: &[SceneEntityRecord],
    mappings: &[LocalizedPrefabEntity],
) {
    assert_eq!(mappings.len(), 7);
    assert_eq!(reviewed.len(), original.len() + mappings.len());
    let ids = mappings
        .iter()
        .map(LocalizedPrefabEntity::entity)
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), mappings.len());
    assert!(
        ids.iter()
            .all(|id| original.iter().all(|record| &record.id != *id))
    );
    let root_a = mapped(mappings, &[step("base", "base", "root-a")]);
    let root_b = mapped(mappings, &[step("base", "base", "root-b")]);
    let root_c = mapped(mappings, &[step("base", "base", "root-c")]);
    let leaf = mapped(mappings, &[step("base", "base", "leaf")]);
    let nested_b = mapped(
        mappings,
        &[
            step("base", "base", "root-b"),
            step("nested", "nested", "leaf"),
        ],
    );
    let nested_c = mapped(
        mappings,
        &[
            step("base", "base", "root-c"),
            step("nested-alias", "nested", "leaf"),
        ],
    );
    let addition = mapped(
        mappings,
        &[PrefabProjectionStep::InstanceAddition { entity: id("leaf") }],
    );
    for (record, target) in [
        (root_a, leaf),
        (leaf, root_a),
        (nested_b, nested_b),
        (nested_c, nested_c),
        (addition, root_a),
    ] {
        assert_eq!(
            entity(reviewed, record).components[&component()],
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone(),
                        }),
                    ),
                    ("external", ComponentValue::EntityReference(persistent())),
                    ("amount", ComponentValue::U64(7)),
                ]),
            )
            .with_owner(owner())
        );
        assert_eq!(entity(reviewed, record).components.len(), 1);
    }
    assert_eq!(entity(reviewed, root_a).children, [leaf.clone()]);
    assert_eq!(entity(reviewed, root_b).children, [nested_b.clone()]);
    assert_eq!(entity(reviewed, root_c).children, [nested_c.clone()]);
    for leaf in [leaf, nested_b, nested_c, addition] {
        assert!(entity(reviewed, leaf).children.is_empty());
    }
    for root in [root_b, root_c] {
        assert!(entity(reviewed, root).components.is_empty());
    }
    assert!(
        mappings
            .iter()
            .all(|mapping| entity(reviewed, mapping.entity()).prefab.is_none())
    );
    for old in original {
        let retained = entity(reviewed, &old.id);
        if old.id == id("A") {
            assert_eq!(retained.components, old.components);
            assert!(retained.prefab.is_none());
            assert_eq!(
                retained.children,
                [
                    root_a.clone(),
                    root_b.clone(),
                    root_c.clone(),
                    addition.clone(),
                    id("local-child"),
                    id("local-peer"),
                ]
            );
        } else {
            assert_eq!(retained, old);
        }
    }
}

fn add_later() -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("later")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

fn source_refusal(error: PrefabConversionError<LoadFailure>) -> ScenePatchReport {
    let PrefabConversionError::Source(report) = error else {
        panic!("expected a source refusal, got {error:?}")
    };
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert!(report.diagnostics.has_errors());
    report
}

struct SavedSource(PathBuf);

impl SavedSource {
    fn write(contents: &str, extension: &str) -> Self {
        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara-conversion-{}-{timestamp}-{}.{extension}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::write(&path, contents).unwrap();
        Self(path)
    }

    fn read(&self) -> String {
        fs::read_to_string(&self.0).unwrap()
    }
}

impl Drop for SavedSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

macro_rules! source_workflow {
    ($workflow:ident, $stale:ident, $dependencies:ident, $fixture:ident, $open:ident,
     $candidate:ty, $request:ty, $prepared:ty) => {
        #[test]
        fn $workflow() {
            let basis = basis();
            let original = $fixture();
            let mut session = $open(&original, &basis);
            let before = session.shared_document();
            let revision = session.revision();
            let content = session.content_state();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            assert_eq!(manifest.record_count(), 2);
            assert!(session.authoring_initial_component(&component()).is_none());

            let cancelled: $request = session
                .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                .unwrap();
            assert_eq!(cancelled.base_revision(), revision);
            assert_eq!(cancelled.anchor(), &id("A"));
            drop(cancelled);
            let request: $request = session
                .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                .unwrap();
            let worker_basis = basis.clone();
            let (cancelled, mut sources): ($prepared, Sources) = thread::spawn(move || {
                let mut sources = Sources::new(&worker_basis);
                let prepared = request
                    .prepare_with_loader(None, |request, work| sources.load(request, work))
                    .unwrap();
                (prepared, sources)
            })
            .join()
            .unwrap();
            assert_actual_reads(&sources.reads);
            assert_review(
                &original.entities,
                &cancelled.document().entities,
                cancelled.entities(),
            );
            drop(cancelled);
            assert!(Arc::ptr_eq(&before, &session.shared_document()));
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_usage().records, 0);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

            let request: $request = session
                .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                .unwrap();
            let worker = thread::spawn(move || {
                sources.reads.clear();
                let prepared = request
                    .prepare_with_loader(None, |request, work| sources.load(request, work))
                    .unwrap();
                (prepared, sources)
            });
            let mut unrelated_b = open_scene(
                &SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-b"))]),
                &basis,
            );
            let mut unrelated_c = open_prefab(
                &PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("unrelated-c"))]),
                &basis,
            );
            assert!(unrelated_b.apply_authoring_patch(&add_later()).applied);
            assert!(unrelated_c.apply_authoring_patch(&add_later()).applied);
            let (prepared, mut sources): ($prepared, Sources) = worker.join().unwrap();
            assert_eq!(prepared.base_revision(), revision);
            assert_eq!(prepared.anchor(), &id("A"));
            assert_actual_reads(&sources.reads);
            assert!(Arc::ptr_eq(&before, &session.shared_document()));
            assert_eq!(session.history_usage().records, 0);
            let reviewed = prepared.document().clone();
            let mappings = mapping_snapshot(prepared.entities());
            assert_review(&original.entities, &reviewed.entities, prepared.entities());
            assert_eq!(reviewed.roots, original.roots);

            // Reacquiring equal bodies is valid; unrelated missing/erroring sources are irrelevant.
            for name in ["base", "nested", "nested-alias"] {
                let old = &sources.documents[&source(name)];
                let rebound = ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(old.document().clone()),
                    basis.clone(),
                    old.key().clone(),
                );
                sources.documents.insert(source(name), rebound);
            }
            sources.fail = Some(source("unrelated-c"));
            sources.reads.clear();
            let receipt: PrefabConversionReceipt = session
                .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                    sources.resolve(request)
                })
                .unwrap();
            assert!(
                receipt.report().applied,
                "{:?}",
                receipt.report().diagnostics
            );
            assert_eq!(mapping_snapshot(receipt.entities()), mappings);
            assert_eq!(session.document(), &reviewed);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(session.history_status().redo_depth, 0);
            assert_eq!(session.history_usage().records, 1);
            assert_actual_reads(&sources.reads);
            let converted_manifest = session.unavailable_record_manifest().unwrap().clone();
            // Projected roots now precede the preserved local child. Its raw record stays
            // exact, while its manifest placement must describe that new predecessor.
            let expected = $open(&reviewed, &basis);
            assert_eq!(
                converted_manifest.entries(),
                expected.unavailable_record_manifest().unwrap().entries(),
            );
            assert_eq!(
                converted_manifest
                    .overlay_declarations()
                    .collect::<Vec<_>>(),
                manifest
                    .overlay_declarations()
                    .filter(|(anchor, _)| *anchor != &id("A"))
                    .collect::<Vec<_>>()
            );
            let converted_content = session.content_state();
            drop(sources);
            let undo = session.undo_authoring();
            assert!(undo.applied, "{:?}", undo.diagnostics);
            assert_eq!(session.document(), &original);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert_eq!(session.history_status().undo_depth, 0);
            assert_eq!(session.history_status().redo_depth, 1);
            let redo = session.redo_authoring();
            assert!(redo.applied, "{:?}", redo.diagnostics);
            assert_eq!(session.document(), &reviewed);
            assert_eq!(session.content_state(), converted_content);
            assert_eq!(session.history_usage().records, 1);

            for (extension, encoded) in [
                ("json", reviewed.to_json_string().unwrap()),
                ("ron", reviewed.to_ron_string().unwrap()),
            ] {
                let saved = SavedSource::write(&encoded, extension);
                let decoded = if extension == "json" {
                    <$candidate>::decode_json_str(&saved.read())
                } else {
                    <$candidate>::decode_ron_str(&saved.read())
                }
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(decoded).unwrap();
                assert_eq!(reopened.document(), &reviewed);
                assert_eq!(reopened.authoring_schema_basis(), Some(basis.clone()));
                assert_ne!(
                    reopened.revision().source_id(),
                    session.revision().source_id()
                );
                assert_eq!(reopened.history_usage().records, 0);
                assert_eq!(
                    reopened.unavailable_record_manifest(),
                    Some(&converted_manifest)
                );
            }
        }

        #[test]
        fn $stale() {
            let basis = basis();
            let mut session = $open(&$fixture(), &basis);
            let revision = session.revision();
            let request: $request = session
                .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                .unwrap();
            assert!(session.apply_authoring_patch(&add_later()).applied);
            assert!(
                session
                    .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                    .is_err()
            );
            let worker_basis = basis.clone();
            let prepared: $prepared = thread::spawn(move || {
                let mut sources = Sources::new(&worker_basis);
                request
                    .prepare_with_loader(None, |request, work| sources.load(request, work))
                    .unwrap()
            })
            .join()
            .unwrap();
            assert_eq!(prepared.base_revision(), revision);
            assert!(
                prepared
                    .document()
                    .entities
                    .iter()
                    .all(|record| record.id != id("later"))
            );
            let accepted = session.shared_document();
            let accepted_revision = session.revision();
            let content = session.content_state();
            let history = session.history_status();
            let usage = session.history_usage();
            let report = source_refusal(
                session
                    .commit_complete_prefab_conversion_with_loader(
                        prepared,
                        |_| -> Result<Option<ResolvedPrefabSource>, LoadFailure> {
                            panic!("stale roots must reject before dependency acquisition")
                        },
                    )
                    .unwrap_err(),
            );
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authoring-edit-stale-basis"
            }));
            assert!(Arc::ptr_eq(&accepted, &session.shared_document()));
            assert_eq!(session.revision(), accepted_revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
            assert_eq!(session.history_usage(), usage);

            for (extension, encoded) in [
                ("json", accepted.to_json_string().unwrap()),
                ("ron", accepted.to_ron_string().unwrap()),
            ] {
                let request = session
                    .request_complete_prefab_conversion(
                        accepted_revision,
                        &id("A"),
                        Default::default(),
                    )
                    .unwrap();
                let mut sources = Sources::new(&basis);
                let prepared = request
                    .prepare_with_loader(None, |request, work| sources.load(request, work))
                    .unwrap();
                let saved = SavedSource::write(&encoded, extension);
                let candidate = if extension == "json" {
                    <$candidate>::decode_json_str(&saved.read())
                } else {
                    <$candidate>::decode_ron_str(&saved.read())
                }
                .unwrap();
                let mut reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                let reopened_revision = reopened.revision();
                let reopened_content = reopened.content_state();
                assert_eq!(
                    reopened.authoring_schema_basis(),
                    session.authoring_schema_basis()
                );
                assert_ne!(reopened_revision.source_id(), accepted_revision.source_id());
                let report = source_refusal(
                    reopened
                        .commit_complete_prefab_conversion_with_loader(
                            prepared,
                            |_| -> Result<Option<ResolvedPrefabSource>, LoadFailure> {
                                panic!("reopened sources must reject before dependency acquisition")
                            },
                        )
                        .unwrap_err(),
                );
                assert!(report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.authoring-edit-stale-basis"
                }));
                assert_eq!(reopened.document(), accepted.as_ref());
                assert_eq!(reopened.revision(), reopened_revision);
                assert_eq!(reopened.content_state(), reopened_content);
                assert_eq!(reopened.history_usage().records, 0);
            }
        }

        #[test]
        fn $dependencies() {
            for dependency in ["base", "nested", "nested-alias"] {
                for change in [
                    DependencyChange::Key,
                    DependencyChange::Body,
                    DependencyChange::Missing,
                    DependencyChange::Loader,
                    DependencyChange::Schema,
                ] {
                    let basis = basis();
                    let mut session = $open(&$fixture(), &basis);
                    let before = session.shared_document();
                    let revision = session.revision();
                    let content = session.content_state();
                    let manifest = session.unavailable_record_manifest().unwrap().clone();
                    let history = session.history_status();
                    let usage = session.history_usage();
                    let mut sources = Sources::new(&basis);
                    let prepared = session
                        .request_complete_prefab_conversion(revision, &id("A"), Default::default())
                        .unwrap()
                        .prepare_with_loader(None, |request, work| sources.load(request, work))
                        .unwrap();
                    change_dependency(&mut sources, dependency, change, &basis);
                    sources.reads.clear();
                    let error = session
                        .commit_complete_prefab_conversion_with_loader(prepared, |request| {
                            sources.resolve(request)
                        })
                        .expect_err("every actual dependency, including aliases, must be fenced");
                    if matches!(change, DependencyChange::Loader) {
                        assert!(matches!(
                            error,
                            PrefabConversionError::Loader(LoadFailure("offline"))
                        ));
                    } else {
                        let report = source_refusal(error);
                        assert!(
                            report.diagnostics.iter().any(|diagnostic| {
                                diagnostic.code().as_str()
                                    == "scene.authoring-prefab-conversion-source-changed"
                            }),
                            "{dependency}: {change:?}: {:?}",
                            report.diagnostics
                        );
                    }
                    assert!(
                        sources.reads.contains(&source(dependency)),
                        "{dependency}: {change:?}"
                    );
                    assert!(Arc::ptr_eq(&before, &session.shared_document()));
                    assert_eq!(session.revision(), revision);
                    assert_eq!(session.content_state(), content);
                    assert_eq!(session.history_status(), history);
                    assert_eq!(session.history_usage(), usage);
                    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy)]
enum DependencyChange {
    Key,
    Body,
    Missing,
    Loader,
    Schema,
}

fn change_dependency(
    sources: &mut Sources,
    name: &str,
    change: DependencyChange,
    original_basis: &AuthoringSchemaBasis,
) {
    let request = source(name);
    let previous = sources.documents[&request].clone();
    match change {
        DependencyChange::Missing => {
            sources.documents.remove(&request);
        }
        DependencyChange::Loader => sources.fail = Some(request),
        DependencyChange::Key | DependencyChange::Body | DependencyChange::Schema => {
            let mut document = previous.document().clone();
            if matches!(change, DependencyChange::Body) {
                document
                    .entities
                    .iter_mut()
                    .find_map(|record| record.components.get_mut(&component()))
                    .unwrap()
                    .value
                    .replace_path(
                        &ComponentFieldPath::from_fields(["amount"]),
                        ComponentValue::U64(99),
                    )
                    .unwrap();
            }
            let basis = if matches!(change, DependencyChange::Schema) {
                let independently_admitted = basis();
                assert_ne!(&independently_admitted, original_basis);
                independently_admitted
            } else {
                original_basis.clone()
            };
            let key = if matches!(change, DependencyChange::Key) {
                prefab_source::key("rebound")
            } else {
                previous.key().clone()
            };
            sources.documents.insert(
                request,
                ResolvedPrefabSource::bound_authoring_source(Arc::new(document), basis, key),
            );
        }
    }
}

source_workflow!(
    scene_conversion_reviews_cancels_commits_replays_and_reopens_complete_source,
    scene_conversion_request_keeps_its_snapshot_and_rejects_changed_or_reopened_roots,
    scene_conversion_rejects_changed_actual_dependencies_without_partial_publication,
    scene_document,
    open_scene,
    SceneDocumentCandidate,
    ScenePrefabConversionRequest,
    PreparedScenePrefabConversion
);

source_workflow!(
    prefab_conversion_reviews_cancels_commits_replays_and_reopens_complete_source,
    prefab_conversion_request_keeps_its_snapshot_and_rejects_changed_or_reopened_roots,
    prefab_conversion_rejects_changed_actual_dependencies_without_partial_publication,
    prefab_document,
    open_prefab,
    PrefabDocumentCandidate,
    PrefabConversionRequest,
    PreparedPrefabConversion
);

#[test]
fn unavailable_selected_records_and_acquisition_errors_refuse_before_source_publication() {
    let basis = basis();
    let mut session = open_scene(&scene_document(), &basis);
    let before = session.shared_document();
    let revision = session.revision();
    let content = session.content_state();
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let mut sources = Sources::new(&basis);
    let mut future = sources.documents[&source("nested")].document().clone();
    future.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .version = ComponentSchemaVersion(2);
    let unavailable = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(future),
        basis.clone(),
        prefab_source::key("nested"),
    );
    sources
        .documents
        .insert(source("nested"), unavailable.clone());
    sources
        .documents
        .insert(source("nested-alias"), unavailable);
    let error = session
        .request_complete_prefab_conversion(revision, &id("A"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |request, work| sources.load(request, work))
        .unwrap_err();
    source_refusal(error);

    let error = session
        .request_complete_prefab_conversion(revision, &id("A"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |_, _| {
            Err::<Option<ResolvedPrefabSource>, _>(LoadFailure("offline"))
        })
        .unwrap_err();
    assert!(matches!(
        error,
        PrefabConversionError::Loader(LoadFailure("offline"))
    ));
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.history_usage().records, 0);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(!session.undo_authoring().applied);
}
