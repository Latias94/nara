use super::*;
use crate::{
    PrefabAddition, PrefabAuthoringSession, PrefabComponentSetMode, PrefabInstanceClaim,
    PrefabReferenceBinding, SceneAuthoringSession, SourceDocumentKey, SourceDocumentKind,
};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchemaCatalog, ComponentShapeMember,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn asset(path: &str) -> AssetRef {
    AssetRef::path(path).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e3818e8d-e2d8-4f67-a82f-55074d3614b2".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.overlay.Value")
}

fn instance(overlay: PrefabOverlay) -> PrefabInstance {
    PrefabInstance {
        source: asset("prefabs/base.prefab.json"),
        overrides: Some(overlay),
    }
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
}

fn set(value: ComponentValue) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("inherited")),
        component: component(),
        record: record(value),
        bindings: Vec::new(),
    }
}

fn document(instance: PrefabInstance) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(instance);
    SceneDocument::from_ordered_roots([anchor])
}

fn empty_basis() -> Arc<AuthoringClassificationBasis> {
    Arc::new(
        AuthoringClassificationBasis::new(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap(),
    )
}

fn classify(
    document: &SceneDocument,
    basis: Arc<AuthoringClassificationBasis>,
    limits: SceneFileLimits,
) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
    classify_scene_document(
        document,
        AuthoringDocumentSource::new(asset("scenes/main.scene.json")),
        AuthoringDocumentKind::Scene,
        basis,
        limits,
    )
}

fn nested_instance() -> PrefabInstance {
    let mut nested = PrefabOverlay::new(key("nested"));
    nested
        .components
        .push(set(ComponentValue::String("preserve nested".into())));
    instance(nested)
}

#[test]
fn actual_addition_set_and_nested_instance_declarations_have_distinct_locations() {
    let mut overlay = PrefabOverlay::new(key("base"));
    let mut addition = PrefabAddition::new(id("addition"));
    addition
        .components
        .insert(component(), record(ComponentValue::Null));
    addition.prefab = Some(nested_instance());
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("nested-anchor")),
        value: Some(nested_instance()),
    });
    overlay.components.push(set(ComponentValue::U64(5)));
    let document = document(instance(overlay));
    let state = classify(&document, empty_basis(), SceneFileLimits::default()).unwrap();
    assert_eq!(state.record_facts.len(), 4);
    assert_eq!(state.unavailable_manifest.record_count(), 4);
    assert!(matches!(state.record_facts[0].location(),
        AuthoringRecordLocation::OverlayComponent {
            slot: AuthoringOverlayRecordSlot::Addition { index: 0 }, target, ..
        } if target == &PrefabEntityTarget::addition(id("addition"))));
    assert!(matches!(state.record_facts[1].location(),
        AuthoringRecordLocation::OverlayComponent { overlays, .. }
        if matches!(overlays.as_slice(), [AuthoringOverlayPathSegment::Root { .. },
            AuthoringOverlayPathSegment::Addition { index: 0, entity, .. }] if entity == &id("addition"))));
    assert!(matches!(state.record_facts[2].location(),
        AuthoringRecordLocation::OverlayComponent { overlays, .. }
        if matches!(overlays.as_slice(), [AuthoringOverlayPathSegment::Root { .. },
            AuthoringOverlayPathSegment::InstanceClaim { index: 0, .. }])));
    assert_eq!(
        state
            .unavailable_manifest
            .overlay_declarations()
            .next()
            .unwrap()
            .1,
        document.entities[0].prefab.as_ref().unwrap()
    );
}

#[test]
fn equality_guards_complete_source_intent_including_bindings_base_and_owner() {
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(set(ComponentValue::Null));
    overlay
        .components
        .push(property(PrefabPropertyIntent::Set(ComponentValue::Null)));
    let original = instance(overlay);
    let classify_instance = |instance| {
        classify(
            &document(instance),
            empty_basis(),
            SceneFileLimits::default(),
        )
        .unwrap()
        .unavailable_manifest
    };
    let expected = classify_instance(original.clone());
    let changes: [fn(&mut PrefabInstance); 10] = [
        |instance| instance.source = asset("prefabs/alias.prefab.json"),
        |instance| instance.overrides.as_mut().unwrap().base = key("replacement"),
        |instance| {
            if let PrefabComponentIntent::Set { record, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                record.value = ComponentValue::U64(1);
            }
        },
        |instance| {
            if let PrefabComponentIntent::Set { record, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                record.version = ComponentSchemaVersion(2);
            }
        },
        |instance| {
            if let PrefabComponentIntent::Set { record, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                record.owner =
                    Some(PersistentComponentSchemaOwnerId::try_new("test.other").unwrap());
            }
        },
        |instance| {
            if let PrefabComponentIntent::Set { bindings, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                bindings.push(PrefabReferenceBinding {
                    slot: ComponentFieldPath::empty(),
                    expected: id("spelling"),
                    target: PrefabEntityTarget::addition(id("other")),
                });
            }
        },
        |instance| {
            if let PrefabComponentIntent::Set { mode, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                *mode = PrefabComponentSetMode::Add;
            }
        },
        |instance| {
            if let PrefabComponentIntent::Set { target, .. } =
                &mut instance.overrides.as_mut().unwrap().components[0]
            {
                *target = PrefabEntityTarget::addition(id("different-origin"));
            }
        },
        |instance| {
            if let PrefabComponentIntent::Properties { claims, .. } =
                &mut instance.overrides.as_mut().unwrap().components[1]
            {
                claims[0]
                    .path
                    .steps
                    .push(ComponentPropertyStep::MapEntry("other".into()));
            }
        },
        |instance| {
            instance
                .overrides
                .as_mut()
                .unwrap()
                .instances
                .push(PrefabInstanceClaim {
                    target: PrefabEntityTarget::source(id("detached")),
                    value: None,
                })
        },
    ];
    for change in changes {
        let mut changed = original.clone();
        change(&mut changed);
        assert_ne!(classify_instance(changed), expected);
    }
}

fn asset_basis(adapter: bool) -> Arc<AuthoringClassificationBasis> {
    let schema = ComponentSchema::new(component(), "Value", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
                crate::patch::asset_ref_value(&asset("textures/default.png")),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("settings"),
                "Settings",
                ComponentFieldPath::from_fields(["settings"]),
                ComponentValueShape::record_members([ComponentShapeMember::new(
                    "image",
                    ComponentValueKind::AssetRef,
                )
                .with_id(ComponentFieldId::new("nested-image"))]),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
        ]);
    let owner = AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.overlay").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    );
    let adapters = if adapter {
        vec![AuthoringAdapterReceipt::from_schema_owner(&owner, &BTreeSet::new()).unwrap()]
    } else {
        Vec::new()
    };
    let bundle = AuthoringSchemaBundle::new([owner]).unwrap();
    Arc::new(AuthoringClassificationBasis::new(&bundle, &adapters).unwrap())
}

fn property(intent: PrefabPropertyIntent) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("not-resolved-by-source-admission")),
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("image")),
            intent,
            bindings: Vec::new(),
        }],
    }
}

#[test]
fn inventory_includes_complete_defaults_property_payloads_and_absent_defaults_without_activation() {
    let mut overlay = PrefabOverlay::new(key("unresolved-base"));
    let mut addition = PrefabAddition::new(id("addition"));
    addition
        .components
        .insert(component(), record(ComponentValue::Map(BTreeMap::new())));
    overlay.additions.push(addition);
    overlay.components.extend([
        set(ComponentValue::Map(BTreeMap::new())),
        property(PrefabPropertyIntent::Set(crate::patch::asset_ref_value(
            &asset("textures/property.png"),
        ))),
        property(PrefabPropertyIntent::Absent),
    ]);
    let document = document(instance(overlay));
    for adapter in [false, true] {
        let state = classify(&document, asset_basis(adapter), SceneFileLimits::default()).unwrap();
        let references = state
            .schema_declared_asset_references()
            .map(|reference| reference.asset_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            references,
            vec![
                &asset("textures/default.png"),
                &asset("textures/default.png"),
                &asset("textures/property.png"),
                &asset("textures/default.png"),
            ]
        );
        assert_eq!(
            state.unavailable_manifest.overlay_target_count(),
            if adapter { 0 } else { 2 }
        );
    }
}

#[test]
fn property_inventory_shares_one_aggregate_reference_budget() {
    let limits = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(8).unwrap())
        .with_component_value_bytes(ByteLimit::new(56).unwrap());
    let mut overlay = PrefabOverlay::new(key("base"));
    let claim = property(PrefabPropertyIntent::Set(crate::patch::asset_ref_value(
        &asset("textures/a.png"),
    )));
    overlay.components.push(claim.clone());
    assert!(
        classify(
            &document(instance(overlay.clone())),
            asset_basis(false),
            limits
        )
        .is_ok()
    );
    overlay.components.push(claim.clone());
    let state = classify(
        &document(instance(overlay.clone())),
        asset_basis(false),
        limits,
    )
    .unwrap();
    assert_eq!(state.schema_declared_asset_references().count(), 2);
    // Three selectors fit six entries/30 bytes; their payload-relative asset facts need 57 bytes.
    overlay.components.push(claim);
    let document = document(instance(overlay));
    let error = classify(&document, asset_basis(false), limits).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authoring-asset-reference-budget-exceeded"
    }));
    let state = classify(
        &document,
        asset_basis(false),
        limits.with_component_value_bytes(ByteLimit::new(57).unwrap()),
    )
    .unwrap();
    assert_eq!(state.schema_declared_asset_references().count(), 3);
}

#[test]
fn unknown_property_roots_share_exact_selector_limits_without_asset_references() {
    let mut unknown = property(PrefabPropertyIntent::Absent);
    if let PrefabComponentIntent::Properties { claims, .. } = &mut unknown {
        claims[0].path = ComponentPropertyPath::new(ComponentFieldId::new("missing"));
    }
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(unknown.clone());
    let single = document(instance(overlay.clone()));
    overlay.components.push(unknown);
    let repeated = document(instance(overlay));
    let basis = asset_basis(true);

    // Each failed lookup scans the selector and both roots: three entries and 7 + 5 + 8 bytes.
    for (nodes, bytes, accepted) in [(6, 40, true), (5, 40, false), (6, 39, false)] {
        let limits = SceneFileLimits::default()
            .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
            .with_component_value_bytes(ByteLimit::new(bytes).unwrap());
        let single_state = classify(&single, Arc::clone(&basis), limits).unwrap();
        assert_eq!(single_state.schema_declared_asset_references().count(), 0);
        assert_eq!(single_state.unavailable_manifest.overlay_target_count(), 1);

        let result = classify(&repeated, Arc::clone(&basis), limits);
        if accepted {
            let state = result.unwrap();
            assert_eq!(state.readiness, AuthoringDocumentReadiness::Degraded);
            assert_eq!(state.schema_declared_asset_references().count(), 0);
            assert_eq!(state.unavailable_manifest.overlay_target_count(), 2);
        } else {
            let error = result.unwrap_err();
            assert!(error.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authoring-asset-reference-budget-exceeded"
            }));
        }
    }
}

#[test]
fn successful_and_failed_property_selectors_consume_the_same_budget() {
    let known = property(PrefabPropertyIntent::Set(ComponentValue::Null));
    let mut unknown = property(PrefabPropertyIntent::Absent);
    if let PrefabComponentIntent::Properties { claims, .. } = &mut unknown {
        claims[0].path = ComponentPropertyPath::new(ComponentFieldId::new("missing"));
    }
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.extend([known, unknown]);
    let document = document(instance(overlay));
    let basis = asset_basis(true);

    // The known root costs two entries/10 bytes; the unknown root costs three entries/20 bytes.
    for (nodes, bytes, accepted) in [(5, 30, true), (4, 30, false), (5, 29, false)] {
        let limits = SceneFileLimits::default()
            .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
            .with_component_value_bytes(ByteLimit::new(bytes).unwrap());
        let result = classify(&document, Arc::clone(&basis), limits);
        if accepted {
            let state = result.unwrap();
            assert_eq!(state.schema_declared_asset_references().count(), 0);
            assert_eq!(state.unavailable_manifest.overlay_target_count(), 1);
        } else {
            let error = result.unwrap_err();
            assert!(error.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authoring-asset-reference-budget-exceeded"
            }));
        }
    }
}

#[test]
fn nested_property_asset_inventory_is_payload_relative_without_an_effective_base() {
    let mut overlay = PrefabOverlay::new(key("not-loaded"));
    let mut nested = property(PrefabPropertyIntent::Set(crate::patch::asset_ref_value(
        &asset("textures/nested.png"),
    )));
    if let PrefabComponentIntent::Properties { claims, .. } = &mut nested {
        claims[0].path =
            ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("nested-image")),
            ]);
    }
    overlay.components.push(nested);
    let original = document(instance(overlay));
    let state = classify(&original, asset_basis(true), SceneFileLimits::default()).unwrap();
    let references = state.schema_declared_asset_references().collect::<Vec<_>>();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].asset_ref(), &asset("textures/nested.png"));
    assert!(references[0].path().segments().is_empty());
    assert_eq!(references[0].field_id(), &ComponentFieldId::new("settings"));
    assert_eq!(state.unavailable_manifest.overlay_target_count(), 0);
    assert_eq!(
        state
            .unavailable_manifest
            .overlay_declarations()
            .next()
            .unwrap()
            .1,
        original.entities[0].prefab.as_ref().unwrap()
    );
}

#[test]
fn obsolete_or_invalid_property_declarations_remain_repairable_and_exact() {
    let mut overlay = PrefabOverlay::new(key("base"));
    let mut obsolete = property(PrefabPropertyIntent::Set(ComponentValue::Null));
    if let PrefabComponentIntent::Properties { version, .. } = &mut obsolete {
        *version = ComponentSchemaVersion(9);
    }
    let mut nested = property(PrefabPropertyIntent::Set(crate::patch::asset_ref_value(
        &asset("textures/nested.png"),
    )));
    if let PrefabComponentIntent::Properties { claims, .. } = &mut nested {
        claims[0].path =
            ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("retired-image")),
            ]);
        claims[0].bindings.push(PrefabReferenceBinding {
            slot: ComponentFieldPath::empty(),
            expected: id("original"),
            target: PrefabEntityTarget::source(id("unresolved")),
        });
    }
    overlay.components.extend([
        obsolete,
        nested,
        property(PrefabPropertyIntent::Set(ComponentValue::U64(9))),
    ]);
    let original = instance(overlay);
    let state = classify(
        &document(original.clone()),
        asset_basis(true),
        SceneFileLimits::default(),
    )
    .unwrap();
    assert_eq!(state.readiness, AuthoringDocumentReadiness::Degraded);
    assert_eq!(state.unavailable_manifest.overlay_target_count(), 3);
    assert_eq!(
        state
            .unavailable_manifest
            .overlay_declarations()
            .next()
            .unwrap()
            .1,
        &original
    );
    let UnavailableRecordManifestEntry::OverlayTarget(target) =
        &state.unavailable_manifest.entries()[1]
    else {
        panic!("nested property declaration must be retained");
    };
    assert_eq!(target.property().unwrap().bindings.len(), 1);
    assert!(matches!(target.value(), Some(ComponentValue::Map(_))));
}

#[test]
fn source_readmission_round_trips_unresolved_intent_without_compiling_its_targets() {
    let mut overlay = PrefabOverlay::new(key("not-loaded"));
    let mut obsolete = property(PrefabPropertyIntent::Set(ComponentValue::String(
        "preserve".into(),
    )));
    if let PrefabComponentIntent::Properties {
        version, claims, ..
    } = &mut obsolete
    {
        *version = ComponentSchemaVersion(8);
        claims[0]
            .path
            .steps
            .push(ComponentPropertyStep::MapEntry("old-key".into()));
        claims[0].bindings.push(PrefabReferenceBinding {
            slot: ComponentFieldPath::empty(),
            expected: id("old-spelling"),
            target: PrefabEntityTarget::addition(id("missing-addition")),
        });
    }
    overlay.components.push(obsolete);
    let document = document(instance(overlay));
    let state = classify(&document, asset_basis(true), SceneFileLimits::default()).unwrap();
    let session =
        SceneAuthoringSession::from_published_authoring_document(PublishedAuthoringSceneDocument {
            document: document.clone(),
            state,
        });
    let admission = session.authoring_readmission().unwrap();
    for candidate in [
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap(),
        SceneDocumentCandidate::decode_ron_bytes(document.to_ron_string().unwrap().as_bytes())
            .unwrap(),
    ] {
        let reopened = admission.reopen_scene_candidate(candidate).unwrap();
        assert!(admission.preserves_scene(&reopened));
        assert_eq!(reopened.document(), &document);
    }
}

#[test]
fn invalid_complete_overlay_values_are_preserved_while_direct_record_admission_stays_strict() {
    let invalid = ComponentValue::map([
        ("image", ComponentValue::U64(9)),
        (
            "settings",
            ComponentValue::map([(
                "image",
                crate::patch::asset_ref_value(&asset("textures/understood.png")),
            )]),
        ),
    ]);
    let mut overlay = PrefabOverlay::new(key("base"));
    overlay.components.push(set(invalid.clone()));
    let state = classify(
        &document(instance(overlay)),
        asset_basis(true),
        SceneFileLimits::default(),
    )
    .unwrap();
    assert_eq!(state.readiness, AuthoringDocumentReadiness::Degraded);
    assert_eq!(state.unavailable_manifest.record_count(), 1);
    assert_eq!(
        state
            .schema_declared_asset_references()
            .next()
            .unwrap()
            .asset_ref(),
        &asset("textures/understood.png")
    );
    let direct = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("direct")).with_component(component(), record(invalid))
    ]);
    assert!(classify(&direct, asset_basis(true), SceneFileLimits::default()).is_err());
}

#[test]
fn source_transactions_preserve_both_snapshot_sides_through_scene_and_prefab_history() {
    let mut old = PrefabOverlay::new(key("old"));
    old.components
        .push(set(ComponentValue::String("old unavailable data".into())));
    let old = instance(old);
    let mut next = PrefabOverlay::new(key("next"));
    next.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("claimed")),
        value: Some(nested_instance()),
    });
    let next = instance(next);
    let mut document = document(old.clone());
    document.entities[0].components.insert(
        ComponentTypeId::new("test.direct.Unknown"),
        record(ComponentValue::String("direct".into())),
    );
    let state = classify(&document, empty_basis(), SceneFileLimits::default()).unwrap();
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(PublishedAuthoringSceneDocument {
            document: document.clone(),
            state: state.clone(),
        });
    let prefab_state = classify_scene_document(
        &document,
        state.source.clone(),
        AuthoringDocumentKind::Prefab,
        empty_basis(),
        SceneFileLimits::default(),
    )
    .unwrap();
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(
        PublishedAuthoringPrefabDocument {
            document: PrefabDocument {
                roots: document.roots.clone(),
                entities: document.entities.clone(),
            },
            state: prefab_state,
        },
    );
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("anchor"),
        expected: Some(old.clone()),
        value: Some(next.clone()),
    }]);
    assert!(scene.apply_authoring_patch(&patch).applied);
    assert_eq!(scene.document().entities[0].prefab.as_ref(), Some(&next));
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert_eq!(
        scene.unavailable_record_manifest(),
        Some(&state.unavailable_manifest)
    );
    assert!(scene.redo_authoring().applied);
    assert_eq!(scene.document().entities[0].prefab.as_ref(), Some(&next));
    assert!(prefab.apply_authoring_patch(&patch).applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(prefab.document().entities[0].prefab.as_ref(), Some(&old));
    assert!(prefab.redo_authoring().applied);
    assert_eq!(prefab.document().entities[0].prefab.as_ref(), Some(&next));
}
