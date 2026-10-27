use std::sync::Arc;

use super::*;
use crate::{
    AuthoringDocumentSource, PrefabAddition, PrefabComponentSetMode, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstanceClaim, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabPropertyClaim, PrefabPropertyIntent,
    SceneAuthoringHistoryLimits, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SourceDocumentKey, SourceDocumentKind,
};
use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn limits() -> PrefabDeclarationLimits {
    PrefabDeclarationLimits::default()
}

fn root_path() -> PrefabInstancePath {
    PrefabInstancePath::new(id("anchor"))
}

fn nested_path(steps: &[PrefabInstancePathSegment]) -> PrefabInstancePath {
    PrefabInstancePath {
        anchor: id("anchor"),
        steps: steps.to_vec(),
    }
}

fn overlay() -> PrefabOverlay {
    PrefabOverlay::new(
        SourceDocumentKey::named(
            "5e7c0f41-a1f7-4ce4-bb69-e4927f4f661b".parse().unwrap(),
            "base",
            SourceDocumentKind::Prefab,
        )
        .unwrap(),
    )
}

fn instance(overlay: PrefabOverlay) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: Some(overlay),
    }
}

fn opaque(value: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::String(value.into()),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("missing.owner").unwrap())
}

fn unknown_intent(value: &str) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("inherited")),
        component: ComponentTypeId::new("missing.Component"),
        record: opaque(value),
        bindings: Vec::new(),
    }
}

fn properties() -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("inherited")),
        component: ComponentTypeId::new("missing.Properties"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("payload")),
            intent: PrefabPropertyIntent::Set(ComponentValue::String("remove only me".into())),
            bindings: Vec::new(),
        }],
    }
}

fn document(declaration: Option<PrefabInstance>) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor")).with_component(
        ComponentTypeId::new("missing.Direct"),
        opaque("keep direct"),
    );
    anchor.prefab = declaration;
    anchor.children.push(id("child"));
    SceneDocument::new(
        [id("anchor")],
        [anchor, SceneEntityRecord::new(id("child"))],
    )
}

fn basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn scene(document: &SceneDocument) -> SceneAuthoringSession {
    scene_with_history_limits(document, SceneAuthoringHistoryLimits::default())
}

fn scene_with_history_limits(
    document: &SceneDocument,
    history_limits: SceneAuthoringHistoryLimits,
) -> SceneAuthoringSession {
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/main.scene.json").unwrap()),
                &basis(),
            )
            .unwrap()
            .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        published,
        history_limits,
    )
}

fn prefab(document: &SceneDocument) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/main.prefab.json").unwrap()),
                &basis(),
            )
            .unwrap()
            .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

#[test]
fn property_revert_preserves_unknown_siblings_and_one_exact_history_entry() {
    let mut original = overlay();
    original.components.push(properties());
    original.components.push(unknown_intent("keep sibling"));
    let mut addition = PrefabAddition::new(id("opaque_addition"));
    addition.components.insert(
        ComponentTypeId::new("missing.Added"),
        opaque("keep addition"),
    );
    original.additions.push(addition);
    let document = document(Some(instance(original.clone())));
    let mut session = scene(&document);
    let untouched = session.shared_document();
    let observation = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    assert_eq!(observation.revision, session.revision());
    assert_eq!(Some(observation.schema), session.authoring_schema_basis());
    assert_eq!(observation.instance, document.entities[0].prefab);
    let prepared = session
        .prepare_prefab_revert(
            session.revision(),
            &root_path(),
            PrefabRevert::Intent(PrefabOverlaySlot::Property {
                component_index: 0,
                claim_index: 0,
            }),
            limits(),
        )
        .unwrap();
    let mut expected = document.clone();
    let expected_overlay = expected.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    expected_overlay.components.remove(0);
    assert_eq!(prepared.document(), &expected);
    assert_eq!(prepared.patch().operations.len(), 1);
    assert!(matches!(&prepared.patch().operations[0],
        ScenePatchOperation::SetPrefabInstance { entity, expected, value }
        if entity == &id("anchor")
            && expected == &document.entities[0].prefab
            && value == &prepared.document().entities[0].prefab));
    assert_eq!(session.document(), &document);
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &expected);
    assert_eq!(untouched.as_ref(), &document);
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
}

#[test]
fn final_property_revert_keeps_empty_base_pin_and_overlay_revert_keeps_source() {
    let mut original = overlay();
    original.components.push(properties());
    let document = document(Some(instance(original.clone())));
    let mut session = prefab(&document);
    let prepared = session
        .prepare_prefab_revert(
            session.revision(),
            &root_path(),
            PrefabRevert::Intent(PrefabOverlaySlot::Property {
                component_index: 0,
                claim_index: 0,
            }),
            limits(),
        )
        .unwrap();
    let selected = prepared.document().entities[0].prefab.as_ref().unwrap();
    assert_eq!(selected.overrides, Some(overlay()));
    assert_eq!(
        selected.source,
        document.entities[0].prefab.as_ref().unwrap().source
    );
    assert!(session.commit_authoring_edit(prepared).applied);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document().entities, document.entities);
    assert!(session.redo_authoring().applied);
    let prepared = session
        .prepare_prefab_revert(
            session.revision(),
            &root_path(),
            PrefabRevert::Overlay,
            limits(),
        )
        .unwrap();
    let selected = prepared.document().entities[0].prefab.as_ref().unwrap();
    assert_eq!(selected.overrides, None);
    assert_eq!(
        selected.source,
        document.entities[0].prefab.as_ref().unwrap().source
    );
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.history_status().undo_depth, 2);
    assert!(session.undo_authoring().applied);
    assert_eq!(
        session
            .observe_prefab_instance(&root_path(), limits())
            .unwrap()
            .instance
            .unwrap()
            .overrides,
        Some(overlay()),
    );
}

#[test]
fn nested_locations_include_additions_claims_and_explicit_detach() {
    let mut inner = overlay();
    inner.components.push(unknown_intent("nested unknown"));
    inner.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("detached")),
        value: None,
    });
    let mut outer = overlay();
    let mut addition = PrefabAddition::new(id("nested"));
    addition.prefab = Some(instance(inner.clone()));
    outer.additions.push(addition);
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("claimed")),
        value: Some(instance(inner)),
    });
    let document = document(Some(instance(outer)));
    let mut session = scene(&document);
    let addition = PrefabInstancePathSegment::Addition { index: 0 };
    let claim = PrefabInstancePathSegment::InstanceClaim { index: 0 };
    let observed = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    assert_eq!(
        observed.nested_instances,
        vec![
            nested_path(&[addition]),
            nested_path(&[addition, claim]),
            nested_path(&[claim]),
            nested_path(&[claim, claim]),
        ]
    );
    for path in [
        nested_path(&[addition, claim]),
        nested_path(&[claim, claim]),
    ] {
        let observed = session.observe_prefab_instance(&path, limits()).unwrap();
        assert!(observed.instance.is_none());
        assert!(
            session
                .prepare_prefab_revert(session.revision(), &path, PrefabRevert::Overlay, limits(),)
                .is_err()
        );
    }
    for (path, is_addition) in [
        (nested_path(&[addition]), true),
        (nested_path(&[claim]), false),
    ] {
        let prepared = session
            .prepare_prefab_revert(session.revision(), &path, PrefabRevert::Overlay, limits())
            .unwrap();
        let mut expected = document.clone();
        let outer = expected.entities[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap();
        let selected = if is_addition {
            outer.additions[0].prefab.as_mut().unwrap()
        } else {
            outer.instances[0].value.as_mut().unwrap()
        };
        selected.overrides = None;
        assert_eq!(prepared.document(), &expected);
        assert!(session.commit_authoring_edit(prepared).applied);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &document);
    }
    let prepared = session
        .prepare_prefab_revert(
            session.revision(),
            &nested_path(&[addition]),
            PrefabRevert::Intent(PrefabOverlaySlot::InstanceClaim { index: 0 }),
            limits(),
        )
        .unwrap();
    assert!(
        instance_at(&prepared.document().entities[0].prefab, &[addition])
            .unwrap()
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .instances
            .is_empty()
    );
}

#[test]
fn intent_slots_remove_only_the_selected_array_entry_without_cascades() {
    let mut original = overlay();
    original.additions.push(PrefabAddition::new(id("added")));
    original.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("nested")),
        value: None,
    });
    original
        .components
        .push(unknown_intent("keep unless selected"));
    original.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    original
        .suppressions
        .push(PrefabEntityTarget::source(id("suppressed")));
    let document = document(Some(instance(original.clone())));
    let session = scene(&document);
    for slot in [
        PrefabOverlaySlot::Addition { index: 0 },
        PrefabOverlaySlot::InstanceClaim { index: 0 },
        PrefabOverlaySlot::Component { index: 0 },
        PrefabOverlaySlot::Placement { index: 0 },
        PrefabOverlaySlot::Suppression { index: 0 },
    ] {
        let mut expected = original.clone();
        match slot {
            PrefabOverlaySlot::Addition { .. } => expected.additions.clear(),
            PrefabOverlaySlot::InstanceClaim { .. } => expected.instances.clear(),
            PrefabOverlaySlot::Component { .. } => expected.components.clear(),
            PrefabOverlaySlot::Placement { .. } => expected.placements.clear(),
            PrefabOverlaySlot::Suppression { .. } => expected.suppressions.clear(),
            PrefabOverlaySlot::Property { .. } => unreachable!(),
        }
        let prepared = session
            .prepare_prefab_revert(
                session.revision(),
                &root_path(),
                PrefabRevert::Intent(slot),
                limits(),
            )
            .unwrap();
        assert_eq!(
            prepared.document().entities[0]
                .prefab
                .as_ref()
                .unwrap()
                .overrides,
            Some(expected),
        );
        assert_eq!(session.document(), &document);
    }
}

#[test]
fn failed_paths_slots_and_budgets_leave_the_source_allocation_untouched() {
    let mut original = overlay();
    original.components.push(unknown_intent(&"x".repeat(1_024)));
    let document = document(Some(instance(original)));
    let session = scene(&document);
    let source = session.shared_document();
    let revision = session.revision();
    let history = session.history_status();
    let tiny = PrefabDeclarationLimits::new(64, 32, 8).unwrap();
    assert_eq!(
        session.observe_prefab_instance(&root_path(), tiny),
        Err(PrefabDeclarationError::BudgetExceeded),
    );
    for (path, action, limits) in [
        (root_path(), PrefabRevert::Overlay, tiny),
        (
            nested_path(&[PrefabInstancePathSegment::Addition { index: usize::MAX }]),
            PrefabRevert::Overlay,
            limits(),
        ),
        (
            root_path(),
            PrefabRevert::Intent(PrefabOverlaySlot::Component { index: usize::MAX }),
            limits(),
        ),
        (
            root_path(),
            PrefabRevert::Intent(PrefabOverlaySlot::Property {
                component_index: 0,
                claim_index: 0,
            }),
            limits(),
        ),
    ] {
        assert!(
            session
                .prepare_prefab_revert(revision, &path, action, limits)
                .is_err()
        );
        assert_eq!(session.document(), &document);
        assert!(Arc::ptr_eq(&source, &session.document));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
    }
    let mut absent_document = document.clone();
    absent_document.entities[0].prefab = None;
    let absent = scene(&absent_document);
    assert_eq!(
        absent
            .observe_prefab_instance(&root_path(), limits())
            .unwrap()
            .instance,
        None,
    );
    assert!(
        absent
            .prepare_prefab_revert(
                absent.revision(),
                &root_path(),
                PrefabRevert::Overlay,
                limits(),
            )
            .is_err()
    );
}

#[test]
fn ordinary_history_refusal_after_lowering_preserves_the_original_source_and_manifest() {
    let mut original = overlay();
    original.components.push(unknown_intent(&"keep".repeat(64)));
    let document = document(Some(instance(original)));
    let history_limits = SceneAuthoringHistoryLimits::new(2, 4, 32, 64, 2, 16, 32).unwrap();
    let session = scene_with_history_limits(&document, history_limits);
    let source = session.shared_document();
    let revision = session.revision();
    let history = session.history_status();
    let content = session.content_state();
    let schema = session.authoring_schema_basis();
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let report = session
        .prepare_prefab_revert(revision, &root_path(), PrefabRevert::Overlay, limits())
        .unwrap_err();
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert_eq!(session.document(), &document);
    assert!(Arc::ptr_eq(&source, &session.document));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.authoring_schema_basis(), schema);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn observations_admit_aggregate_location_bytes_and_payload_before_copying() {
    let mut original = overlay();
    original.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("detached")),
        value: None,
    });
    let original = instance(original);
    let cost = prefab_instance_cost(&original, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
    let session = scene(&document(Some(original)));
    let bytes = cost.logical_bytes() + 2 * "anchor".len() + 9;
    let nodes = cost.nodes + 3;
    let exact = PrefabDeclarationLimits::new(nodes, bytes, 64).unwrap();
    assert!(session.observe_prefab_instance(&root_path(), exact).is_ok());
    assert_eq!(
        session.observe_prefab_instance(
            &root_path(),
            PrefabDeclarationLimits::new(nodes, bytes - 1, 64).unwrap(),
        ),
        Err(PrefabDeclarationError::BudgetExceeded),
    );
    assert_eq!(
        session.observe_prefab_instance(
            &root_path(),
            PrefabDeclarationLimits::new(nodes - 1, bytes, 64).unwrap(),
        ),
        Err(PrefabDeclarationError::BudgetExceeded),
    );
    assert!(PrefabDeclarationLimits::new(0, bytes, 64).is_none());
    assert!(PrefabDeclarationLimits::new(nodes, bytes, 65).is_none());
}

#[test]
fn revert_admits_both_complete_root_snapshots_even_for_a_nested_target() {
    let mut outer = overlay();
    outer.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("nested")),
        value: Some(instance(overlay())),
    });
    let original = instance(outer);
    let cost = prefab_instance_cost(&original, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
    let document = document(Some(original));
    let session = scene(&document);
    let path = nested_path(&[PrefabInstancePathSegment::InstanceClaim { index: 0 }]);
    let bytes = 2 * cost.logical_bytes() + "anchor".len() + 9;
    let nodes = 2 * cost.nodes + 2;
    let exact = PrefabDeclarationLimits::new(nodes, bytes, 64).unwrap();
    assert!(
        session
            .prepare_prefab_revert(session.revision(), &path, PrefabRevert::Overlay, exact,)
            .is_ok()
    );
    assert!(
        session
            .prepare_prefab_revert(
                session.revision(),
                &path,
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::new(nodes, bytes - 1, 64).unwrap(),
            )
            .is_err()
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn semantic_value_depth_is_bounded_by_the_existing_raw_cost_owner() {
    let mut value = ComponentValue::Null;
    for _ in 0..12 {
        value = ComponentValue::List(vec![value]);
    }
    let mut original = overlay();
    let mut intent = unknown_intent("payload");
    if let PrefabComponentIntent::Set { record, .. } = &mut intent {
        record.value = value;
    }
    original.components.push(intent);
    let document = document(Some(instance(original)));
    let session = scene(&document);
    let shallow = PrefabDeclarationLimits::new(4_096, 256 * 1024, 8).unwrap();
    assert_eq!(
        session.observe_prefab_instance(&root_path(), shallow),
        Err(PrefabDeclarationError::BudgetExceeded),
    );
    assert!(
        session
            .prepare_prefab_revert(
                session.revision(),
                &root_path(),
                PrefabRevert::Overlay,
                shallow,
            )
            .is_err()
    );
    assert_eq!(session.document(), &document);
}

#[test]
fn stale_revision_wrong_session_and_schema_change_reject_existing_tokens() {
    let document = document(Some(instance(overlay())));
    let mut session = scene(&document);
    let old_revision = session.revision();
    let prepared = session
        .prepare_prefab_revert(old_revision, &root_path(), PrefabRevert::Overlay, limits())
        .unwrap();
    let stale = session
        .prepare_prefab_revert(old_revision, &root_path(), PrefabRevert::Overlay, limits())
        .unwrap();
    let mut other = scene(&document);
    let wrong_owner = session
        .prepare_prefab_revert(old_revision, &root_path(), PrefabRevert::Overlay, limits())
        .unwrap();
    assert!(!other.commit_authoring_edit(wrong_owner).applied);
    assert_eq!(other.document(), &document);
    assert!(session.commit_authoring_edit(prepared).applied);
    let after = session.document().clone();
    assert!(!session.commit_authoring_edit(stale).applied);
    assert!(
        session
            .prepare_prefab_revert(old_revision, &root_path(), PrefabRevert::Overlay, limits(),)
            .is_err()
    );
    assert_eq!(session.document(), &after);

    let mut session = scene(&document);
    let revision = session.revision();
    let prepared = session
        .prepare_prefab_revert(revision, &root_path(), PrefabRevert::Overlay, limits())
        .unwrap();
    let old_schema = session.authoring_schema_basis().unwrap();
    session.authority = scene(&document).authority;
    assert_ne!(Some(old_schema), session.authoring_schema_basis());
    assert_eq!(session.revision(), revision);
    assert!(!session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &document);

    let mut session = prefab(&document);
    let mut other = prefab(&document);
    let wrong_owner = session
        .prepare_prefab_revert(
            session.revision(),
            &root_path(),
            PrefabRevert::Overlay,
            limits(),
        )
        .unwrap();
    assert!(!other.commit_authoring_edit(wrong_owner).applied);
    assert_eq!(other.document().entities, document.entities);
    assert_eq!(other.history_status().undo_depth, 0);
    let prepared = session
        .prepare_prefab_revert(
            session.revision(),
            &root_path(),
            PrefabRevert::Overlay,
            limits(),
        )
        .unwrap();
    session.authority = prefab(&document).authority;
    assert!(!session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document().entities, document.entities);
}

#[test]
fn unadmitted_sources_and_excessive_path_depth_are_explicit_refusals() {
    let document = document(Some(instance(overlay())));
    let native = SceneAuthoringSession::new(document.clone());
    assert_eq!(
        native.observe_prefab_instance(&root_path(), limits()),
        Err(PrefabDeclarationError::NotAdmitted),
    );
    let session = scene(&document);
    let path = nested_path(&vec![PrefabInstancePathSegment::Addition { index: 0 }; 65]);
    assert_eq!(
        session.observe_prefab_instance(&path, limits()),
        Err(PrefabDeclarationError::BudgetExceeded),
    );
    assert!(
        session
            .prepare_prefab_revert(session.revision(), &path, PrefabRevert::Overlay, limits(),)
            .is_err()
    );
}

#[test]
fn reverted_source_round_trips_keep_unknown_intent_and_reject_pre_reopen_tokens() {
    let mut original = overlay();
    original.components.push(properties());
    original
        .components
        .push(unknown_intent("keep across source reopen"));
    original.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("detached")),
        value: None,
    });
    let document = document(Some(instance(original)));
    let action = PrefabRevert::Intent(PrefabOverlaySlot::Property {
        component_index: 0,
        claim_index: 0,
    });
    let mut expected = document.clone();
    expected.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .components
        .remove(0);

    let mut session = scene(&document);
    let before = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    let prepared = session
        .prepare_prefab_revert(before.revision, &before.path, action, limits())
        .unwrap();
    assert!(session.commit_authoring_edit(prepared).applied);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
    let observation = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    let readmission = session.authoring_readmission().unwrap();
    let json = session.document().to_json_string().unwrap();
    let ron = session.document().to_ron_string().unwrap();
    for candidate in [
        SceneDocumentCandidate::decode_json_bytes(json.as_bytes()).unwrap(),
        SceneDocumentCandidate::decode_ron_bytes(ron.as_bytes()).unwrap(),
    ] {
        let retained = session
            .prepare_prefab_revert(
                session.revision(),
                &root_path(),
                PrefabRevert::Overlay,
                limits(),
            )
            .unwrap();
        let mut reopened = readmission.reopen_scene_candidate(candidate).unwrap();
        assert!(readmission.preserves_scene(&reopened));
        assert_eq!(reopened.document(), &expected);
        let observed = reopened
            .observe_prefab_instance(&root_path(), limits())
            .unwrap();
        assert_eq!(observed.instance, observation.instance);
        assert_eq!(observed.nested_instances, observation.nested_instances);
        assert_eq!(observed.schema, observation.schema);
        assert_ne!(observed.revision, observation.revision);
        assert!(!reopened.commit_authoring_edit(retained).applied);
        assert_eq!(reopened.document(), &expected);
        assert_eq!(reopened.history_status().undo_depth, 0);
    }

    let mut session = prefab(&document);
    let before = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    let prepared = session
        .prepare_prefab_revert(before.revision, &before.path, action, limits())
        .unwrap();
    assert!(session.commit_authoring_edit(prepared).applied);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document().entities, document.entities);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document().entities, expected.entities);
    let observation = session
        .observe_prefab_instance(&root_path(), limits())
        .unwrap();
    let readmission = session.authoring_readmission().unwrap();
    let json = session.document().to_json_string().unwrap();
    let ron = session.document().to_ron_string().unwrap();
    for candidate in [
        PrefabDocumentCandidate::decode_json_bytes(json.as_bytes()).unwrap(),
        PrefabDocumentCandidate::decode_ron_bytes(ron.as_bytes()).unwrap(),
    ] {
        let retained = session
            .prepare_prefab_revert(
                session.revision(),
                &root_path(),
                PrefabRevert::Overlay,
                limits(),
            )
            .unwrap();
        let mut reopened = readmission.reopen_prefab_candidate(candidate).unwrap();
        assert!(readmission.preserves_prefab(&reopened));
        assert_eq!(reopened.document().entities, expected.entities);
        let observed = reopened
            .observe_prefab_instance(&root_path(), limits())
            .unwrap();
        assert_eq!(observed.instance, observation.instance);
        assert_eq!(observed.nested_instances, observation.nested_instances);
        assert_eq!(observed.schema, observation.schema);
        assert_ne!(observed.revision, observation.revision);
        assert!(!reopened.commit_authoring_edit(retained).applied);
        assert_eq!(reopened.document().entities, expected.entities);
        assert_eq!(reopened.history_status().undo_depth, 0);
    }
}
