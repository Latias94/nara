use std::convert::Infallible;

use nara_asset::{
    AssetProductRef, AssetRecord, AssetSourceKind, AssetSourcePath, ImportedProductId,
    ProjectAssetDatabase, StableAssetId,
};
use nara_ecs::World;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentFieldSchema, ComponentRegistry,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
};

use super::*;
use crate::{
    InMemoryPrefabSourceResolver, PrefabProjectionStep, ResolvedPrefabSource,
    SceneAuthoringSession, SceneComponentBindings, SceneExportOptions, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn source_id() -> StableAssetId {
    "00000000-0000-4000-8000-000000000041".parse().unwrap()
}

fn product(primary: bool) -> AssetRef {
    AssetRef::Product(
        AssetProductRef::new(
            source_id(),
            if primary {
                ImportedProductId::PRIMARY
            } else {
                ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000051").unwrap()
            },
        )
        .unwrap(),
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ProductReferences")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.product-references").unwrap()
}

fn definition() -> (
    ComponentSchemaOwnerSnapshot,
    AuthoringSchemaBundle,
    AuthoringSchemaBasis,
) {
    let schema = ComponentSchema::new(component(), "Products", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("products"),
            "Products",
            ComponentFieldPath::from_fields(["products"]),
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::AssetRef,
            ))),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]);
    let snapshot = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::from(&snapshot)]).unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    (snapshot, bundle, basis)
}

fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "products",
            ComponentValue::map([(
                "caption",
                ComponentValue::List(vec![
                    crate::patch::asset_ref_value(&product(false)),
                    ComponentValue::Null,
                    crate::patch::asset_ref_value(&product(true)),
                ]),
            )]),
        )]),
    )
    .with_owner(owner())
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(component(), record())
    ])
}

#[test]
fn nested_product_inventory_survives_missing_adapter_unknown_payload_and_wire_roundtrips() {
    let (_, bundle, available) = definition();
    let unbound = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    let mut document = document();
    // Unknown payloads are opaque, including values resembling malformed product references.
    document.entities[0].components.insert(
        ComponentTypeId::new("test.UnknownProductPayload"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("kind", ComponentValue::String("product".into())),
                (
                    "value",
                    ComponentValue::String("opaque, not an asset leaf".into()),
                ),
                ("retained", crate::patch::asset_ref_value(&product(false))),
            ]),
        ),
    );
    for (basis, readiness) in [
        (available, AuthoringRecordReadiness::Available),
        (unbound, AuthoringRecordReadiness::KnownUnbound),
    ] {
        let published =
            SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(AuthoringDocumentSource::new(product(true)), &basis)
                .unwrap()
                .publish();
        assert_eq!(published.document(), &document);
        let known = published
            .record_facts()
            .iter()
            .find(|fact| fact.component() == &component())
            .unwrap();
        assert_eq!(known.readiness(), readiness);
        let references = published
            .schema_declared_asset_references()
            .collect::<Vec<_>>();
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].asset_ref(), &product(false));
        assert_eq!(references[1].asset_ref(), &product(true));
        assert_eq!(references[0].path().to_string(), "products.caption[0]");
        assert_eq!(references[1].path().to_string(), "products.caption[2]");
        assert!(published.record_facts().iter().any(|fact| {
            fact.readiness() == AuthoringRecordReadiness::UnknownSchema
                && fact.schema_declared_asset_references().is_empty()
        }));
        let manifest = published.unavailable_manifest().clone();
        let session = SceneAuthoringSession::from_published_authoring_document(published);
        for json in [true, false] {
            let candidate = if json {
                SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            } else {
                SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap())
            }
            .unwrap();
            let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
            assert_eq!(reopened.document(), &document);
            assert_eq!(
                reopened.authoring_record_facts(),
                session.authoring_record_facts()
            );
            assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
        }
    }
}

#[test]
fn scene_product_source_check_and_both_export_policies_preserve_complete_selection() {
    let (snapshot, _, _) = definition();
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let bindings = SceneComponentBindings::new(registry.snapshot().unwrap(), [snapshot]).unwrap();
    let document = document();
    let candidate = bindings
        .expand_prefabs(&document, &InMemoryPrefabSourceResolver::new())
        .candidate
        .unwrap()
        .bind_source(crate::tests::scene_source_key("products/source"));
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            source_id(),
            AssetSourcePath::new("text/captions.bundle").unwrap(),
            AssetSourceKind::Other("text_bundle".into()),
        ))
        .unwrap();
    let mut world = World::new();
    // This owner has no domain or product registry. Missing enclosing sources still refuse;
    // a source record permits data publication without claiming the auxiliary product exists.
    let missing = bindings.prepare_scene(
        &mut world,
        &candidate,
        &SceneOccurrenceKey::new("products", "missing").unwrap(),
        Some(ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT)),
    );
    assert!(missing.is_err());
    let report = bindings
        .prepare_scene(
            &mut world,
            &candidate,
            &SceneOccurrenceKey::new("products", "admitted").unwrap(),
            Some(database.resolver(&nara_asset::AssetPathScope::ROOT)),
        )
        .unwrap()
        .commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert!(report.instance.is_some());
    for policy in [
        nara_asset::AssetRefExportPolicy::Path,
        nara_asset::AssetRefExportPolicy::StableIdWhenKnown,
    ] {
        // Neither product selection needs a source-spelling conversion, even under Path.
        let exported = bindings.export_scene_with_options(
            &world,
            SceneExportOptions {
                asset_ref_export_policy: policy,
                ..Default::default()
            },
            None,
        );
        assert!(!exported.diagnostics.has_errors(), "{exported:?}");
        assert_eq!(
            exported.output().unwrap().document.entities[0].components[&component()],
            record()
        );
    }
}

#[test]
fn nested_primary_prefab_conversion_retains_product_inventory_provenance_and_history() {
    let (_, _, basis) = definition();
    let inner_id: StableAssetId = "00000000-0000-4000-8000-000000000042".parse().unwrap();
    let inner_ref =
        AssetRef::Product(AssetProductRef::new(inner_id, ImportedProductId::PRIMARY).unwrap());
    let inner = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::new(document().roots, document().entities)),
        basis.clone(),
        SourceDocumentKey::stable_asset(inner_id, SourceDocumentKind::Prefab),
    );
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(PrefabInstance {
        source: inner_ref.clone(),
        overrides: None,
    });
    let outer = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([nested])),
        basis.clone(),
        SourceDocumentKey::stable_asset(source_id(), SourceDocumentKind::Prefab),
    );
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: product(true),
        overrides: None,
    });
    let original = SceneDocument::from_ordered_roots([anchor]);
    let published = SceneDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/products.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let load = |request: &AssetRef| {
        Ok::<_, Infallible>(if request == &product(true) {
            Some(outer.clone())
        } else if request == &inner_ref {
            Some(inner.clone())
        } else {
            panic!("unexpected source request: {request}");
        })
    };
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |request, _| load(request))
        .unwrap();
    assert_eq!(prepared.effective_asset_references().len(), 2);
    assert_eq!(
        prepared.effective_asset_references()[0].asset_ref(),
        &product(false)
    );
    assert_eq!(
        prepared.effective_asset_references()[1].asset_ref(),
        &product(true)
    );
    assert!(prepared.entities().iter().any(|entity| {
        entity.provenance().projection_path().is_some_and(|path| {
            matches!(path.steps(), [
                PrefabProjectionStep::SourceEntity { source_document: outer, .. },
                PrefabProjectionStep::SourceEntity { source_document: inner, .. },
            ] if outer == &product(true) && inner == &inner_ref)
        })
    }));
    let converted = prepared.document().clone();
    session
        .commit_complete_prefab_conversion_with_loader(prepared, load)
        .unwrap();
    assert_eq!(session.document(), &converted);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &converted);
    let reopened = session
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_ron_str(&converted.to_ron_string().unwrap()).unwrap(),
        )
        .unwrap();
    assert_eq!(reopened.document(), &converted);
    assert_eq!(
        reopened.authoring_record_facts(),
        session.authoring_record_facts()
    );
}

#[test]
fn auxiliary_prefab_sources_refuse_before_loader_and_primary_requires_matching_source_proof() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: product(false),
        overrides: None,
    });
    let auxiliary = SceneDocument::from_ordered_roots([anchor]);
    let report = auxiliary
        .try_expand_prefabs_with_loader(
            &registry,
            Default::default(),
            None,
            |_, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
                panic!("an auxiliary selection must not open or route a native Prefab source")
            },
        )
        .unwrap();
    assert!(report.candidate.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-source-kind-invalid")
    );
    assert!(SceneDocumentCandidate::decode_json_str(&auxiliary.to_json_string().unwrap()).is_err());
    assert!(SceneDocumentCandidate::decode_ron_str(&auxiliary.to_ron_string().unwrap()).is_err());
    let body = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("leaf")),
    ]));
    let mut primary = auxiliary.clone();
    primary.entities[0].prefab.as_mut().unwrap().source = product(true);
    for (key, accepted) in [
        (crate::tests::named_prefab_source_key("unproved"), false),
        (
            SourceDocumentKey::stable_asset(source_id(), SourceDocumentKind::Scene),
            false,
        ),
        (
            SourceDocumentKey::stable_asset(source_id(), SourceDocumentKind::Prefab),
            true,
        ),
    ] {
        let source = ResolvedPrefabSource::bound(Arc::clone(&body), key);
        let report = primary
            .try_expand_prefabs_with_loader(&registry, Default::default(), None, |_, _| {
                Ok::<_, Infallible>(Some(source.clone()))
            })
            .unwrap();
        assert_eq!(report.candidate.is_some(), accepted, "{report:?}");
    }
}
