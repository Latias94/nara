//! Real immutable input transfers, Product-associated sources and cumulative preparation.

use nara_asset::{
    AssetProductRef, AssetSourceKind, AssetSourcePath, ImportedProductId, StableAssetId,
};
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

use super::*;
use crate::{
    __private::{AuthoringAdapterReceipt, PrefabExtractionBinding, PrefabExtractionEnvironment},
    AuthoringRecordReadiness, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabDeclarationSource, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabOverlay, PrefabOverlaySlot, SceneCompilationLimits, SceneComponentRecord,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "1324edbc-e519-45f6-9d7f-bb878998814d".parse().unwrap(),
        AssetSourcePath::new(format!("content/{value}.ron")).unwrap(),
        kind,
    )
}

fn metadata(value: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(format!("content/{value}.ron")).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn p_metadata() -> AssetRecord {
    metadata("p", "3603fe5d-4b25-4073-8874-00c36bac3e73")
}
fn q_metadata() -> AssetRecord {
    metadata("q", "c7d79ab6-d16c-4b7f-8185-b8c4a2db865a")
}
fn path(value: &str) -> AssetRef {
    AssetRef::path(format!("templates/{value}.ron")).unwrap()
}
fn product(stable: StableAssetId) -> AssetRef {
    AssetRef::Product(AssetProductRef::new(stable, ImportedProductId::PRIMARY).unwrap())
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.CorpusLink")
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.corpus").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let schema = ComponentSchema::new(component(), "Corpus link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
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

fn local(value: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(value) }
}

fn reference(anchor: &str, source: &str, entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: key(source, SourceDocumentKind::Prefab),
            }],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn link(reference: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(reference))]),
    )
    .with_owner(owner())
}

fn original_prefab() -> PrefabDocument {
    PrefabDocument {
        // An unused declared parameter must survive without creating a surrogate instance.
        inputs: [ReferenceInputId::new("unused-original-input").unwrap()].into(),
        placements: vec![
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::root(id("x")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
        ],
        entities: vec![
            SceneEntityRecord::new(id("k")).with_component(component(), link(local("b"))),
            SceneEntityRecord::new(id("x")).with_component(component(), link(local("b"))),
            SceneEntityRecord::new(id("b")).with_component(component(), link(local("x"))),
        ],
    }
}

fn instance(entity: &str, source: AssetRef) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(entity));
    entity.prefab = Some(PrefabInstance::forest(source));
    entity
}

fn binding(reference: AssetRef, source: &str) -> PrefabExtractionBinding {
    PrefabExtractionBinding {
        reference,
        source: key(source, SourceDocumentKind::Prefab),
    }
}

fn environment(
    source_count: usize,
    before: Vec<PrefabExtractionBinding>,
    destination: &ExtractionDestination,
) -> PrefabExtractionEnvironment {
    let mut after: Vec<_> = before
        .iter()
        .map(|binding| PrefabExtractionBinding {
            reference: binding.reference.clone(),
            source: binding.source.clone(),
        })
        .collect();
    after.push(PrefabExtractionBinding {
        reference: destination.reference.clone(),
        source: destination.key.clone(),
    });
    PrefabExtractionEnvironment {
        name: "corpus-fixture".into(),
        sources: (0..source_count).collect(),
        before,
        after,
        assets_before: None,
        assets_after: None,
    }
}

fn append_source(
    request: &mut ExtractionRequest,
    source: CapturedExtractionSource,
    references: &[AssetRef],
) {
    let environment = &mut request.environments[0];
    environment.sources.push(request.sources.len());
    for reference in references {
        let binding = PrefabExtractionBinding {
            reference: reference.clone(),
            source: source.key().clone(),
        };
        environment.before.push(PrefabExtractionBinding {
            reference: binding.reference.clone(),
            source: binding.source.clone(),
        });
        environment.after.push(binding);
    }
    request.sources.push(source);
}

fn request() -> ExtractionRequest {
    let basis = basis();
    let p_metadata = p_metadata();
    let p = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(original_prefab()),
        basis.clone(),
        key("p", SourceDocumentKind::Prefab),
    )
    .with_source_asset(&p_metadata)
    .unwrap();
    let r = PrefabDocument::from_ordered_roots([instance("r", product(p_metadata.stable_id()))
        .with_component(component(), link(reference("r", "p", "b")))]);
    let main = SceneDocument::from_ordered_roots([
        instance("nested", path("r")),
        instance("repeated", AssetRef::StableId(p_metadata.stable_id())),
    ]);
    let inactive = SceneDocument::from_ordered_roots([
        instance("inactive", path("p")),
        SceneEntityRecord::new(id("local"))
            .with_component(component(), link(reference("inactive", "p", "b"))),
    ]);
    let destination = ExtractionDestination {
        key: key("q", SourceDocumentKind::Prefab),
        reference: product(q_metadata().stable_id()),
        metadata: q_metadata(),
    };
    let environment = environment(
        4,
        vec![
            binding(path("p"), "p"),
            binding(AssetRef::StableId(p_metadata.stable_id()), "p"),
            binding(product(p_metadata.stable_id()), "p"),
            binding(path("r"), "r"),
        ],
        &destination,
    );
    ExtractionRequest {
        sources: vec![
            CapturedExtractionSource::Scene {
                key: key("main", SourceDocumentKind::Scene),
                document: Arc::new(main),
                basis: basis.clone(),
            },
            CapturedExtractionSource::Prefab { source: p },
            CapturedExtractionSource::Scene {
                key: key("inactive", SourceDocumentKind::Scene),
                document: Arc::new(inactive),
                basis: basis.clone(),
            },
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(r),
                    basis.clone(),
                    key("r", SourceDocumentKind::Prefab),
                ),
            },
        ],
        environments: vec![environment],
        selected_environment: 0,
        basis,
        selected: key("p", SourceDocumentKind::Prefab),
        root: id("k"),
        destination,
        max_depth: 32,
    }
}

fn prepare(request: ExtractionRequest, limits: SceneCompilationLimits) -> PreparedExtractionCorpus {
    request
        .prepare(ExtractionWork::new(limits, Some(0)).unwrap())
        .unwrap_or_else(|failure| panic!("{:?}", failure.error))
}

#[test]
fn real_worker_prepares_repeated_nested_and_inactive_sources_without_losing_associations() {
    let request = request();
    let CapturedExtractionSource::Scene {
        document: original_main,
        ..
    } = &request.sources[0]
    else {
        panic!("Scene")
    };
    let main_allocation = Arc::clone(original_main);
    let CapturedExtractionSource::Prefab {
        source: original_p, ..
    } = &request.sources[1]
    else {
        panic!("Prefab")
    };
    let old_p = original_p.document().clone();
    let prepared = std::thread::spawn(move || prepare(request, Default::default()))
        .join()
        .unwrap();
    let CapturedExtractionSource::Scene {
        document: retained_main,
        ..
    } = &prepared.original().sources[0]
    else {
        panic!("Scene")
    };
    assert!(Arc::ptr_eq(&main_allocation, retained_main));
    let CapturedExtractionSource::Prefab {
        source: retained_p, ..
    } = &prepared.original().sources[1]
    else {
        panic!("Prefab")
    };
    assert_eq!(retained_p.document(), &old_p);
    assert_eq!(prepared.sources().len(), 4);
    assert_eq!(prepared.occurrences().len(), 2);
    assert_eq!(
        prepared
            .occurrences()
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(
        prepared
            .occurrences()
            .iter()
            .all(|source| source.entities.len() >= 5)
    );
    let CapturedExtractionSource::Prefab { source: new_p } = &prepared.sources()[1] else {
        panic!("Prefab")
    };
    let environment = &prepared.original().environments[0];
    assert_eq!(
        environment
            .before
            .iter()
            .filter(|binding| &binding.source == new_p.key())
            .map(|binding| &binding.reference)
            .collect::<Vec<_>>(),
        [
            &path("p"),
            &AssetRef::StableId(p_metadata().stable_id()),
            &product(p_metadata().stable_id()),
        ]
    );
    assert!(new_p.matches_request(&product(p_metadata().stable_id())));
    assert_eq!(new_p.key(), &key("p", SourceDocumentKind::Prefab));
    assert!(
        new_p
            .document()
            .inputs
            .contains(&ReferenceInputId::new("unused-original-input").unwrap())
    );
    assert_eq!(
        prepared.extracted().key(),
        &key("q", SourceDocumentKind::Prefab)
    );
    assert!(
        prepared
            .extracted()
            .matches_request(&product(q_metadata().stable_id()))
    );
    assert_eq!(prepared.extracted().document().entities.len(), 2);
    assert_eq!(prepared.extracted().document().inputs.len(), 1);
    for (position, record) in old_p.entities.iter().enumerate() {
        let expected = if record.id == id("x") {
            key("p", SourceDocumentKind::Prefab)
        } else {
            key("q", SourceDocumentKind::Prefab)
        };
        let mapped = prepared.record(1, position).unwrap();
        assert_eq!(mapped.source, &expected);
        assert_eq!(mapped.before.id, mapped.after.id);
    }
    assert!(
        old_p
            .placements
            .iter()
            .enumerate()
            .all(|(position, _)| prepared.placement_destination(1, position).is_some())
    );
    assert!(
        prepared
            .placement_destination(1, old_p.placements.len())
            .is_none()
    );
    assert!(prepared.record(1, old_p.entities.len()).is_none());
    assert!(prepared.usage().0 > 0 && prepared.usage().1 > 0);
}

#[test]
fn refusal_returns_the_same_source_allocations_and_the_spent_allowance() {
    let mut request = request();
    let CapturedExtractionSource::Scene { document, .. } = &mut request.sources[2] else {
        panic!("Scene")
    };
    let mut broken = document.as_ref().clone();
    broken
        .entities
        .push(instance("missing", path("not-captured")));
    broken
        .placements
        .push(SceneEntityPlacement::root(id("missing")));
    *document = Arc::new(broken);
    let original_allocation = Arc::clone(document);
    let failure = match request.prepare(ExtractionWork::new(Default::default(), Some(0)).unwrap()) {
        Ok(_) => panic!("missing active source must refuse"),
        Err(failure) => failure,
    };
    let ExtractionError::Compilation { diagnostics, .. } = &failure.error else {
        panic!("{:?}", failure.error)
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-source-missing")
    );
    assert_eq!(failure.environment, Some(0));
    let CapturedExtractionSource::Scene { document, .. } = &failure.request.sources[2] else {
        panic!("Scene")
    };
    assert!(Arc::ptr_eq(document, &original_allocation));
    assert!(failure.work.usage().0 > 0 && failure.work.usage().1 > 0);
}

#[test]
fn one_cumulative_allowance_covers_all_compilers_and_correspondence() {
    let baseline = prepare(request(), Default::default());
    let usage = baseline.usage();
    let limits = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(usage.1).unwrap())
        .with_reference_work_nodes(ItemLimit::new(usage.0).unwrap());
    assert_eq!(prepare(request(), limits).usage(), usage);
    for limits in [
        limits.with_working_bytes(ByteLimit::new(usage.1 - 1).unwrap()),
        limits.with_reference_work_nodes(ItemLimit::new(usage.0 - 1).unwrap()),
    ] {
        let failure = match request().prepare(ExtractionWork::new(limits, Some(0)).unwrap()) {
            Ok(_) => panic!("whole-corpus one-short capacity must refuse"),
            Err(failure) => failure,
        };
        assert!(
            matches!(failure.error, ExtractionError::Capacity),
            "{:?}",
            failure.error
        );
        assert!(failure.work.usage().1 > 0);
    }
}

#[test]
fn canonical_alias_and_nominal_authority_collisions_refuse_without_recapture() {
    for case in 0..3 {
        let mut request = request();
        match case {
            0 => request.destination.key = request.selected.clone(),
            1 => {
                request.environments[0].before.push(binding(path("p"), "r"));
                request.environments[0].after.push(binding(path("p"), "r"));
            }
            _ => {
                let CapturedExtractionSource::Scene { basis: actual, .. } = &mut request.sources[0]
                else {
                    panic!("Scene")
                };
                *actual = basis(); // Equal declarations do not replace the original nominal authority.
            }
        }
        let CapturedExtractionSource::Scene { document, .. } = &request.sources[0] else {
            panic!("Scene")
        };
        let original = Arc::clone(document);
        let failure =
            match request.prepare(ExtractionWork::new(Default::default(), Some(0)).unwrap()) {
                Ok(_) => panic!("collision must refuse"),
                Err(failure) => failure,
            };
        assert!(matches!(failure.error, ExtractionError::InvalidSource));
        let CapturedExtractionSource::Scene { document, .. } = &failure.request.sources[0] else {
            panic!("Scene")
        };
        assert!(Arc::ptr_eq(&original, document));
    }
}

fn unknown_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "retained",
            ComponentValue::String("unavailable complete source data".into()),
        )]),
    )
}

#[test]
fn independent_and_root_bound_unavailable_sources_keep_their_actual_physical_owner() {
    let mut request = request();
    let unknown = ComponentTypeId::new("missing.IndependentComponent");
    let future = SceneComponentRecord::new(
        ComponentSchemaVersion::new(7).unwrap(),
        ComponentValue::map([("target", ComponentValue::EntityReference(local("u")))]),
    )
    .with_owner(owner());
    let wrong_owner = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(local("u")))]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("game.another-owner").unwrap());
    let u = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("u")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("future")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("u"),
                ))),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("owner")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("u"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("u")).with_component(unknown.clone(), unknown_record()),
            SceneEntityRecord::new(id("future")).with_component(component(), future),
            SceneEntityRecord::new(id("owner")).with_component(component(), wrong_owner),
        ],
    ));
    let mut p = match request.sources[1].document() {
        SourceDocumentInput::Prefab(p) => p.clone(),
        _ => panic!("Prefab"),
    };
    p.entities
        .iter_mut()
        .find(|record| record.id == id("k"))
        .unwrap()
        .prefab = Some(PrefabInstance::root(
        path("u"),
        key("u", SourceDocumentKind::Prefab),
        PrefabEntityTarget::source(id("u")),
    ));
    let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[1] else {
        panic!("Prefab")
    };
    *source = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(p),
        request.basis.clone(),
        key("p", SourceDocumentKind::Prefab),
    )
    .with_source_asset(&p_metadata())
    .unwrap();
    let source = ResolvedPrefabSource::bound_authoring_source(
        Arc::clone(&u),
        request.basis.clone(),
        key("u", SourceDocumentKind::Prefab),
    );
    append_source(
        &mut request,
        CapturedExtractionSource::Prefab { source },
        &[path("u")],
    );
    let independent = Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        id("independent"),
    )
    .with_component(unknown.clone(), unknown_record())]));
    let source = CapturedExtractionSource::Scene {
        key: key("independent", SourceDocumentKind::Scene),
        document: Arc::clone(&independent),
        basis: request.basis.clone(),
    };
    append_source(&mut request, source, &[]);
    let prepared = prepare(request, Default::default());
    let CapturedExtractionSource::Prefab { source, .. } = &prepared.sources()[4] else {
        panic!("Prefab")
    };
    assert!(std::ptr::eq(source.document(), u.as_ref()));
    assert_eq!(source.document().entities.len(), 3);
    assert_eq!(
        source
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("u"))
            .unwrap()
            .components[&unknown],
        unknown_record()
    );
    let CapturedExtractionSource::Scene { document, .. } = &prepared.sources()[5] else {
        panic!("Scene")
    };
    assert!(Arc::ptr_eq(document, &independent));
    assert_eq!(prepared.occurrences().len(), 3);
    let SourceDocumentInput::Prefab(p) = prepared.original().sources[1].document() else {
        panic!("Prefab")
    };
    let old_root = p
        .entities
        .iter()
        .position(|record| record.id == id("k"))
        .unwrap();
    assert_eq!(
        prepared.record(1, old_root).unwrap().source,
        &key("q", SourceDocumentKind::Prefab)
    );
    // U's unavailable root contributes to P:k and then P:k/Q:k, without acquiring either
    // physical declaration owner. Actual compiler-selected origins prove its preservation.
    let SourceDocumentInput::Prefab(original_u) = prepared.original().sources[4].document() else {
        panic!("Prefab")
    };
    for index in 0..original_u.entities.len() {
        let mapped = prepared.record(4, index).unwrap();
        assert_eq!(mapped.source, &key("u", SourceDocumentKind::Prefab));
        assert!(std::ptr::eq(mapped.before, mapped.after));
    }
}

#[test]
fn unavailable_payloads_in_moved_retained_and_dependent_sources_remain_refusals() {
    for (source_index, entity) in [(1, "k"), (1, "x"), (3, "r")] {
        let mut request = request();
        let mut changed = match request.sources[source_index].document() {
            SourceDocumentInput::Prefab(document) => document.clone(),
            _ => panic!("Prefab"),
        };
        changed
            .entities
            .iter_mut()
            .find(|record| record.id == id(entity))
            .unwrap()
            .components
            .insert(ComponentTypeId::new("missing.Affected"), unknown_record());
        let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[source_index]
        else {
            panic!("Prefab")
        };
        source.document = crate::prefab::ResolvedPrefabDocument::AuthoringSource {
            document: Arc::new(changed),
            basis: request.basis.clone(),
        };
        let original = source.document();
        let original_ptr = original as *const PrefabDocument;
        let failure =
            match request.prepare(ExtractionWork::new(Default::default(), Some(0)).unwrap()) {
                Ok(_) => panic!("affected unknown meaning must refuse"),
                Err(failure) => failure,
            };
        assert!(
            matches!(failure.error, ExtractionError::UnknownSchema),
            "{:?}",
            failure.error
        );
        let SourceDocumentInput::Prefab(retained) =
            failure.request.sources[source_index].document()
        else {
            panic!("Prefab")
        };
        assert_eq!(retained as *const PrefabDocument, original_ptr);
    }
}

#[test]
fn an_outer_claim_entering_p_invalidates_the_unavailable_records_actual_lexical_context() {
    let mut request = request();
    let target = reference("x", "p", "b");
    let known = link(target.clone());
    let unavailable = link(target).with_owner(
        PersistentComponentSchemaOwnerId::try_new("game.temporarily-unavailable-owner").unwrap(),
    );
    let u = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("u")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("x")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("u"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("u")).with_component(component(), known),
            SceneEntityRecord::new(id("x")),
        ],
    );
    append_prefab(&mut request, "u", u);
    let mut instance = PrefabInstance::forest(path("u"));
    let mut overlay = crate::PrefabOverlay::new(key("u", SourceDocumentKind::Prefab));
    overlay.instances.push(crate::PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("x")),
        value: Some(PrefabInstance::forest(path("p"))),
    });
    instance.overrides = Some(overlay);
    let mut anchor = SceneEntityRecord::new(id("outer"));
    anchor.prefab = Some(instance);
    let scene = SceneDocument::from_ordered_roots([anchor]);
    let CapturedExtractionSource::Scene { document, .. } = &mut request.sources[0] else {
        panic!("Scene")
    };
    *document = Arc::new(scene);
    // Under its understood definition this is a real valid reference, not inert test bytes.
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    let lookup = CorpusLookup::new(&request, &mut work, &mut None).unwrap();
    let SourceDocumentInput::Scene(scene) = request.sources[0].document() else {
        panic!("Scene")
    };
    let control = compile_scene(
        scene,
        &request,
        &request.sources,
        &lookup,
        0,
        None,
        &mut work,
        None,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(control.unavailable.is_empty());
    assert!(!control.document.entities().is_empty());
    let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[4] else {
        panic!("Prefab")
    };
    let mut u = source.document().clone();
    u.entities
        .iter_mut()
        .find(|entity| entity.id == id("u"))
        .unwrap()
        .components
        .insert(component(), unavailable);
    source.document = crate::prefab::ResolvedPrefabDocument::AuthoringSource {
        document: Arc::new(u),
        basis: request.basis.clone(),
    };
    // Physical U has no dependency on P. This occurrence does, because the containing
    // source replaces U:x. Preserving U's source allocation alone cannot prove its context.
    let result = request.prepare(ExtractionWork::new(Default::default(), Some(0)).unwrap());
    assert!(
        matches!(
            result,
            Err(ExtractionPreparationFailure {
                error: ExtractionError::OwnerMismatch,
                ..
            })
        ),
        "actual refusal: {:?}",
        result.as_ref().err().map(|failure| &failure.error)
    );
}

#[test]
fn preserved_contexts_still_rewrite_understood_dormant_references() {
    let mut request = request();
    let unknown = ComponentTypeId::new("missing.UnrelatedRecord");
    let original = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("dormant"))
            .with_component(component(), link(reference("x", "p", "b")))
            .with_component(unknown.clone(), unknown_record()),
        SceneEntityRecord::new(id("x")),
    ]));
    let source = ResolvedPrefabSource::bound_authoring_source(
        Arc::clone(&original),
        request.basis.clone(),
        key("closed", SourceDocumentKind::Prefab),
    );
    append_source(
        &mut request,
        CapturedExtractionSource::Prefab { source },
        &[path("closed")],
    );
    let prepared = prepare(request, Default::default());
    let CapturedExtractionSource::Prefab { source, .. } = &prepared.sources()[4] else {
        panic!("Prefab")
    };
    let entity = source
        .document()
        .entities
        .iter()
        .find(|record| record.id == id("dormant"))
        .unwrap();
    let ComponentValue::EntityReference(EntityReference::Authored { reference }) = &entity
        .components[&component()]
        .value
        .field("target")
        .unwrap()
    else {
        panic!("authored reference")
    };
    assert_eq!(reference.through.len(), 2);
    assert_eq!(
        reference.through[0].expected_base,
        key("p", SourceDocumentKind::Prefab)
    );
    assert_eq!(
        reference.through[1].expected_base,
        key("q", SourceDocumentKind::Prefab)
    );
    assert_eq!(
        reference.through[1].anchor,
        PrefabEntityOrigin::SourceEntity(id("k"))
    );
    assert_eq!(entity.components[&unknown], unknown_record());
    assert!(!std::ptr::eq(source.document(), original.as_ref()));
    let mapped = prepared.record(4, 0).unwrap();
    assert_eq!(mapped.source, &key("closed", SourceDocumentKind::Prefab));
}

fn unavailable_overlay_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::new(7).unwrap(),
        ComponentValue::map([
            (
                "retained",
                ComponentValue::String("complete unavailable overlay data".into()),
            ),
            (
                "uninterpreted-reference",
                ComponentValue::EntityReference(reference("x", "p", "b")),
            ),
        ]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("missing.overlay-owner").unwrap())
    .with_contextual_default(
        ComponentFieldId::new("hidden"),
        ComponentValue::List(vec![
            ComponentValue::U64(41),
            ComponentValue::String("kept".into()),
        ]),
    )
}

fn overlay_record_instance(
    slot: PrefabOverlaySlot,
    component: ComponentTypeId,
    record: SceneComponentRecord,
) -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(key("v", SourceDocumentKind::Prefab));
    match slot {
        PrefabOverlaySlot::Addition { index: 0 } => {
            let mut addition = PrefabAddition::new(id("extra"));
            addition.components.insert(component, record);
            overlay.additions.push(addition);
            overlay.placements.push(SceneEntityPlacement::new(
                PrefabEntityTarget::addition(id("extra")),
                ScenePlacement::last(ScenePlacementParent::Root),
            ));
        }
        PrefabOverlaySlot::Component { index: 0 } => {
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: PrefabEntityTarget::source(id("v")),
                component,
                record,
            });
        }
        _ => panic!("fixture requires its only addition or complete Set"),
    }
    let mut instance = PrefabInstance::forest(path("v"));
    instance.overrides = Some(overlay);
    instance
}

fn append_prefab(request: &mut ExtractionRequest, name: &str, document: PrefabDocument) {
    let source = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(document),
        request.basis.clone(),
        key(name, SourceDocumentKind::Prefab),
    );
    append_source(
        request,
        CapturedExtractionSource::Prefab { source },
        &[path(name)],
    );
}

fn activate_independent_u(request: &mut ExtractionRequest) {
    let CapturedExtractionSource::Scene { document, .. } = &mut request.sources[0] else {
        panic!("Scene")
    };
    let scene = Arc::make_mut(document);
    scene.entities.push(instance("independent", path("u")));
    scene
        .placements
        .push(SceneEntityPlacement::root(id("independent")));
}

fn overlay_contributor_request(
    slot: PrefabOverlaySlot,
    component: ComponentTypeId,
    record: SceneComponentRecord,
) -> ExtractionRequest {
    let mut request = request();
    let mut u = SceneEntityRecord::new(id("u"));
    u.prefab = Some(overlay_record_instance(slot, component, record));
    append_prefab(&mut request, "u", PrefabDocument::from_ordered_roots([u]));
    append_prefab(
        &mut request,
        "v",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("v"))]),
    );
    activate_independent_u(&mut request);
    request
}

fn assert_preserved_overlay_contributor(
    prepared: &PreparedExtractionCorpus,
    scene_index: usize,
    source_index: usize,
    record_index: usize,
    path: &PrefabInstancePath,
    slot: PrefabOverlaySlot,
    component: &ComponentTypeId,
) {
    let original = prepared.original();
    let source_key = original.sources[source_index].key();
    let mapped = prepared.record(source_index, record_index).unwrap();
    assert_eq!(mapped.source, source_key);
    let instance = mapped.instance(path).unwrap();
    assert_eq!(instance.source, source_key);
    let overlay = instance.intent(slot).unwrap();
    assert_eq!(overlay.source, source_key);
    assert_eq!(overlay.slot, slot);
    let (before_record, after_record) = match slot {
        PrefabOverlaySlot::Addition { index } => {
            assert_eq!(
                overlay.before.additions[index],
                overlay.after.additions[index]
            );
            (
                &overlay.before.additions[index].components[component],
                &overlay.after.additions[index].components[component],
            )
        }
        PrefabOverlaySlot::Component { index } => {
            assert_eq!(
                overlay.before.components[index],
                overlay.after.components[index]
            );
            let PrefabComponentIntent::Set {
                component: before,
                record: old,
                ..
            } = &overlay.before.components[index]
            else {
                panic!("complete Set")
            };
            let PrefabComponentIntent::Set {
                component: after,
                record: new,
                ..
            } = &overlay.after.components[index]
            else {
                panic!("complete Set")
            };
            assert_eq!(before, component);
            assert_eq!(after, component);
            (old, new)
        }
        _ => panic!("unavailable complete record slot"),
    };
    assert_eq!(before_record, &unavailable_overlay_record());
    assert_eq!(before_record, after_record);
    if !std::ptr::eq(mapped.before, mapped.after) {
        assert!(!std::ptr::eq(before_record, after_record));
    }

    // Observe both real compiler outputs as well as the physical declaration map. V's
    // materialized entity and the containing Scene are not U's declaration owner.
    let mut work = ExtractionWork::new(Default::default(), Some(0)).unwrap();
    let lookup = CorpusLookup::new(original, &mut work, &mut None).unwrap();
    let SourceDocumentInput::Scene(before) = original.sources[scene_index].document() else {
        panic!("Scene")
    };
    let SourceDocumentInput::Scene(after) = prepared.sources()[scene_index].document() else {
        panic!("Scene")
    };
    let before = compile_scene(
        before,
        original,
        &original.sources,
        &lookup,
        0,
        None,
        &mut work,
        None,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    let after = compile_scene(
        after,
        original,
        prepared.sources(),
        &lookup,
        0,
        Some(prepared.extracted()),
        &mut work,
        None,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    let expected_source = if scene_index == source_index {
        PrefabDeclarationSource::Containing
    } else {
        PrefabDeclarationSource::Prefab(source_key.clone())
    };
    for observation in [&before, &after] {
        assert_eq!(observation.unavailable.len(), 1);
        let declaration = &observation.unavailable[0];
        assert_eq!(declaration.source, expected_source);
        assert_eq!(declaration.record, path.anchor);
        assert_eq!(&declaration.component, component);
        let crate::prefab::UnavailableComponentSlot::Overlay {
            steps,
            slot: observed_slot,
        } = &declaration.slot
        else {
            panic!("actual compiler must retain the physical overlay slot")
        };
        assert_eq!(steps, &path.steps);
        assert_eq!(*observed_slot, slot);
        let omissions = observation
            .omissions
            .iter()
            .filter(|omission| {
                matches!(
                    omission.reason,
                    crate::prefab::AuthoringSceneCompilationOmissionReason::RecordUnavailable(_)
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(omissions.len(), 1);
        assert_eq!(omissions[0].entity, declaration.entity);
        assert_eq!(&omissions[0].component, component);
        assert_eq!(
            omissions[0].reason,
            crate::prefab::AuthoringSceneCompilationOmissionReason::RecordUnavailable(
                AuthoringRecordReadiness::UnknownSchema,
            )
        );
    }
    let occurrences = prepared
        .occurrences()
        .iter()
        .find(|source| source.source == scene_index)
        .unwrap();
    let pair = occurrences
        .entities
        .iter()
        .find(|pair| pair.old() == &before.unavailable[0].entity)
        .unwrap();
    assert_eq!(pair.new(), &after.unavailable[0].entity);
}

#[test]
fn independent_addition_and_complete_set_keep_their_actual_overlay_contributor() {
    let unknown = ComponentTypeId::new("missing.OverlayContributor");
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
    ] {
        let prepared = prepare(
            overlay_contributor_request(slot, unknown.clone(), unavailable_overlay_record()),
            Default::default(),
        );
        assert_eq!(prepared.occurrences().len(), 2);
        assert_preserved_overlay_contributor(
            &prepared,
            0,
            4,
            0,
            &PrefabInstancePath::new(id("u")),
            slot,
            &unknown,
        );
        let mapped = prepared.record(4, 0).unwrap();
        assert_eq!(mapped.before, mapped.after);
    }
}

#[test]
fn containing_scene_owned_addition_and_complete_set_keep_their_overlay_contributor() {
    let unknown = ComponentTypeId::new("missing.SceneOverlayContributor");
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
    ] {
        let mut request = request();
        append_prefab(
            &mut request,
            "v",
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("v"))]),
        );
        let mut u = SceneEntityRecord::new(id("u"));
        u.prefab = Some(overlay_record_instance(
            slot,
            unknown.clone(),
            unavailable_overlay_record(),
        ));
        let source = CapturedExtractionSource::Scene {
            key: key("independent-overlay", SourceDocumentKind::Scene),
            document: Arc::new(SceneDocument::from_ordered_roots([u])),
            basis: request.basis.clone(),
        };
        append_source(&mut request, source, &[]);
        let prepared = prepare(request, Default::default());
        assert_eq!(prepared.occurrences().len(), 3);
        assert_preserved_overlay_contributor(
            &prepared,
            5,
            5,
            0,
            &PrefabInstancePath::new(id("u")),
            slot,
            &unknown,
        );
    }
}

#[test]
fn nested_claim_and_addition_paths_keep_the_actual_unavailable_overlay_contributor() {
    let unknown = ComponentTypeId::new("missing.NestedOverlayContributor");
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
    ] {
        for claim_first in [true, false] {
            let mut request = request();
            let leaf = overlay_record_instance(slot, unknown.clone(), unavailable_overlay_record());
            let mut middle_overlay = PrefabOverlay::new(key("bridge", SourceDocumentKind::Prefab));
            let mut outer_overlay = PrefabOverlay::new(key("shell", SourceDocumentKind::Prefab));
            if claim_first {
                let mut graft = PrefabAddition::new(id("graft"));
                graft.prefab = Some(leaf);
                middle_overlay.additions.push(graft);
                middle_overlay.placements.push(SceneEntityPlacement::new(
                    PrefabEntityTarget::addition(id("graft")),
                    ScenePlacement::last(ScenePlacementParent::Root),
                ));
            } else {
                middle_overlay.instances.push(PrefabInstanceClaim {
                    target: PrefabEntityTarget::source(id("slot")),
                    value: Some(leaf),
                });
            }
            let mut middle = PrefabInstance::forest(path("bridge"));
            middle.overrides = Some(middle_overlay);
            let steps = if claim_first {
                outer_overlay.instances.push(PrefabInstanceClaim {
                    target: PrefabEntityTarget::source(id("slot")),
                    value: Some(middle),
                });
                vec![
                    PrefabInstancePathSegment::InstanceClaim { index: 0 },
                    PrefabInstancePathSegment::Addition { index: 0 },
                ]
            } else {
                let mut graft = PrefabAddition::new(id("graft"));
                graft.prefab = Some(middle);
                outer_overlay.additions.push(graft);
                outer_overlay.placements.push(SceneEntityPlacement::new(
                    PrefabEntityTarget::addition(id("graft")),
                    ScenePlacement::last(ScenePlacementParent::Root),
                ));
                vec![
                    PrefabInstancePathSegment::Addition { index: 0 },
                    PrefabInstancePathSegment::InstanceClaim { index: 0 },
                ]
            };
            let mut outer = PrefabInstance::forest(path("shell"));
            outer.overrides = Some(outer_overlay);
            let mut u = SceneEntityRecord::new(id("u"));
            u.prefab = Some(outer);
            append_prefab(&mut request, "u", PrefabDocument::from_ordered_roots([u]));
            for name in ["shell", "bridge"] {
                append_prefab(
                    &mut request,
                    name,
                    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("slot"))]),
                );
            }
            append_prefab(
                &mut request,
                "v",
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("v"))]),
            );
            activate_independent_u(&mut request);
            let prepared = prepare(request, Default::default());
            assert_preserved_overlay_contributor(
                &prepared,
                0,
                4,
                0,
                &PrefabInstancePath {
                    anchor: id("u"),
                    steps,
                },
                slot,
                &unknown,
            );
            let mapped = prepared.record(4, 0).unwrap();
            assert_eq!(mapped.before, mapped.after);
        }
    }
}

fn overwritten_overlay_contributor_request(
    slot: PrefabOverlaySlot,
    unknown: ComponentTypeId,
) -> ExtractionRequest {
    let mut request = overlay_contributor_request(slot, unknown, unavailable_overlay_record());
    let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[4] else {
        panic!("Prefab")
    };
    let mut u = source.document().clone();
    u.entities[0]
        .components
        .insert(component(), link(reference("x", "p", "b")));
    u.entities.push(SceneEntityRecord::new(id("x")));
    u.placements.push(SceneEntityPlacement::root(id("x")));
    source.document = crate::prefab::ResolvedPrefabDocument::AuthoringSource {
        document: Arc::new(u),
        basis: request.basis.clone(),
    };
    let CapturedExtractionSource::Scene { document, .. } = &mut request.sources[0] else {
        panic!("Scene")
    };
    let main = Arc::make_mut(document);
    let anchor = main
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("independent"))
        .unwrap();
    let mut overlay = PrefabOverlay::new(key("u", SourceDocumentKind::Prefab));
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("u")),
        component: component(),
        record: link(local("independent")),
    });
    anchor.prefab.as_mut().unwrap().overrides = Some(overlay);
    request
}

#[test]
fn overwritten_reference_reallocates_u_without_changing_its_unavailable_overlay_record() {
    let unknown = ComponentTypeId::new("missing.ReallocatedOverlayContributor");
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
    ] {
        let request = overwritten_overlay_contributor_request(slot, unknown.clone());
        // U:x deliberately has no P instance. The outer complete Set overwrites the old
        // reference before demand; its understood source spelling must still gain k@Q.
        let prepared = prepare(request, Default::default());
        let SourceDocumentInput::Prefab(original_u) = prepared.original().sources[4].document()
        else {
            panic!("Prefab")
        };
        let SourceDocumentInput::Prefab(candidate_u) = prepared.sources()[4].document() else {
            panic!("Prefab")
        };
        assert!(!std::ptr::eq(original_u, candidate_u));
        let rewritten = EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![
                    PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("x")),
                        expected_base: key("p", SourceDocumentKind::Prefab),
                    },
                    PrefabInstanceHop {
                        anchor: PrefabEntityOrigin::SourceEntity(id("k")),
                        expected_base: key("q", SourceDocumentKind::Prefab),
                    },
                ],
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
            },
        };
        let mut expected = original_u.clone();
        expected.entities[0]
            .components
            .insert(component(), link(rewritten));
        assert_eq!(candidate_u, &expected);
        assert_ne!(original_u, candidate_u);
        assert_preserved_overlay_contributor(
            &prepared,
            0,
            4,
            0,
            &PrefabInstancePath::new(id("u")),
            slot,
            &unknown,
        );
    }
}

#[test]
fn unavailable_overlay_capture_and_raw_comparison_share_the_exact_byte_allowance() {
    let slot = PrefabOverlaySlot::Component { index: 0 };
    let unknown = ComponentTypeId::new("missing.BudgetedOverlayContributor");
    let fresh_request = || overwritten_overlay_contributor_request(slot, unknown.clone());
    let baseline = prepare(fresh_request(), Default::default());
    assert_preserved_overlay_contributor(
        &baseline,
        0,
        4,
        0,
        &PrefabInstancePath::new(id("u")),
        slot,
        &unknown,
    );
    let mapped = baseline.record(4, 0).unwrap();
    assert!(!std::ptr::eq(mapped.before, mapped.after));
    let usage = baseline.usage();
    let exact =
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(usage.1).unwrap());
    assert_eq!(prepare(fresh_request(), exact).usage(), usage);
    let one_short = exact.with_working_bytes(ByteLimit::new(usage.1 - 1).unwrap());
    let failure = match fresh_request().prepare(ExtractionWork::new(one_short, Some(0)).unwrap()) {
        Ok(_) => panic!("one-short overlay capture and raw comparison must refuse"),
        Err(failure) => failure,
    };
    assert!(
        matches!(failure.error, ExtractionError::Capacity),
        "{:?}",
        failure.error
    );
    assert!(failure.work.usage().0 > 0 && failure.work.usage().1 > 0);
}

#[test]
fn affected_u_overlay_contributors_still_refuse_unknown_schema_and_owner_mismatch() {
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
    ] {
        for unknown_schema in [true, false] {
            let mut request = overlay_contributor_request(slot, component(), link(local("u")));
            let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[4] else {
                panic!("Prefab")
            };
            let mut u = source.document().clone();
            u.entities.push(instance("affected", path("p")));
            u.placements
                .push(SceneEntityPlacement::root(id("affected")));
            source.document = crate::prefab::ResolvedPrefabDocument::AuthoringSource {
                document: Arc::new(u),
                basis: request.basis.clone(),
            };
            // Qualify the same actual U -> P topology through preparation with an understood
            // record before changing only the contributor's Schema/owner availability.
            let mut request = prepare(request, Default::default()).into_parts().0.original;
            let CapturedExtractionSource::Prefab { source, .. } = &mut request.sources[4] else {
                panic!("Prefab")
            };
            let mut u = source.document().clone();
            let overlay = u.entities[0]
                .prefab
                .as_mut()
                .unwrap()
                .overrides
                .as_mut()
                .unwrap();
            let (unavailable_component, unavailable_record) = if unknown_schema {
                (
                    ComponentTypeId::new("missing.AffectedOverlay"),
                    unavailable_overlay_record(),
                )
            } else {
                (
                    component(),
                    link(local("u")).with_owner(
                        PersistentComponentSchemaOwnerId::try_new("game.another-owner").unwrap(),
                    ),
                )
            };
            match slot {
                PrefabOverlaySlot::Addition { index } => {
                    overlay.additions[index].components =
                        [(unavailable_component, unavailable_record)].into();
                }
                PrefabOverlaySlot::Component { index } => {
                    let PrefabComponentIntent::Set {
                        component, record, ..
                    } = &mut overlay.components[index]
                    else {
                        panic!("complete Set")
                    };
                    *component = unavailable_component;
                    *record = unavailable_record;
                }
                _ => unreachable!(),
            }
            source.document = crate::prefab::ResolvedPrefabDocument::AuthoringSource {
                document: Arc::new(u),
                basis: request.basis.clone(),
            };
            let original_u = source.document() as *const PrefabDocument;
            let failure =
                match request.prepare(ExtractionWork::new(Default::default(), Some(0)).unwrap()) {
                    Ok(_) => panic!("affected unavailable overlay meaning must refuse"),
                    Err(failure) => failure,
                };
            assert!(
                if unknown_schema {
                    matches!(failure.error, ExtractionError::UnknownSchema)
                } else {
                    matches!(failure.error, ExtractionError::OwnerMismatch)
                },
                "actual refusal for {slot:?}: {:?}",
                failure.error
            );
            let SourceDocumentInput::Prefab(retained) = failure.request.sources[4].document()
            else {
                panic!("Prefab")
            };
            assert_eq!(retained as *const PrefabDocument, original_u);
        }
    }
}
