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
    });
    anchor.prefab = Some(PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: AssetRef::path("prefabs/nested.prefab.json").unwrap(),
        arguments: None,
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
        PrefabAuthoringSession::new(PrefabDocument::new(document.placements, document.entities));
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
                PrefabDocument::new(changed.placements, changed.entities)
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
                PrefabDocument::new(document.placements.clone(), document.entities.clone())
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
    let prefab_document = PrefabDocument::new(document.placements, document.entities);
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
    let prefab_document =
        PrefabDocument::new(document.placements.clone(), document.entities.clone());
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
            try_compile_authoring_scene_with_loader,
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
        }],
    });
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: request.clone(),
        arguments: None,
        overrides: Some(overlay),
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let before = document.clone();
    let mut observed = Vec::new();
    let report = try_compile_authoring_scene_with_loader(
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
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    let candidate = report.candidate().unwrap();
    let (projected, provenance) = candidate
        .provenance()
        .iter()
        .find(|(_, origin)| origin.projection_path().is_some())
        .unwrap();
    assert_eq!(provenance.local_entity_id(), &id("leaf"));
    assert_eq!(
        candidate
            .entities()
            .iter()
            .find(|entity| entity.id() == projected)
            .unwrap()
            .components()[&component()]
            .value
            .field_u64("value")
            .unwrap(),
        7
    );
    let expected = candidate.provenance().clone();
    let nara_scene::__private::AuthoringNativePreview {
        candidate: native_preview,
        omissions,
        ..
    } = report.into_native_preview().unwrap();
    assert_eq!(native_preview.canonical_provenance(), &expected);
    assert!(
        native_preview
            .entities()
            .iter()
            .all(|entity| entity.components().is_empty())
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
    use nara_scene::{__private::try_compile_authoring_scene_with_loader, ResolvedPrefabSource};
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
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: AssetRef::path("shared.json").unwrap(),
        arguments: None,
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let report = try_compile_authoring_scene_with_loader(
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
    assert!(report.candidate().is_none());
    assert!(report.diagnostics().has_errors());
    assert!(!report.budget_exceeded());
    assert_eq!(published.document(), &prefab);
}

#[test]
fn authoring_omits_unavailable_complete_overlay_records_but_refuses_property_interpretation() {
    use nara_reflect::ComponentPropertyPath;
    use nara_scene::{
        __private::{
            AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason,
            AuthoringSceneCompilationOmissionReason, try_compile_authoring_scene_with_loader,
        },
        PrefabAddition, PrefabEntityOrigin, PrefabInstanceHop, PrefabPropertyClaim,
        PrefabPropertyIntent, ResolvedPrefabSource, SceneCompilationFailureKind,
        SceneEntityPlacement, ScenePlacement, ScenePlacementParent,
    };
    use std::{convert::Infallible, sync::Arc};

    let (bundle, adapters) = catalog();
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let u_key = prefab_source::key("unavailable-overlay-u");
    let v_key = prefab_source::key("unavailable-overlay-v");
    let u_request = AssetRef::path("unavailable-overlay-u.json").unwrap();
    let v_request = AssetRef::path("unavailable-overlay-v.json").unwrap();
    let v = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("leaf")),
    ]));
    let v_before = v.as_ref().clone();
    for (component, version, declared_owner, reason) in [
        (
            COMPONENT,
            CURRENT,
            OWNER,
            AuthoringRecordReadiness::Available,
        ),
        (
            "missing.Unknown",
            CURRENT,
            OWNER,
            AuthoringRecordReadiness::UnknownSchema,
        ),
        (
            COMPONENT,
            ComponentSchemaVersion(3),
            OWNER,
            AuthoringRecordReadiness::FutureSchema,
        ),
        (
            COMPONENT,
            ComponentSchemaVersion::ONE,
            OWNER,
            AuthoringRecordReadiness::Unmigratable,
        ),
        (
            COMPONENT,
            CURRENT,
            "different.owner",
            AuthoringRecordReadiness::UnknownSchema,
        ),
        (
            "test.lookup.Unbound",
            CURRENT,
            "test.lookup.unbound",
            AuthoringRecordReadiness::KnownUnbound,
        ),
    ] {
        for addition in [false, true] {
            for pin in [false, true] {
                let component = ComponentTypeId::new(component);
                let mut record = SceneComponentRecord::new(
                    version,
                    ComponentValue::map([("value", ComponentValue::U64(9))]),
                )
                .with_owner(owner(declared_owner));
                if !reason.is_available() {
                    record.value = ComponentValue::map([
                        ("value", ComponentValue::U64(9)),
                        (
                            "unknown",
                            ComponentValue::String("retained raw payload".into()),
                        ),
                    ]);
                    record = record.with_contextual_default(
                        ComponentFieldId::new("future-default"),
                        ComponentValue::String("retained unknown default".into()),
                    );
                }
                let target = if addition {
                    PrefabEntityTarget::addition(id("extra"))
                } else {
                    PrefabEntityTarget::source(id("leaf"))
                };
                // U owns the complete record; the Scene can only pin it from an outer layer.
                let mut inner = PrefabOverlay::new(v_key.clone());
                if addition {
                    let mut extra = PrefabAddition::new(id("extra"));
                    extra.components.insert(component.clone(), record);
                    inner.additions.push(extra);
                    inner.placements.push(SceneEntityPlacement::new(
                        target.clone(),
                        ScenePlacement::last(ScenePlacementParent::Root),
                    ));
                } else {
                    inner.components.push(PrefabComponentIntent::Set {
                        mode: PrefabComponentSetMode::Add,
                        target: target.clone(),
                        component: component.clone(),
                        record,
                    });
                }
                let mut u_anchor = SceneEntityRecord::new(id("u"));
                let mut inner_instance = PrefabInstance::forest(v_request.clone());
                inner_instance.overrides = Some(inner);
                u_anchor.prefab = Some(inner_instance);
                let u = Arc::new(PrefabDocument::from_ordered_roots([u_anchor]));
                let u_before = u.as_ref().clone();

                let mut outer = PrefabOverlay::new(u_key.clone());
                if pin {
                    outer.components.push(PrefabComponentIntent::Properties {
                        target: PrefabEntityTarget {
                            through: vec![PrefabInstanceHop {
                                anchor: PrefabEntityOrigin::SourceEntity(id("u")),
                                expected_base: v_key.clone(),
                            }],
                            entity: target.entity.clone(),
                        },
                        component: component.clone(),
                        version,
                        claims: vec![PrefabPropertyClaim {
                            path: ComponentPropertyPath::new(ComponentFieldId::new("value")),
                            intent: PrefabPropertyIntent::Set(ComponentValue::U64(11)),
                        }],
                    });
                }
                let mut anchor = SceneEntityRecord::new(id("anchor"));
                let mut instance = PrefabInstance::forest(u_request.clone());
                instance.overrides = Some(outer);
                anchor.prefab = Some(instance);
                let document = SceneDocument::from_ordered_roots([anchor]);
                let before = document.clone();
                let mut observed = Vec::new();
                let report = try_compile_authoring_scene_with_loader(
                    &document,
                    &basis,
                    Default::default(),
                    None,
                    |actual, working| {
                        observed.push((actual.clone(), working.source_depth()));
                        let resolved = if actual == &u_request {
                            ResolvedPrefabSource::bound_authoring_source(
                                Arc::clone(&u),
                                basis.clone(),
                                u_key.clone(),
                            )
                        } else {
                            assert_eq!(actual, &v_request);
                            ResolvedPrefabSource::bound_authoring_source(
                                Arc::clone(&v),
                                basis.clone(),
                                v_key.clone(),
                            )
                        };
                        Ok::<_, Infallible>(Some(resolved))
                    },
                )
                .unwrap();
                assert_eq!(observed, [(u_request.clone(), 1), (v_request.clone(), 2)]);
                assert!(!report.budget_exceeded());
                if pin && !reason.is_available() {
                    assert!(
                        report.candidate().is_none(),
                        "unavailable record must not admit field interpretation"
                    );
                    assert_eq!(
                        report.failure().unwrap().reason(),
                        &SceneCompilationFailureKind::Component,
                        "{reason:?}, addition={addition}"
                    );
                } else {
                    assert!(report.failure().is_none(), "{report:?}");
                    assert!(!report.diagnostics().has_errors(), "{report:?}");
                    let candidate = report
                        .candidate()
                        .unwrap_or_else(|| panic!("{:?}", report.diagnostics()));
                    let (projected, provenance) = candidate
                        .provenance()
                        .iter()
                        .find(|(_, provenance)| {
                            provenance.local_entity_id() == target.entity.entity()
                        })
                        .unwrap();
                    assert!(provenance.projection_path().is_some());
                    let projected = projected.clone();
                    assert_eq!(report.omissions().len(), 1);
                    assert_eq!(report.omissions()[0].component, component);
                    assert_eq!(report.omissions()[0].entity, projected);
                    if reason.is_available() {
                        let record = &candidate
                            .entities()
                            .iter()
                            .find(|entity| entity.id() == &projected)
                            .unwrap()
                            .components()[&component];
                        assert_eq!(record.version, CURRENT);
                        assert_eq!(record.owner(), Some(&owner(OWNER)));
                        assert_eq!(
                            record.value.field_u64("value").unwrap(),
                            if pin { 11 } else { 9 }
                        );
                        assert!(record.contextual_defaults.is_empty());
                        assert_eq!(
                            report.omissions()[0].reason,
                            AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable
                        );
                    } else {
                        assert!(
                            candidate
                                .entities()
                                .iter()
                                .all(|entity| entity.components().is_empty())
                        );
                        assert_eq!(
                            report.omissions()[0].reason,
                            AuthoringSceneCompilationOmissionReason::RecordUnavailable(reason)
                        );
                        let preview = report.into_native_preview().unwrap();
                        assert!(preview.failure.is_none());
                        assert!(!preview.diagnostics.has_errors());
                        assert!(
                            preview
                                .candidate
                                .entities()
                                .iter()
                                .all(|entity| entity.components().is_empty())
                        );
                        assert_eq!(preview.omissions.len(), 1);
                        assert_eq!(
                            preview.omissions[0].reason(),
                            AuthoringPreviewOmissionReason::RecordUnavailable(reason)
                        );
                        assert!(matches!(preview.omissions[0].location(),
                            AuthoringPreviewOmissionLocation::ProjectedEntityComponent { entity, component: omitted, .. }
                                if entity == &projected && omitted == &component));
                    }
                }
                assert_eq!(document, before);
                assert_eq!(u.as_ref(), &u_before);
                assert_eq!(v.as_ref(), &v_before);
            }
        }
    }
}

#[test]
fn authoring_rejects_current_complete_overlay_records_without_scene_capability_even_when_unbound() {
    use nara_scene::{
        __private::{
            AuthoringSceneCompilationOmissionReason, try_compile_authoring_scene_with_loader,
        },
        PrefabAddition, ResolvedPrefabSource, SceneCompilationFailureKind, SceneEntityPlacement,
        ScenePlacement, ScenePlacementParent,
    };
    use std::{convert::Infallible, sync::Arc};

    let component = ComponentTypeId::new("test.lookup.NoScene");
    let schema = ComponentSchema::new(component.clone(), "No Scene value", CURRENT).with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        ),
    ]);
    assert!(schema.capabilities.is_empty());
    assert!(schema.fields[0].capabilities.is_empty());
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(OWNER),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    let key = prefab_source::key("no-scene-overlay");
    let request = AssetRef::path("no-scene-overlay.json").unwrap();
    for (version, declared_owner, unavailable) in [
        (CURRENT, OWNER, None),
        (
            ComponentSchemaVersion::ONE,
            OWNER,
            Some(AuthoringRecordReadiness::Unmigratable),
        ),
        (
            ComponentSchemaVersion(3),
            OWNER,
            Some(AuthoringRecordReadiness::FutureSchema),
        ),
        (
            CURRENT,
            "different.owner",
            Some(AuthoringRecordReadiness::UnknownSchema),
        ),
    ] {
        for mode in [
            None,
            Some(PrefabComponentSetMode::Add),
            Some(PrefabComponentSetMode::Replace),
        ] {
            let mut leaf = SceneEntityRecord::new(id("leaf"));
            if mode == Some(PrefabComponentSetMode::Replace) {
                leaf.components
                    .insert(component.clone(), record(CURRENT, OWNER));
            }
            let prefab = Arc::new(PrefabDocument::from_ordered_roots([leaf]));
            let prefab_before = prefab.as_ref().clone();
            let mut selected = SceneComponentRecord::new(
                version,
                ComponentValue::map([("value", ComponentValue::U64(9))]),
            )
            .with_owner(owner(declared_owner));
            if unavailable.is_some() {
                selected.value = ComponentValue::String("uninterpreted raw value".into());
                selected = selected.with_contextual_default(
                    ComponentFieldId::new("unknown-default"),
                    ComponentValue::String("uninterpreted raw default".into()),
                );
            }
            let mut overlay = PrefabOverlay::new(key.clone());
            if let Some(mode) = mode {
                overlay.components.push(PrefabComponentIntent::Set {
                    mode,
                    target: PrefabEntityTarget::source(id("leaf")),
                    component: component.clone(),
                    record: selected,
                });
            } else {
                let mut extra = PrefabAddition::new(id("extra"));
                extra.components.insert(component.clone(), selected);
                overlay.additions.push(extra);
                overlay.placements.push(SceneEntityPlacement::new(
                    PrefabEntityTarget::addition(id("extra")),
                    ScenePlacement::last(ScenePlacementParent::Root),
                ));
            }
            let mut anchor = SceneEntityRecord::new(id("anchor"));
            let mut instance = PrefabInstance::forest(request.clone());
            instance.overrides = Some(overlay);
            anchor.prefab = Some(instance);
            let document = SceneDocument::from_ordered_roots([anchor]);
            let before = document.clone();
            let mut observed = Vec::new();
            let report = try_compile_authoring_scene_with_loader(
                &document,
                &basis,
                Default::default(),
                None,
                |actual, working| {
                    observed.push((actual.clone(), working.source_depth()));
                    assert_eq!(actual, &request);
                    Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                        Arc::clone(&prefab),
                        basis.clone(),
                        key.clone(),
                    )))
                },
            )
            .unwrap();
            assert_eq!(observed, [(request.clone(), 1)]);
            assert!(!report.budget_exceeded());
            if let Some(reason) = unavailable {
                assert!(report.failure().is_none(), "{report:?}");
                assert!(!report.diagnostics().has_errors(), "{report:?}");
                let candidate = report.candidate().unwrap();
                assert!(
                    candidate
                        .entities()
                        .iter()
                        .all(|entity| entity.components().is_empty())
                );
                assert_eq!(report.omissions().len(), 1);
                assert_eq!(report.omissions()[0].component, component);
                assert_eq!(
                    report.omissions()[0].reason,
                    AuthoringSceneCompilationOmissionReason::RecordUnavailable(reason)
                );
            } else {
                assert!(report.candidate().is_none(), "{mode:?}: {report:?}");
                assert_eq!(
                    report.failure().unwrap().reason(),
                    &SceneCompilationFailureKind::Component,
                    "{mode:?}"
                );
            }
            assert_eq!(document, before);
            assert_eq!(prefab.as_ref(), &prefab_before);
        }
    }
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
    let prefab = PrefabDocument::new(document.placements.clone(), document.entities.clone());
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
