#![cfg(feature = "serde")]

#[path = "support/prefab_source.rs"]
mod prefab_source;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringFieldEdit,
    AuthoringRecordLocation, AuthoringRecordReadiness, AuthoringSchemaBasis,
    PrefabAuthoringSession, PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance, PrefabOverlay,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};

const OWNER: &str = "test.lookup.bound";
const COMPONENT: &str = "test.lookup.Value";
const CURRENT: ComponentSchemaVersion = ComponentSchemaVersion(2);

#[test]
fn authoring_schema_lookup_requires_an_exact_available_direct_record_in_both_kinds() {
    let (bundle, adapters) = catalog();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    let mut overlay = PrefabOverlay::new(prefab_source::key("lookup.nested"));
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("overlay-only")),
        component: component(),
        record: record(CURRENT, OWNER),
        bindings: Vec::new(),
    });
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/nested.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    let document = SceneDocument::from_ordered_roots([
        entity("available", CURRENT, OWNER),
        entity("old", ComponentSchemaVersion::ONE, OWNER),
        entity("future", ComponentSchemaVersion(3), OWNER),
        entity("wrong-owner", CURRENT, "test.lookup.unbound"),
        SceneEntityRecord::new(id("unbound")).with_component(
            ComponentTypeId::new("test.lookup.Unbound"),
            record(CURRENT, "test.lookup.unbound"),
        ),
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("test.lookup.Unknown"),
            record(CURRENT, OWNER),
        ),
        SceneEntityRecord::new(id("empty")),
        anchor,
    ]);
    let (scene, prefab) = sessions(&document, &bundle, &adapters);
    assert_eq!(
        scene
            .authoring_component_schema(&id("available"), &component())
            .unwrap()
            .version(),
        CURRENT
    );
    assert_eq!(
        prefab
            .authoring_component_schema(&id("available"), &component())
            .unwrap()
            .version(),
        CURRENT
    );
    for (entity, component) in [
        ("old", COMPONENT),
        ("future", COMPONENT),
        ("wrong-owner", COMPONENT),
        ("unbound", "test.lookup.Unbound"),
        ("unknown", "test.lookup.Unknown"),
        ("empty", COMPONENT),
        ("missing", COMPONENT),
        ("overlay-only", COMPONENT),
        ("available", "test.lookup.Unknown"),
    ] {
        assert!(
            scene
                .authoring_component_schema(&id(entity), &ComponentTypeId::new(component))
                .is_none(),
            "{entity}/{component}"
        );
        assert!(
            prefab
                .authoring_component_schema(&id(entity), &ComponentTypeId::new(component))
                .is_none(),
            "{entity}/{component}"
        );
    }
    for (entity, expected) in [
        ("old", AuthoringRecordReadiness::Unmigratable),
        ("future", AuthoringRecordReadiness::FutureSchema),
        ("wrong-owner", AuthoringRecordReadiness::UnknownSchema),
        ("unbound", AuthoringRecordReadiness::KnownUnbound),
        ("unknown", AuthoringRecordReadiness::UnknownSchema),
    ] {
        let fact = scene.authoring_record_facts().unwrap().iter().find(|fact| {
            matches!(fact.location(), AuthoringRecordLocation::EntityComponent { entity: owner } if owner == &id(entity))
        }).unwrap();
        assert_eq!(fact.readiness(), expected);
    }
    assert!(scene.authoring_record_facts().unwrap().iter().any(|fact| {
        fact.readiness() == AuthoringRecordReadiness::Available
            && matches!(fact.location(), AuthoringRecordLocation::OverlayComponent { target, .. } if target == &PrefabEntityTarget::source(id("overlay-only")))
    }), "an Available overlay record must not become direct schema authority");
    let native = SceneAuthoringSession::new(document.clone());
    let native_prefab =
        PrefabAuthoringSession::new(PrefabDocument::new(document.roots, document.entities));
    assert!(
        native
            .authoring_component_schema(&id("available"), &component())
            .is_none()
    );
    assert!(
        native_prefab
            .authoring_component_schema(&id("available"), &component())
            .is_none()
    );
}

#[test]
fn authoring_schema_lookup_tracks_component_edits_and_exact_history_in_both_kinds() {
    let (bundle, adapters) = catalog();
    let document = SceneDocument::from_ordered_roots([entity("root", CURRENT, OWNER)]);
    let (mut scene, mut prefab) = sessions(&document, &bundle, &adapters);
    let removed = ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
        entity: id("root"),
        component: component(),
    }]);
    assert!(scene.apply_authoring_patch(&removed).applied);
    assert!(prefab.apply_authoring_patch(&removed).applied);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(scene.redo_authoring().applied);
    assert!(prefab.redo_authoring().applied);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    let edit = AuthoringFieldEdit::single(
        id("root"),
        component(),
        CURRENT,
        ComponentFieldId::new("value"),
        ComponentValue::U64(7),
    )
    .unwrap();
    assert!(scene.apply_authoring_field_edit(edit.clone()).applied);
    assert!(prefab.apply_authoring_field_edit(edit).applied);
    assert_eq!(
        scene.document().entities[0].components[&component()]
            .value
            .field("value")
            .unwrap(),
        &ComponentValue::U64(7)
    );
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
}

#[test]
fn authoring_schema_lookup_is_rebuilt_for_reopened_classification_without_mutating_origin() {
    let (bundle, adapters) = catalog();
    let document = SceneDocument::from_ordered_roots([entity("root", CURRENT, OWNER)]);
    let (scene, prefab) = sessions(&document, &bundle, &adapters);
    let changed =
        SceneDocument::from_ordered_roots([entity("root", ComponentSchemaVersion(3), OWNER)]);
    let reopened = scene
        .authoring_readmission()
        .unwrap()
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_json_bytes(changed.to_json_string().unwrap().as_bytes())
                .unwrap(),
        )
        .unwrap();
    let reopened_prefab = prefab
        .authoring_readmission()
        .unwrap()
        .reopen_prefab_candidate(
            PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(changed.roots, changed.entities)
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        reopened
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        reopened_prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert_eq!(
        reopened.authoring_record_facts().unwrap()[0].readiness(),
        AuthoringRecordReadiness::FutureSchema
    );
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert_eq!(scene.document(), &document);
    let restored = reopened
        .authoring_readmission()
        .unwrap()
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    let restored_prefab = reopened_prefab
        .authoring_readmission()
        .unwrap()
        .reopen_prefab_candidate(
            PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(document.roots.clone(), document.entities.clone())
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        restored
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        restored_prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
}

#[test]
fn shared_authoring_basis_is_retained_across_independent_scene_and_prefab_sources() {
    let basis = {
        let (bundle, adapters) = catalog();
        AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
    };
    let first_document = SceneDocument::from_ordered_roots([entity("first", CURRENT, OWNER)]);
    let first = SceneDocumentCandidate::decode_json_bytes(
        first_document.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring_with_basis(source("scenes/first.scene.json"), &basis.clone())
    .unwrap()
    .publish();
    let second_document = SceneDocument::from_ordered_roots([entity("second", CURRENT, OWNER)]);
    let second = SceneDocumentCandidate::decode_json_bytes(
        second_document.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring_with_basis(source("scenes/second.scene.json"), &first.schema_basis())
    .unwrap()
    .publish();
    let prefab_document = PrefabDocument::from_ordered_roots([entity("prefab", CURRENT, OWNER)]);
    let prefab = PrefabDocumentCandidate::decode_json_bytes(
        prefab_document.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring_with_basis(source("prefabs/shared.prefab.json"), &second.schema_basis())
    .unwrap()
    .publish();

    assert_eq!(first.schema_basis(), basis);
    assert_eq!(second.schema_basis(), basis);
    assert_eq!(prefab.schema_basis(), basis);
    drop(basis);
    assert_eq!(first.schema_basis(), second.schema_basis());
    assert_eq!(first.schema_basis(), prefab.schema_basis());
    assert_eq!(first.document(), &first_document);
    assert_eq!(second.document(), &second_document);
    assert_eq!(prefab.document(), &prefab_document);
    for facts in [
        first.record_facts(),
        second.record_facts(),
        prefab.record_facts(),
    ] {
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].readiness(), AuthoringRecordReadiness::Available);
        assert_eq!(facts[0].resolved_owner(), Some(&owner(OWNER)));
        assert!(!facts[0].adapter().unwrap().supports_native_preview());
    }
}

#[test]
fn independent_basis_and_convenience_admissions_keep_distinct_identities() {
    let (bundle, adapters) = catalog();
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    assert_ne!(
        basis,
        AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
    );
    let document = SceneDocument::from_ordered_roots([entity("root", CURRENT, OWNER)]);
    let encoded = document.to_json_string().unwrap();
    let first = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .admit_authoring(source("scenes/independent.scene.json"), &bundle, &adapters)
        .unwrap()
        .publish();
    let second = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .admit_authoring(source("scenes/independent.scene.json"), &bundle, &adapters)
        .unwrap()
        .publish();
    let prefab_document = PrefabDocument::new(document.roots, document.entities);
    let prefab = PrefabDocumentCandidate::decode_json_bytes(
        prefab_document.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring(
        source("prefabs/independent.prefab.json"),
        &bundle,
        &adapters,
    )
    .unwrap()
    .publish();

    assert_eq!(first.document(), second.document());
    assert_eq!(first.record_facts(), second.record_facts());
    assert_ne!(first.schema_basis(), basis);
    assert_ne!(first.schema_basis(), second.schema_basis());
    assert_ne!(prefab.schema_basis(), basis);
    assert_ne!(prefab.schema_basis(), first.schema_basis());
}

#[test]
fn shared_basis_preserves_exact_owner_version_and_adapter_classification_in_both_kinds() {
    let (bundle, adapters) = catalog();
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let document = SceneDocument::from_ordered_roots([
        entity("available", CURRENT, OWNER),
        entity("old", ComponentSchemaVersion::ONE, OWNER),
        entity("future", ComponentSchemaVersion(3), OWNER),
        entity("wrong-owner", CURRENT, "test.lookup.unbound"),
        SceneEntityRecord::new(id("unbound")).with_component(
            ComponentTypeId::new("test.lookup.Unbound"),
            record(CURRENT, "test.lookup.unbound"),
        ),
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("test.lookup.Unknown"),
            record(CURRENT, OWNER),
        ),
        SceneEntityRecord::new(id("unclaimed")).with_component(
            component(),
            SceneComponentRecord::new(
                CURRENT,
                ComponentValue::map([("value", ComponentValue::U64(1))]),
            ),
        ),
    ]);
    let scene =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring_with_basis(source("scenes/classified.scene.json"), &basis)
            .unwrap()
            .publish();
    let prefab_document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let prefab = PrefabDocumentCandidate::decode_json_bytes(
        prefab_document.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring_with_basis(source("prefabs/classified.prefab.json"), &basis)
    .unwrap()
    .publish();

    assert_eq!(scene.schema_basis(), basis);
    assert_eq!(prefab.schema_basis(), basis);
    assert_eq!(scene.document(), &document);
    assert_eq!(prefab.document(), &prefab_document);
    assert_eq!(scene.record_facts(), prefab.record_facts());
    for (entity, expected, expected_owner) in [
        (
            "available",
            AuthoringRecordReadiness::Available,
            Some(OWNER),
        ),
        ("old", AuthoringRecordReadiness::Unmigratable, Some(OWNER)),
        (
            "future",
            AuthoringRecordReadiness::FutureSchema,
            Some(OWNER),
        ),
        ("wrong-owner", AuthoringRecordReadiness::UnknownSchema, None),
        (
            "unbound",
            AuthoringRecordReadiness::KnownUnbound,
            Some("test.lookup.unbound"),
        ),
        ("unknown", AuthoringRecordReadiness::UnknownSchema, None),
        (
            "unclaimed",
            AuthoringRecordReadiness::Available,
            Some(OWNER),
        ),
    ] {
        let fact = scene.record_facts().iter().find(|fact| {
            matches!(fact.location(), AuthoringRecordLocation::EntityComponent { entity: owner } if owner == &id(entity))
        }).unwrap();
        assert_eq!(fact.readiness(), expected, "{entity}");
        assert_eq!(
            fact.resolved_owner(),
            expected_owner.map(owner).as_ref(),
            "{entity}"
        );
        assert_eq!(
            fact.adapter().is_some(),
            expected.is_available(),
            "{entity}"
        );
    }
}

#[test]
fn shared_basis_admission_rejects_conflicting_exact_adapter_coverage() {
    let (bundle, adapters) = catalog();
    let error = AuthoringSchemaBasis::admit(&bundle, &[adapters[0].clone(), adapters[0].clone()])
        .unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authoring-adapter-coverage-conflict"
    }));
}

#[test]
fn data_only_prefab_compilation_preserves_pins_and_typed_omissions_without_native_authority() {
    use nara_reflect::ComponentPropertyPath;
    use nara_scene::{
        __private::{
            AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason,
            try_expand_authoring_prefabs_with_loader,
        },
        PrefabPropertyClaim, PrefabPropertyIntent, ResolvedPrefabSource,
    };
    use std::{convert::Infallible, sync::Arc};

    let (bundle, adapters) = catalog();
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let prefab = PrefabDocument::from_ordered_roots([entity("leaf", CURRENT, OWNER)]);
    let published = Arc::new(
        PrefabDocumentCandidate::decode_json_str(&prefab.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(source("prefabs/shared.json"), &basis)
            .unwrap()
            .publish(),
    );
    let request = AssetRef::path("shared.json").unwrap();
    let key = prefab_source::key("data-only");
    let mut overlay = PrefabOverlay::new(key.clone());
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("leaf")),
        component: component(),
        version: CURRENT,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("value")),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(7)),
            bindings: Vec::new(),
        }],
    });
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: request.clone(),
        overrides: Some(overlay),
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let before = document.clone();
    let mut observed = Vec::new();
    let report = try_expand_authoring_prefabs_with_loader(
        &document,
        &basis,
        Default::default(),
        None,
        |reference, working| {
            observed.push((reference.clone(), working.source_depth()));
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring(
                Arc::clone(&published),
                key.clone(),
            )))
        },
    )
    .unwrap();
    assert_eq!(observed, [(request, 1)]);
    assert!(!report.budget_exceeded());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let candidate = report.candidate.as_ref().unwrap();
    let (projected, provenance) = candidate
        .provenance()
        .iter()
        .find(|(_, origin)| origin.projection_path().is_some())
        .unwrap();
    assert_eq!(provenance.local_entity_id(), &id("leaf"));
    assert_eq!(
        candidate
            .document()
            .entities
            .iter()
            .find(|entity| &entity.id == projected)
            .unwrap()
            .components[&component()]
            .value
            .field_u64("value")
            .unwrap(),
        7
    );
    let expected = candidate.provenance().clone();
    let (native_preview, omissions) = report.into_native_preview().unwrap();
    assert_eq!(native_preview.provenance(), &expected);
    assert!(
        native_preview
            .document()
            .entities
            .iter()
            .all(|entity| entity.components.is_empty())
    );
    assert_eq!(omissions.len(), 1);
    assert_eq!(
        omissions[0].reason(),
        AuthoringPreviewOmissionReason::NativeBindingUnavailable
    );
    assert!(matches!(omissions[0].location(),
        AuthoringPreviewOmissionLocation::ProjectedEntityComponent { provenance, component: omitted, .. }
            if provenance.projection_path().is_some() && omitted == &component()));
    assert_eq!(document, before);
    assert_eq!(published.document(), &prefab);
}

#[test]
fn prefab_compilation_rejects_an_independently_admitted_identical_basis() {
    use nara_scene::{__private::try_expand_authoring_prefabs_with_loader, ResolvedPrefabSource};
    use std::{convert::Infallible, sync::Arc};

    let (bundle, adapters) = catalog();
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let other = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let prefab = PrefabDocument::from_ordered_roots([entity("leaf", CURRENT, OWNER)]);
    let published = Arc::new(
        PrefabDocumentCandidate::decode_json_str(&prefab.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(source("prefabs/shared.json"), &other)
            .unwrap()
            .publish(),
    );
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("shared.json").unwrap(),
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let report = try_expand_authoring_prefabs_with_loader(
        &document,
        &basis,
        Default::default(),
        None,
        |_, _| {
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring(
                Arc::clone(&published),
                prefab_source::key("data-only"),
            )))
        },
    )
    .unwrap();
    assert!(report.candidate.is_none());
    assert!(report.diagnostics.has_errors());
    assert!(!report.budget_exceeded());
    assert_eq!(published.document(), &prefab);
}

fn catalog() -> (AuthoringSchemaBundle, Vec<AuthoringAdapterReceipt>) {
    let bundle = AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(
            owner(OWNER),
            ComponentSchemaCatalog {
                components: vec![schema(COMPONENT)],
                ..ComponentSchemaCatalog::default()
            },
            None,
        ),
        AuthoringSchemaBundleOwner::new(
            owner("test.lookup.unbound"),
            ComponentSchemaCatalog {
                components: vec![schema("test.lookup.Unbound")],
                ..ComponentSchemaCatalog::default()
            },
            None,
        ),
    ])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    (bundle, vec![adapter])
}

fn schema(component: &str) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(component), "Lookup value", CURRENT)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]))
}

fn sessions(
    document: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> (SceneAuthoringSession, PrefabAuthoringSession) {
    let scene =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/lookup.scene.json"), bundle, adapters)
            .unwrap()
            .publish();
    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let prefab =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("prefabs/lookup.prefab.json"), bundle, adapters)
            .unwrap()
            .publish();
    (
        SceneAuthoringSession::from_published_authoring_document(scene),
        PrefabAuthoringSession::from_published_authoring_document(prefab),
    )
}

fn entity(entity: &str, version: ComponentSchemaVersion, owner: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(entity)).with_component(component(), record(version, owner))
}

fn record(version: ComponentSchemaVersion, declared_owner: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        version,
        ComponentValue::map([("value", ComponentValue::U64(1))]),
    )
    .with_owner(owner(declared_owner))
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new(COMPONENT)
}
fn owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}
fn source(value: &str) -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path(value).unwrap())
}
