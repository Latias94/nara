use super::*;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId, collect_declared_asset_references_from_schema,
};
use nara_scene::{SceneComponentRecord, SceneEntityId};

#[test]
fn exact_instance_accounting_keeps_equal_snapshots_and_record_owner_payloads() {
    let owner = PersistentComponentSchemaOwnerId::try_new("test.snapshot-owner").unwrap();
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::String("full retained record".repeat(64)),
    );
    let declaration = |record| PrefabInstance {
        source: AssetRef::path("retained/base.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: SceneEntityId::new("nested").unwrap(),
            component: ComponentTypeId::new("test.Retained"),
            value: record,
        }]),
    };
    let unclaimed = declaration(record.clone());
    let claimed = declaration(record.with_owner(owner.clone()));
    assert_eq!(
        prefab_instance_retained_bytes(&claimed).unwrap()
            - prefab_instance_retained_bytes(&unclaimed).unwrap(),
        owner.as_str().len(),
    );
    let patch = |expected, value| {
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected,
            value,
        }])
    };
    let empty = patch_retained_bytes(&patch(None, None)).unwrap();
    let prior = patch_retained_bytes(&patch(Some(claimed.clone()), None)).unwrap() - empty;
    let next = patch_retained_bytes(&patch(None, Some(claimed.clone()))).unwrap() - empty;
    assert!(prior > "full retained record".len() * 64);
    assert_eq!(prior, next);
    assert_eq!(
        patch_retained_bytes(&patch(Some(claimed.clone()), Some(claimed))).unwrap() - empty,
        prior + next,
    );
}

#[test]
fn reference_projection_accounting_includes_defaults_paths_and_duplicate_storage() {
    let reference_value = ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        ("value", ComponentValue::String("default.png".to_owned())),
    ]);
    let references = |field: &str, ancestor: &str, key: &str| {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("test.RetainedReferences"),
            "Retained references",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new(field),
            "Assets",
            ComponentFieldPath::from_fields([ancestor, "assets"]),
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::AssetRef)),
            ComponentValue::map([(key, ComponentValue::List(vec![reference_value.clone()]))]),
        )
        .with_capability(ComponentCapability::AssetRef)]);
        let authored = ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>());
        let references = collect_declared_asset_references_from_schema(
            &schema,
            ComponentSchemaVersion::ONE,
            &authored,
        )
        .unwrap();
        assert_eq!(
            authored,
            ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>())
        );
        assert_eq!(references.len(), 1);
        references
    };
    let short = references("asset", "s", "k");
    let extended = references(
        "asset-with-stable-id",
        "long-storage-parent",
        "long-map-key",
    );
    let short_cost = declared_asset_references_retained_bytes(&short).unwrap();
    assert!(short_cost > size_of::<DeclaredAssetReference>() + "default.png".len());
    assert_eq!(
        declared_asset_references_retained_bytes(&extended).unwrap() - short_cost,
        ("asset-with-stable-id".len() - "asset".len())
            + ("long-storage-parent".len() - "s".len())
            + ("long-map-key".len() - "k".len()),
    );
    assert_eq!(
        declared_asset_references_retained_bytes(short.iter().chain(&short)).unwrap(),
        2 * short_cost,
    );
    assert_eq!(
        declared_asset_references_retained_bytes(std::iter::empty()).unwrap(),
        0
    );
}

#[test]
fn published_unbound_default_assets_charge_both_projection_copies_at_the_budget_boundary() {
    use crate::project_host::authoring::AuthoringProjectAdmissionLimits;
    use nara_reflect::{AuthoringSchemaBundleOwner, ComponentSchemaCatalog};
    use nara_scene::{AuthoringDocumentSource, AuthoringRecordReadiness, SceneDocumentCandidate};

    let component = ComponentTypeId::new("test.UnboundDefaultAssets");
    let owner = PersistentComponentSchemaOwnerId::try_new("test.default-assets").unwrap();
    let reference = ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        ("value", ComponentValue::String("default.png".to_owned())),
    ]);
    let schema = ComponentSchema::new(
        component.clone(),
        "Unbound default assets",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("assets"),
        "Assets",
        ComponentFieldPath::from_fields(["settings", "assets"]),
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        ComponentValue::List(vec![reference; 3]),
    )
    .with_capability(ComponentCapability::AssetRef)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let empty = ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>());
    let document = SceneDocument::from_ordered_roots(["a", "b"].map(|id| {
        let mut entity = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
        entity.components.insert(
            component.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty.clone())
                .with_owner(owner.clone()),
        );
        entity
    }));
    let encoded = document.to_json_string().unwrap();
    let published = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
    assert_eq!(published.document(), &document);
    assert_eq!(published.record_facts().len(), 2);
    for fact in published.record_facts() {
        assert_eq!(fact.readiness(), AuthoringRecordReadiness::KnownUnbound);
        assert_eq!(fact.schema_declared_asset_references().len(), 3);
    }
    assert_eq!(published.unavailable_manifest().total_count(), 2);
    for entry in published.unavailable_manifest().entries() {
        let UnavailableRecordManifestEntry::Record(record) = entry else {
            panic!("complete unbound records must retain complete manifest entries");
        };
        assert_eq!(record.value(), &empty);
        assert_eq!(record.schema_declared_asset_references().len(), 3);
    }
    let projection_bytes =
        declared_asset_references_retained_bytes(published.schema_declared_asset_references())
            .unwrap();
    // Charging does not inspect source identity. Supply an actual live file capability rather
    // than constructing identity evidence solely for this accounting test.
    let file = FileCapability::from_host_handle(
        std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap(),
        nara_fs::TrustMode::TrustedLocal,
        1,
    )
    .unwrap();
    let source = AuthoringSourceFact {
        kind: AuthoringSourceKind::Scene,
        project_path: ProjectPath::new("scene.json").unwrap(),
        identity: file.identity(),
        parent_identity: None,
        digest: ContentDigest::of_bytes(encoded.as_bytes()),
        encoded_bytes: encoded.len(),
        dependency_depth: 0,
    };
    let bundle_encoded_bytes = bundle.to_json_string().unwrap().len();
    let charge = |projection_bytes, budget: &mut AuthoringProjectAdmissionBudget| {
        charge_published_document(
            PublishedDocumentCharge {
                source: &source,
                encoded_bytes: encoded.len(),
                document: published.document(),
                record_facts: published.record_facts(),
                asset_references_retained_bytes: projection_bytes,
                unavailable_records: published.unavailable_manifest().total_count(),
                diagnostics: published.diagnostics(),
                bundle_encoded_bytes,
            },
            budget,
        )
    };
    let mut baseline =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    charge(0, &mut baseline).unwrap();
    let base_bytes = baseline.usage().retained_candidate_bytes();
    let exact = base_bytes + 2 * projection_bytes;
    for maximum in [exact, exact - 1, base_bytes + projection_bytes] {
        let mut budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits {
            retained_candidate_bytes: ByteLimit::new(maximum).unwrap(),
            ..AuthoringProjectAdmissionLimits::default()
        });
        let result = charge(projection_bytes, &mut budget);
        if maximum == exact {
            result.unwrap();
            assert_eq!(budget.usage().retained_candidate_bytes(), exact);
        } else {
            assert!(matches!(
                result,
                Err(AuthoringProjectSourceAdmissionError::Budget(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                        maximum: rejected,
                    },
                )) if rejected == maximum
            ));
        }
    }
}
