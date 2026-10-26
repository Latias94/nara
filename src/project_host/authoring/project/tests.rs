use super::*;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId, collect_declared_asset_references_from_schema,
};
use nara_scene::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
    PrefabInstanceClaim, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
    PrefabPlacementSlot, PrefabReferenceBinding, SceneComponentRecord, SceneEntityId,
    SourceDocumentKey, SourceDocumentKind,
};

fn overlay_base(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e3818e8d-e2d8-4f67-a82f-55074d3614b2".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

#[test]
fn instance_retention_counts_record_owner_payloads() {
    let owner = PersistentComponentSchemaOwnerId::try_new("test.snapshot-owner").unwrap();
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::String("full retained record".repeat(64)),
    );
    let declaration = |record| {
        let mut overlay = PrefabOverlay::new(overlay_base("retained"));
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Add,
            target: PrefabEntityTarget::source(SceneEntityId::new("nested").unwrap()),
            component: ComponentTypeId::new("test.Retained"),
            record,
            bindings: Vec::new(),
        });
        PrefabInstance {
            source: AssetRef::path("retained/base.prefab.json").unwrap(),
            overrides: Some(overlay),
        }
    };
    let unclaimed = declaration(record.clone());
    let claimed = declaration(record.with_owner(owner.clone()));
    assert_eq!(
        prefab_instance_retained_bytes(&claimed).unwrap()
            - prefab_instance_retained_bytes(&unclaimed).unwrap(),
        owner.as_str().len() + crate::scene_retention::STRING_ALLOCATION_OVERHEAD,
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
                overlay_declarations_retained_bytes: manifest_overlay_declarations_retained_bytes(
                    published.unavailable_manifest(),
                )
                .unwrap(),
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

#[test]
fn structural_overlay_manifests_charge_full_instance_copies_at_the_budget_boundary() {
    use crate::project_host::authoring::AuthoringProjectAdmissionLimits;
    use nara_scene::{AuthoringDocumentSource, SceneDocumentCandidate};

    let mut nested = PrefabOverlay::new(overlay_base("nested"));
    nested.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(SceneEntityId::new("detached").unwrap()),
        value: None,
    });
    let mut addition = PrefabAddition::new(SceneEntityId::new("addition").unwrap());
    addition.prefab = Some(PrefabInstance {
        source: AssetRef::path("nested.prefab.json").unwrap(),
        overrides: Some(nested),
    });
    addition.bindings.insert(
        ComponentTypeId::new("test.DormantBinding"),
        vec![PrefabReferenceBinding {
            slot: ComponentFieldPath::from_fields(["references", "target"]),
            expected: SceneEntityId::new("preserved-reference").unwrap(),
            target: PrefabEntityTarget::source(SceneEntityId::new("unresolved").unwrap()),
        }],
    );
    let mut overlay = PrefabOverlay::new(overlay_base("structural"));
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(addition.id.clone()),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    overlay.additions.push(addition);
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("missing-target".repeat(128)).unwrap(),
    ));
    let instance = PrefabInstance {
        source: AssetRef::path("structural.prefab.json").unwrap(),
        overrides: Some(overlay),
    };
    let document = SceneDocument::from_ordered_roots(["first", "second"].map(|id| {
        let mut entity = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
        entity.prefab = Some(instance.clone());
        entity
    }));
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
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
    assert!(published.record_facts().is_empty());
    let manifest = published.unavailable_manifest();
    assert_eq!(manifest.total_count(), 0);
    assert_eq!(manifest.overlay_declarations().count(), 2);
    for (_, retained) in manifest.overlay_declarations() {
        assert_eq!(retained, &instance);
    }
    let manifest_bytes = manifest_overlay_declarations_retained_bytes(manifest).unwrap();
    let retained_instance = manifest.overlay_declarations().next().unwrap().1;
    let overlay_bytes = crate::scene_retention::overlay_retained_bytes(
        retained_instance.overrides.as_ref().unwrap(),
    )
    .unwrap();
    assert!(manifest_bytes > 2 * overlay_bytes);

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
    let charge = |manifest_bytes, budget: &mut AuthoringProjectAdmissionBudget| {
        charge_published_document(
            PublishedDocumentCharge {
                source: &source,
                encoded_bytes: encoded.len(),
                document: published.document(),
                record_facts: published.record_facts(),
                asset_references_retained_bytes: 0,
                unavailable_records: manifest.total_count(),
                overlay_declarations_retained_bytes: manifest_bytes,
                diagnostics: published.diagnostics(),
                bundle_encoded_bytes: bundle.to_json_string().unwrap().len(),
            },
            budget,
        )
    };
    let mut baseline =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    charge(0, &mut baseline).unwrap();
    let base_bytes = baseline.usage().retained_candidate_bytes();
    let exact = base_bytes + manifest_bytes;
    for maximum in [exact, exact - 1, base_bytes + manifest_bytes / 2] {
        let mut budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits {
            retained_candidate_bytes: ByteLimit::new(maximum).unwrap(),
            ..AuthoringProjectAdmissionLimits::default()
        });
        let result = charge(manifest_bytes, &mut budget);
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

#[test]
fn raw_composition_accounting_retains_one_payload_and_no_publication_projections() {
    use crate::project_host::authoring::AuthoringProjectAdmissionLimits;

    let mut overlay = PrefabOverlay::new(overlay_base("raw"));
    overlay.suppressions.push(PrefabEntityTarget::source(
        SceneEntityId::new("obsolete-target".repeat(64)).unwrap(),
    ));
    let mut entity = SceneEntityRecord::new(SceneEntityId::new("raw").unwrap());
    entity.prefab = Some(PrefabInstance {
        source: AssetRef::path("missing.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    entity.components.insert(
        ComponentTypeId::new("test.Raw"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("uninterpreted payload".repeat(128)),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new("test.raw-owner").unwrap()),
    );
    let document = PrefabDocument::from_ordered_roots([entity]);
    let encoded = document.to_json_string().unwrap();
    let file = FileCapability::from_host_handle(
        std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap(),
        nara_fs::TrustMode::TrustedLocal,
        1,
    )
    .unwrap();
    let source = AuthoringSourceFact {
        kind: AuthoringSourceKind::Prefab,
        project_path: ProjectPath::new("prefabs/raw.prefab.json").unwrap(),
        identity: file.identity(),
        parent_identity: None,
        digest: ContentDigest::of_bytes(encoded.as_bytes()),
        encoded_bytes: encoded.len(),
        dependency_depth: 1,
    };
    let document_bytes = document.retained_bytes().unwrap();
    let exact = document_bytes
        + source_fact_retained_bytes(&source).unwrap()
        + size_of::<Arc<PrefabDocument>>();
    for maximum in [exact, exact - 1] {
        let mut budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits {
            retained_candidate_bytes: ByteLimit::new(maximum).unwrap(),
            ..Default::default()
        });
        let result = charge_composition_source(&source, encoded.len(), &document, &mut budget);
        if maximum == exact {
            result.unwrap();
            assert_eq!(budget.usage().retained_candidate_bytes(), exact);
            assert_eq!(budget.usage().decoded_bytes(), document_bytes);
            assert_eq!(budget.usage().decode_work(), document_bytes + encoded.len());
            assert_eq!(budget.usage().diagnostic_items(), 0);
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

#[test]
fn composition_source_candidates_keep_selected_limits_and_validate_only_local_structure() {
    let entity = SceneEntityRecord::new(SceneEntityId::new("local").unwrap()).with_component(
        ComponentTypeId::new("test.Raw"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
    );
    let document = PrefabDocument::from_ordered_roots([entity.clone()]);
    let encoded = document.to_json_string().unwrap();
    let limits = SceneFileLimits::default()
        .with_entities(nara_core::ItemLimit::ONE)
        .with_components(nara_core::ItemLimit::ONE)
        .with_encoded_bytes(ByteLimit::new(encoded.len()).unwrap());
    let candidate =
        PrefabDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits).unwrap();
    assert_eq!(candidate.into_composition_source().unwrap(), document);
    assert!(matches!(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            encoded.as_bytes(),
            limits.with_encoded_bytes(ByteLimit::new(encoded.len() - 1).unwrap()),
        ),
        Err(SceneFormatError::EncodedBytesExceeded { .. })
    ));
    let oversized = PrefabDocument::from_ordered_roots([
        entity.clone(),
        SceneEntityRecord::new(SceneEntityId::new("second").unwrap()),
    ]);
    assert!(matches!(
        PrefabDocumentCandidate::from_source_document(oversized, limits),
        Err(SceneFormatError::Budget(_))
    ));

    let invalid = PrefabDocument::new([SceneEntityId::new("missing").unwrap()], [entity]);
    let candidate = PrefabDocumentCandidate::from_source_document(invalid, limits).unwrap();
    let rejection = candidate.into_composition_source().unwrap_err();
    assert!(
        rejection
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.dangling-root" })
    );
}
