#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentPropertyKey,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, EntityReference, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, ConstructionPlacement,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, SceneAuthoringHistoryLimits,
    SceneAuthoringSession, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn open(document: &SceneDocument) -> SceneAuthoringSession {
    open_with_bundle(
        document,
        &AuthoringSchemaBundle::new([]).unwrap(),
        SceneAuthoringHistoryLimits::default(),
    )
}

fn open_with_bundle(
    document: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
    limits: SceneAuthoringHistoryLimits,
) -> SceneAuthoringSession {
    let adapters = adapters(bundle);
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/prepared.scene.json").unwrap()),
                bundle,
                &adapters,
            )
            .unwrap()
            .publish(),
        limits,
    )
}

fn adapters(bundle: &AuthoringSchemaBundle) -> Vec<AuthoringAdapterReceipt> {
    bundle
        .owners()
        .iter()
        .map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
        })
        .collect()
}

fn add(names: &[&str]) -> ScenePatchDocument {
    ScenePatchDocument::new(names.iter().map(|name| ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }))
}

#[test]
fn prepared_construction_is_unpublished_until_one_commit_and_one_undo() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let mut session = open(&document);
    let revision = session.revision();
    let content = session.content_state();
    let history = session.history_status();
    let prepared = session
        .prepare_authoring_edit(revision, &add(&["first", "second"]))
        .unwrap();
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.history_status(), history);
    assert_eq!(
        prepared.document().roots,
        [id("existing"), id("first"), id("second")]
    );
    assert_eq!(prepared.patch().operations.len(), 2);
    assert_eq!(prepared.inverse().operations.len(), 2);
    let expected = prepared.document().clone();
    let report = session.commit_authoring_edit(prepared);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(session.document(), &expected);
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert_eq!(session.content_state(), content);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
}

#[test]
fn dropping_a_prepared_edit_does_not_mutate_the_owner() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let session = open(&document);
    let revision = session.revision();
    let content = session.content_state();
    let history = session.history_status();
    drop(
        session
            .prepare_authoring_edit(revision, &add(&["discarded"]))
            .unwrap(),
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.history_status(), history);
}

#[test]
fn published_source_creation_owns_identity_without_widening_native_admission() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let mut scene = open(&document);
    let revision = scene.revision();
    let created = scene
        .create_authoring_entity(revision, ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let generated = created.entity().clone();
    assert_ne!(generated, id("existing"));
    assert_eq!(scene.document().roots, [id("existing"), generated.clone()]);
    assert_eq!(scene.history_status().undo_depth, 1);
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert!(scene.redo_authoring().applied);
    assert_eq!(scene.document().roots[1], generated);
    assert!(
        scene
            .create_authoring_entity(revision, ScenePlacement::last(ScenePlacementOwner::Root),)
            .is_err()
    );

    let mut registry = nara_reflect::ComponentRegistry::new();
    registry.freeze().unwrap();
    assert!(
        scene
            .create_entity_with_registry(
                scene.revision(),
                &registry,
                ScenePlacement::last(ScenePlacementOwner::Root),
                Default::default(),
            )
            .is_err()
    );
    let mut native = SceneAuthoringSession::new(SceneDocument::default());
    assert!(
        native
            .create_authoring_entity(
                native.revision(),
                ScenePlacement::last(ScenePlacementOwner::Root),
            )
            .is_err()
    );
}

#[test]
fn prepared_edits_reject_foreign_and_changed_owners_without_publication() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let mut first = open(&document);
    let mut other = open(&document);
    let foreign = first
        .prepare_authoring_edit(first.revision(), &add(&["foreign"]))
        .unwrap();
    let other_revision = other.revision();
    let rejected = other.commit_authoring_edit(foreign);
    assert!(!rejected.applied);
    assert_eq!(other.document(), &document);
    assert_eq!(other.revision(), other_revision);
    assert_eq!(other.history_status().undo_depth, 0);

    let stale = first
        .prepare_authoring_edit(first.revision(), &add(&["stale"]))
        .unwrap();
    assert!(first.apply_authoring_patch(&add(&["accepted"])).applied);
    let accepted = first.document().clone();
    let accepted_revision = first.revision();
    let accepted_history = first.history_status();
    assert!(!first.commit_authoring_edit(stale).applied);
    assert_eq!(first.document(), &accepted);
    assert_eq!(first.revision(), accepted_revision);
    assert_eq!(first.history_status(), accepted_history);
}

#[test]
fn the_last_invalid_construction_operation_publishes_nothing() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let session = open(&document);
    let revision = session.revision();
    let history = session.history_status();
    assert!(
        session
            .prepare_authoring_edit(revision, &add(&["first", "existing"]))
            .is_err()
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
}

fn link_bundle() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.links").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("game.Link"),
                    "Link",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("target"),
                        "Target",
                        ComponentFieldPath::from_fields(["target"]),
                        ComponentValueKind::EntityRef,
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::Inspect,
                        ComponentCapability::Edit,
                        ComponentCapability::EntityRef,
                    ]),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("amount"),
                        "Amount",
                        ComponentFieldPath::from_fields(["amount"]),
                        ComponentValueKind::U64,
                    )
                    .with_numeric_domain(Some(ComponentNumericDomain::U8))
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ])
                .with_authoring_initial_value(ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("existing"),
                        }),
                    ),
                    ("amount", ComponentValue::U64(1)),
                ])),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn linked_construction(
    template: nara_scene::SceneComponentRecord,
    amount: u64,
) -> ScenePatchDocument {
    let component = ComponentTypeId::new("game.Link");
    let mut patch = add(&["first", "second"]);
    for entity in ["first", "second"] {
        patch.operations.push(ScenePatchOperation::AddComponent {
            entity: id(entity),
            component: component.clone(),
            value: template.clone(),
        });
    }
    for (entity, target) in [("first", "second"), ("second", "first")] {
        patch.operations.push(ScenePatchOperation::SetField {
            entity: id(entity),
            component: component.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("target"),
            value: ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id(target),
            }),
        });
    }
    patch.operations.push(ScenePatchOperation::SetField {
        entity: id("second"),
        component,
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("amount"),
        value: ComponentValue::U64(amount),
    });
    patch
}

fn root_property(field: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(field))
}

fn construction_bundle() -> AuthoringSchemaBundle {
    let source = link_bundle();
    let mut catalog = source.owners()[0].current().clone();
    catalog.components.push(
        ComponentSchema::new(
            ComponentTypeId::new("game.Inventory"),
            "Inventory",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("items"),
            "Items",
            ComponentFieldPath::from_fields(["items"]),
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("wire_key", ComponentValueKind::String)
                        .with_id(ComponentFieldId::new("id")),
                    ComponentShapeMember::new("wire_target", ComponentValueKind::EntityRef)
                        .with_id(ComponentFieldId::new("target")),
                ]),
                ComponentFieldId::new("id"),
            ),
        )
        .with_capabilities([
            ComponentCapability::Scene,
            ComponentCapability::Inspect,
            ComponentCapability::Edit,
            ComponentCapability::EntityRef,
        ])])
        .with_authoring_initial_value(ComponentValue::map([(
            "items",
            ComponentValue::List(vec![ComponentValue::map([
                ("wire_key", ComponentValue::String("a".into())),
                (
                    "wire_target",
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: id("existing"),
                    }),
                ),
            ])]),
        )])),
    );
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.links").unwrap(),
        catalog,
        None,
    )])
    .unwrap()
}

fn inventory_property(key: &str, member: &str) -> ComponentPropertyPath {
    root_property("items").with_steps([
        ComponentPropertyStep::ListItem {
            by: ComponentFieldId::new("id"),
            key: ComponentPropertyKey::String(key.into()),
        },
        ComponentPropertyStep::Member(ComponentFieldId::new(member)),
    ])
}

#[test]
fn retained_construction_shares_existing_and_staged_keyed_properties_in_both_source_kinds() {
    let bundle = construction_bundle();
    let inventory = ComponentTypeId::new("game.Inventory");
    let link = ComponentTypeId::new("game.Link");
    let seed = open_with_bundle(
        &SceneDocument::default(),
        &bundle,
        SceneAuthoringHistoryLimits::default(),
    );
    let template = seed.authoring_initial_component(&inventory).unwrap();
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))
        .with_component(inventory.clone(), template)
        .with_component(
            ComponentTypeId::new("game.Unknown"),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion(9),
                ComponentValue::String("keep exactly".into()),
            ),
        )]);
    let scene = open_with_bundle(&original, &bundle, SceneAuthoringHistoryLimits::default());
    let prefab = PrefabDocument::new(original.roots.clone(), original.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/construction.prefab.json").unwrap(),
                ),
                &bundle,
                &adapters(&bundle),
            )
            .unwrap()
            .publish();
    let prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    macro_rules! qualify {
        ($owner:expr) => {{
            let mut session = $owner;
            let before = session.document().clone();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            let mut invalid = session
                .begin_authoring_construction(session.revision())
                .unwrap();
            let local = invalid
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .unwrap();
            invalid.add_component(&local, inventory.clone()).unwrap();
            invalid
                .set_property(
                    &local,
                    inventory.clone(),
                    inventory_property("a", "id"),
                    ComponentValue::String("b".into()),
                )
                .unwrap();
            invalid
                .set_entity_reference(
                    &local,
                    inventory.clone(),
                    inventory_property("a", "target"),
                    &local,
                )
                .unwrap();
            let rejected = session
                .prepare_authoring_construction(&invalid)
                .unwrap_err();
            assert_eq!(
                rejected.diagnostics.iter().next().unwrap().code().as_str(),
                "scene.authoring-property-edit-rejected"
            );
            assert_eq!(session.document(), &before);
            assert_eq!(session.history_status().undo_depth, 0);

            let mut draft = session
                .begin_authoring_construction(session.revision())
                .unwrap();
            let first = draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .unwrap();
            let second = draft
                .create_entity(ConstructionPlacement::ChildOf((&first).into()))
                .unwrap();
            for entity in [&first, &second] {
                draft.add_component(entity, link.clone()).unwrap();
                draft.add_component(entity, inventory.clone()).unwrap();
            }
            draft
                .set_entity_reference(&first, link.clone(), root_property("target"), &second)
                .unwrap();
            draft
                .set_entity_reference(&second, link.clone(), root_property("target"), &first)
                .unwrap();
            draft
                .set_entity_reference(
                    &first,
                    inventory.clone(),
                    inventory_property("a", "target"),
                    &first,
                )
                .unwrap();
            draft
                .set_entity_reference(
                    &second,
                    inventory.clone(),
                    inventory_property("a", "target"),
                    &first,
                )
                .unwrap();
            draft
                .set_property(
                    id("existing"),
                    inventory.clone(),
                    inventory_property("a", "id"),
                    ComponentValue::String("b".into()),
                )
                .unwrap();
            draft
                .set_entity_reference(
                    id("existing"),
                    inventory.clone(),
                    inventory_property("b", "target"),
                    &second,
                )
                .unwrap();
            let prepared = session.prepare_authoring_construction(&draft).unwrap();
            let first_id = prepared.entity(&first).unwrap().clone();
            let second_id = prepared.entity(&second).unwrap().clone();
            assert_eq!(session.document(), &before);
            assert_eq!(
                prepared
                    .patch()
                    .operations
                    .iter()
                    .filter(|operation| matches!(
                        operation,
                        ScenePatchOperation::AddComponent { .. }
                    ))
                    .count(),
                4
            );
            for operation in &prepared.patch().operations {
                if let ScenePatchOperation::AddComponent {
                    component, value, ..
                } = operation
                {
                    assert_eq!(
                        value,
                        &session.authoring_initial_component(component).unwrap()
                    );
                }
            }
            let expected = prepared.document().clone();
            let result = session.commit_authoring_construction(prepared).unwrap();
            assert_eq!(result.entity(&first), Some(&first_id));
            assert_eq!(result.entity(&second), Some(&second_id));
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert_eq!(session.history_status().undo_depth, 1);
            for (entity, target) in [
                (&first_id, &first_id),
                (&second_id, &first_id),
                (&id("existing"), &second_id),
            ] {
                let record = session
                    .document()
                    .entities
                    .iter()
                    .find(|record| &record.id == entity)
                    .unwrap();
                let value = &record.components[&inventory].value;
                let rows = value.field("items").unwrap();
                let ComponentValue::List(rows) = rows else {
                    panic!("inventory remains a keyed list")
                };
                assert_eq!(
                    rows[0].field("wire_target").unwrap(),
                    &ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: target.clone()
                    })
                );
            }
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &before);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &expected);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        }};
    }
    qualify!(scene);
    qualify!(prefab);
}

#[test]
fn retained_construction_resolves_local_handles_once_and_commits_one_history_entry() {
    let bundle = link_bundle();
    let component = ComponentTypeId::new("game.Link");
    let empty = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let mut session = open_with_bundle(&empty, &bundle, SceneAuthoringHistoryLimits::default());
    let revision = session.revision();
    let template = session.authoring_initial_component(&component).unwrap();
    let mut draft = session.begin_authoring_construction(revision).unwrap();
    let first = draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let second = draft
        .create_entity(ConstructionPlacement::ChildOf((&first).into()))
        .unwrap();
    for entity in [&first, &second] {
        draft.add_component(entity, component.clone()).unwrap();
    }
    draft
        .set_entity_reference(&first, component.clone(), root_property("target"), &second)
        .unwrap();
    draft
        .set_entity_reference(&second, component.clone(), root_property("target"), &first)
        .unwrap();
    draft
        .set_property(
            &second,
            component.clone(),
            root_property("amount"),
            ComponentValue::U64(7),
        )
        .unwrap();
    let prepared = session.prepare_authoring_construction(&draft).unwrap();
    let first_id = prepared.entity(&first).unwrap().clone();
    let second_id = prepared.entity(&second).unwrap().clone();
    assert_ne!(first_id, second_id);
    assert_ne!(first_id, id("existing"));
    assert_eq!(
        prepared.document().roots,
        [id("existing"), first_id.clone()]
    );
    assert_eq!(
        prepared
            .document()
            .entities
            .iter()
            .find(|record| record.id == first_id)
            .unwrap()
            .children,
        [second_id.clone()]
    );
    for operation in &prepared.patch().operations {
        if let ScenePatchOperation::AddComponent { value, .. } = operation {
            assert_eq!(
                value, &template,
                "construction must keep its exact creation template in the patch"
            );
        }
    }
    assert_eq!(session.document(), &empty);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    let expected = prepared.document().clone();
    let committed = session.commit_authoring_construction(prepared).unwrap();
    assert!(committed.report().applied);
    assert_eq!(committed.entity(&first), Some(&first_id));
    assert_eq!(committed.entity(&second), Some(&second_id));
    assert_eq!(session.document(), &expected);
    assert_eq!(session.history_status().undo_depth, 1);
    let values = &session
        .document()
        .entities
        .iter()
        .find(|record| record.id == second_id)
        .unwrap()
        .components[&component]
        .value;
    assert_eq!(
        values.field("target").unwrap(),
        &ComponentValue::EntityReference(EntityReference::SceneLocal { entity: first_id })
    );
    assert_eq!(values.field("amount").unwrap(), &ComponentValue::U64(7));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &empty);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
}

#[test]
fn retained_construction_rejects_foreign_handles_and_stale_commits_without_poisoning_append() {
    let bundle = link_bundle();
    let empty = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let mut session = open_with_bundle(&empty, &bundle, SceneAuthoringHistoryLimits::default());
    let mut first = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    let mut second = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    let local_first = first
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let local_second = second
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    assert_ne!(local_first, local_second);
    assert!(
        second
            .add_component(&local_first, ComponentTypeId::new("game.Link"))
            .is_err()
    );
    second
        .add_component(&local_second, ComponentTypeId::new("game.Link"))
        .unwrap();
    assert!(
        second
            .set_entity_reference(
                &local_second,
                ComponentTypeId::new("game.Link"),
                root_property("target"),
                &local_first
            )
            .is_err()
    );
    second
        .set_entity_reference(
            &local_second,
            ComponentTypeId::new("game.Link"),
            root_property("target"),
            &local_second,
        )
        .unwrap();
    let old = session.prepare_authoring_construction(&first).unwrap();
    let accepted = session.prepare_authoring_construction(&second).unwrap();
    let mut other = open_with_bundle(&empty, &bundle, SceneAuthoringHistoryLimits::default());
    assert!(other.prepare_authoring_construction(&second).is_err());
    assert!(other.commit_authoring_construction(old).is_err());
    assert_eq!(other.document(), &empty);
    let old = session.prepare_authoring_construction(&first).unwrap();
    let committed = session.commit_authoring_construction(accepted).unwrap();
    assert!(committed.entity(&local_first).is_none());
    assert!(committed.entity(&local_second).is_some());
    let expected = session.document().clone();
    let history = session.history_status();
    assert!(session.commit_authoring_construction(old).is_err());
    assert!(session.prepare_authoring_construction(&second).is_err());
    assert_eq!(session.document(), &expected);
    assert_eq!(session.history_status(), history);
}

#[test]
fn retained_construction_resolves_existing_and_local_sibling_owners_in_declaration_order() {
    let existing = id("existing");
    let child = id("child");
    let mut root = SceneEntityRecord::new(existing.clone());
    root.children.push(child.clone());
    let scene = SceneDocument::new(
        vec![existing.clone()],
        vec![root, SceneEntityRecord::new(child.clone())],
    );
    let session = open(&scene);
    let prefab = PrefabDocument::new(scene.roots.clone(), scene.entities.clone());
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/placement.prefab.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    let prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    macro_rules! qualify {
        ($session:expr) => {{
            let session = $session;
            let before = session.document().clone();
            let mut draft = session
                .begin_authoring_construction(session.revision())
                .unwrap();
            let before_root = draft
                .create_entity(ConstructionPlacement::Before((&existing).into()))
                .unwrap();
            let after_root = draft
                .create_entity(ConstructionPlacement::After((&existing).into()))
                .unwrap();
            let before_child = draft
                .create_entity(ConstructionPlacement::Before((&child).into()))
                .unwrap();
            let after_child = draft
                .create_entity(ConstructionPlacement::After((&child).into()))
                .unwrap();
            let before_local = draft
                .create_entity(ConstructionPlacement::Before((&before_child).into()))
                .unwrap();
            let after_local = draft
                .create_entity(ConstructionPlacement::After((&after_child).into()))
                .unwrap();
            let prepared = session.prepare_authoring_construction(&draft).unwrap();
            assert_eq!(
                prepared.document().roots,
                [
                    prepared.entity(&before_root).unwrap().clone(),
                    existing.clone(),
                    prepared.entity(&after_root).unwrap().clone()
                ]
            );
            let parent = prepared
                .document()
                .entities
                .iter()
                .find(|record| record.id == existing)
                .unwrap();
            assert_eq!(
                parent.children,
                [
                    prepared.entity(&before_local).unwrap().clone(),
                    prepared.entity(&before_child).unwrap().clone(),
                    child.clone(),
                    prepared.entity(&after_child).unwrap().clone(),
                    prepared.entity(&after_local).unwrap().clone(),
                ]
            );
            assert_eq!(session.document(), &before);
            assert_eq!(session.history_status().undo_depth, 0);
            draft
                .create_entity(ConstructionPlacement::After(id("missing-sibling").into()))
                .unwrap();
            assert!(session.prepare_authoring_construction(&draft).is_err());
            assert_eq!(session.document(), &before);
        }};
    }
    qualify!(session);
    qualify!(prefab);
}

#[test]
fn retained_construction_last_invalid_write_and_raw_write_limit_preserve_source() {
    let bundle = link_bundle();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let session = open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
    let mut draft = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    let local = draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let component = ComponentTypeId::new("game.Link");
    draft.add_component(&local, component.clone()).unwrap();
    for amount in 0..7 {
        draft
            .set_property(
                &local,
                component.clone(),
                root_property("amount"),
                ComponentValue::U64(amount),
            )
            .unwrap();
    }
    draft
        .set_property(
            &local,
            component.clone(),
            root_property("amount"),
            ComponentValue::U64(256),
        )
        .unwrap();
    assert!(
        draft
            .set_property(
                &local,
                component,
                root_property("amount"),
                ComponentValue::U64(1)
            )
            .is_err()
    );
    assert!(session.prepare_authoring_construction(&draft).is_err());
    assert_eq!(session.document(), &document);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn retained_construction_admits_aggregate_template_copy_before_materialization() {
    let component = ComponentTypeId::new("game.Note");
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.notes").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Note", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("text"),
                        "Text",
                        ComponentFieldPath::empty(),
                        ComponentValueKind::String,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                    .with_authoring_initial_value(ComponentValue::String("x".repeat(60_000))),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let limits =
        SceneAuthoringHistoryLimits::new(8, 256, 65_536, 200_000, 256, 65_536, 100_000).unwrap();
    let session = open_with_bundle(&document, &bundle, limits);
    let revision = session.revision();
    let mut draft = session.begin_authoring_construction(revision).unwrap();
    for _ in 0..2 {
        let entity = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        draft.add_component(&entity, component.clone()).unwrap();
    }
    let rejected = session.prepare_authoring_construction(&draft).unwrap_err();
    assert_eq!(
        rejected.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.authoring-construction-template-copy-limit"
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn retained_construction_does_not_promote_unavailable_records_through_staged_templates() {
    let bundle = link_bundle();
    let component = ComponentTypeId::new("game.Link");
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("existing")).with_component(
            component.clone(),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion(99),
                ComponentValue::String("future data".into()),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new("game.links").unwrap()),
        ),
        SceneEntityRecord::new(id("missing")),
    ]);
    let mut session = open_with_bundle(&original, &bundle, SceneAuthoringHistoryLimits::default());
    let revision = session.revision();
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let mut draft = session.begin_authoring_construction(revision).unwrap();
    draft
        .add_component(id("existing"), component.clone())
        .unwrap();
    draft
        .set_property(
            id("existing"),
            component.clone(),
            root_property("amount"),
            ComponentValue::U64(3),
        )
        .unwrap();
    let rejected = session.prepare_authoring_construction(&draft).unwrap_err();
    assert_eq!(
        rejected.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.authoring-property-edit-rejected"
    );
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(session.history_status().undo_depth, 0);

    let mut draft = session.begin_authoring_construction(revision).unwrap();
    draft
        .add_component(id("missing"), component.clone())
        .unwrap();
    draft
        .set_property(
            id("missing"),
            component.clone(),
            root_property("amount"),
            ComponentValue::U64(3),
        )
        .unwrap();
    let prepared = session.prepare_authoring_construction(&draft).unwrap();
    session.commit_authoring_construction(prepared).unwrap();
    assert_eq!(session.document().entities[0], original.entities[0]);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[1].components[&component]
            .value
            .field("amount")
            .unwrap(),
        &ComponentValue::U64(3)
    );
}

#[test]
fn retained_construction_charges_resolved_local_bytes_before_overwritten_roots_coalesce() {
    let component = ComponentTypeId::new("game.Note");
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.notes").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Note", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([
                        ComponentFieldSchema::required(
                            ComponentFieldId::new("text"),
                            "Text",
                            ComponentFieldPath::from_fields(["text"]),
                            ComponentValueKind::String,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                        ComponentFieldSchema::required(
                            ComponentFieldId::new("target"),
                            "Target",
                            ComponentFieldPath::from_fields(["target"]),
                            ComponentValueKind::EntityRef,
                        )
                        .with_capabilities([
                            ComponentCapability::Scene,
                            ComponentCapability::Inspect,
                            ComponentCapability::Edit,
                            ComponentCapability::EntityRef,
                        ]),
                    ])
                    .with_authoring_initial_value(ComponentValue::map([
                        ("text", ComponentValue::String(String::new())),
                        (
                            "target",
                            ComponentValue::EntityReference(EntityReference::SceneLocal {
                                entity: id("existing"),
                            }),
                        ),
                    ])),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let session = open_with_bundle(&original, &bundle, SceneAuthoringHistoryLimits::default());
    // Each write retains its target ID (22 bytes) and component ID before root coalescing.
    // Three selectors cost 14 bytes; the resolved SceneLocal replacement costs another 22.
    let target_bytes = 3 * (22 + component.as_str().len());
    for (bytes, accepted) in [
        (65_536 - 14 - target_bytes, false),
        (65_536 - 14 - target_bytes - 22, true),
    ] {
        let mut draft = session
            .begin_authoring_construction(session.revision())
            .unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        draft.add_component(&local, component.clone()).unwrap();
        draft
            .set_property(
                &local,
                component.clone(),
                root_property("text"),
                ComponentValue::String("x".repeat(bytes)),
            )
            .unwrap();
        draft
            .set_entity_reference(&local, component.clone(), root_property("target"), &local)
            .unwrap();
        draft
            .set_property(
                &local,
                component.clone(),
                root_property("text"),
                ComponentValue::String(String::new()),
            )
            .unwrap();
        let result = session.prepare_authoring_construction(&draft);
        if accepted {
            let prepared = result.unwrap();
            assert_eq!(prepared.entity(&local).unwrap().as_str().len(), 22);
            let created = prepared
                .document()
                .entities
                .iter()
                .find(|record| Some(&record.id) == prepared.entity(&local))
                .unwrap();
            assert_eq!(
                created.components[&component].value.field("text").unwrap(),
                &ComponentValue::String(String::new())
            );
        } else {
            assert_eq!(
                result
                    .unwrap_err()
                    .diagnostics
                    .iter()
                    .next()
                    .unwrap()
                    .code()
                    .as_str(),
                "scene.authoring-property-edit-rejected"
            );
        }
        assert_eq!(session.document(), &original);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn mixed_construction_uses_final_validation_and_retains_unknown_records_in_both_kinds() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))
        .with_component(
            ComponentTypeId::new("game.Unknown"),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve exactly".into()),
            ),
        )]);
    let bundle = link_bundle();
    let mut session = open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let template = session
        .authoring_initial_component(&ComponentTypeId::new("game.Link"))
        .unwrap();
    let revision = session.revision();
    assert!(
        session
            .prepare_authoring_edit(revision, &linked_construction(template.clone(), 256))
            .is_err()
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    let prepared = session
        .prepare_authoring_edit(revision, &linked_construction(template.clone(), 7))
        .unwrap();
    let accepted = prepared.document().clone();
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
    let reopened = session
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_json_bytes(
                accepted.to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(reopened.document(), &accepted);

    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/prepared.prefab.json").unwrap(),
                ),
                &bundle,
                &adapters(&bundle),
            )
            .unwrap()
            .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let revision = session.revision();
    assert!(
        session
            .prepare_authoring_edit(revision, &linked_construction(template.clone(), 256))
            .is_err()
    );
    let prepared = session
        .prepare_authoring_edit(revision, &linked_construction(template, 7))
        .unwrap();
    let accepted = prepared.document().clone();
    assert_eq!(session.document(), &prefab);
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &accepted);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
}

#[test]
fn history_refusal_and_reopen_reject_before_publication() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let limits = SceneAuthoringHistoryLimits::new(8, 64, 128, 4096, 64, 128, 1).unwrap();
    let session = open_with_bundle(&document, &AuthoringSchemaBundle::new([]).unwrap(), limits);
    let revision = session.revision();
    assert!(
        session
            .prepare_authoring_edit(revision, &add(&["oversized-inverse"]))
            .is_err()
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.history_status().undo_depth, 0);
    assert_eq!(session.revision(), revision);

    let mut draft = session.begin_authoring_construction(revision).unwrap();
    draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    assert!(session.prepare_authoring_construction(&draft).is_err());
    assert_eq!(session.document(), &document);
    assert_eq!(session.history_status().undo_depth, 0);
    assert_eq!(session.revision(), revision);

    let session = open(&document);
    let prepared = session
        .prepare_authoring_edit(session.revision(), &add(&["old-incarnation"]))
        .unwrap();
    let mut draft = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    let old_construction = session.prepare_authoring_construction(&draft).unwrap();
    let mut reopened = session
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        session.authoring_schema_basis(),
        reopened.authoring_schema_basis()
    );
    assert_ne!(
        session.revision().source_id(),
        reopened.revision().source_id()
    );
    assert!(!reopened.commit_authoring_edit(prepared).applied);
    assert!(reopened.prepare_authoring_construction(&draft).is_err());
    assert!(
        reopened
            .commit_authoring_construction(old_construction)
            .is_err()
    );
    assert_eq!(reopened.document(), &document);
    assert_eq!(reopened.history_status().undo_depth, 0);
}

#[test]
fn mixed_operation_budget_is_independent_of_the_field_write_budget() {
    let session = open(&SceneDocument::default());
    let operations = (0..=nara_scene::MAX_AUTHORING_EDIT_OPERATIONS)
        .map(|index| ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id(&format!("entity-{index}"))),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        })
        .collect::<Vec<_>>();
    let accepted = ScenePatchDocument::new(
        operations[..nara_scene::MAX_AUTHORING_EDIT_OPERATIONS]
            .iter()
            .cloned(),
    );
    let prepared = session
        .prepare_authoring_edit(session.revision(), &accepted)
        .unwrap();
    assert_eq!(
        prepared.document().entities.len(),
        nara_scene::MAX_AUTHORING_EDIT_OPERATIONS
    );
    drop(prepared);
    assert!(
        session
            .prepare_authoring_edit(session.revision(), &ScenePatchDocument::new(operations))
            .is_err()
    );
    assert!(session.document().entities.is_empty());
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn retained_construction_bounds_declarations_and_coalesced_property_roots_together() {
    let bundle = link_bundle();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let session = open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
    for (count, accepted) in [(254, true), (255, false)] {
        let mut draft = session
            .begin_authoring_construction(session.revision())
            .unwrap();
        draft
            .add_component(id("existing"), ComponentTypeId::new("game.Link"))
            .unwrap();
        for _ in 0..count {
            draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .unwrap();
        }
        if !accepted {
            assert!(
                draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .is_err()
            );
            drop(session.prepare_authoring_construction(&draft).unwrap());
        }
        for amount in 0..if accepted { 8 } else { 1 } {
            draft
                .set_property(
                    id("existing"),
                    ComponentTypeId::new("game.Link"),
                    root_property("amount"),
                    ComponentValue::U64(amount),
                )
                .unwrap();
        }
        let result = session.prepare_authoring_construction(&draft);
        if accepted {
            let prepared = result.unwrap();
            assert_eq!(prepared.patch().operations.len(), 256);
            drop(prepared);
        } else {
            assert!(result.is_err());
            assert!(
                draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .is_err()
            );
        }
        assert_eq!(session.document(), &document);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn mixed_edits_reject_an_unreplayable_field_inverse_before_either_kind_publishes() {
    let component = ComponentTypeId::new("game.Note");
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.notes").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Note", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("text"),
                        "Text",
                        ComponentFieldPath::from_fields(["text"]),
                        ComponentValueKind::String,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))
        .with_component(
            component.clone(),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("text", ComponentValue::String("x".repeat(70_000)))]),
            ),
        )]);
    let mut patch = add(&["extra"]);
    patch.operations.push(ScenePatchOperation::SetField {
        entity: id("existing"),
        component,
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("text"),
        value: ComponentValue::String("short".into()),
    });
    let session = open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
    let revision = session.revision();
    let scene_rejected = session.prepare_authoring_edit(revision, &patch).is_err();
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/large-note.prefab.json").unwrap(),
                ),
                &bundle,
                &adapters(&bundle),
            )
            .unwrap()
            .publish();
    let session = PrefabAuthoringSession::from_published_authoring_document(published);
    let revision = session.revision();
    let prefab_rejected = session.prepare_authoring_edit(revision, &patch).is_err();
    assert_eq!(session.document(), &prefab);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    assert!(
        scene_rejected && prefab_rejected,
        "Scene refused={scene_rejected}; Prefab refused={prefab_rejected}"
    );
}

#[test]
fn invalid_format_and_field_count_reject_before_history_payload_accounting() {
    let session = open(&SceneDocument::default());
    for invalid_format in [true, false] {
        let mut patch =
            ScenePatchDocument::new((0..9).map(|index| ScenePatchOperation::SetField {
                entity: id("absent"),
                component: ComponentTypeId::new("game.Note"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new(format!("field-{index}")),
                value: if index == 0 {
                    ComponentValue::String("x".repeat(17 * 1024 * 1024))
                } else {
                    ComponentValue::Null
                },
            }));
        if invalid_format {
            patch.format_version = u32::MAX;
        }
        let rejection = session
            .prepare_authoring_edit(session.revision(), &patch)
            .unwrap_err();
        assert_eq!(
            rejection.diagnostics.iter().next().unwrap().code().as_str(),
            "scene.authoring-field-patch-unavailable"
        );
        assert!(session.document().entities.is_empty());
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn mixed_removal_and_forward_reference_transactions_replay_exactly_for_both_kinds() {
    let bundle = link_bundle();
    let component = ComponentTypeId::new("game.Link");
    let record = |target: &str| {
        SceneEntityRecord::new(id("existing")).with_component(
            component.clone(),
            nara_scene::SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id(target),
                        }),
                    ),
                    ("amount", ComponentValue::U64(1)),
                ]),
            ),
        )
    };
    let field = |name: &str, value| ScenePatchOperation::SetField {
        entity: id("existing"),
        component: component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(name),
        value,
    };
    let document = SceneDocument::from_ordered_roots([record("existing")]);
    for removal in [
        ScenePatchOperation::RemoveComponent {
            entity: id("existing"),
            component: component.clone(),
        },
        ScenePatchOperation::RemoveEntity {
            entity: id("existing"),
            from: ScenePlacement::current(&document, &id("existing")).unwrap(),
        },
    ] {
        assert_both_kind_roundtrip(
            &document,
            &bundle,
            &ScenePatchDocument::new([field("amount", ComponentValue::U64(2)), removal]),
        );
    }
    assert_both_kind_roundtrip(
        &document,
        &bundle,
        &ScenePatchDocument::new([
            field(
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("later"),
                }),
            ),
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(id("later")),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        ]),
    );
    let document =
        SceneDocument::from_ordered_roots([record("victim"), SceneEntityRecord::new(id("victim"))]);
    assert_both_kind_roundtrip(
        &document,
        &bundle,
        &ScenePatchDocument::new([
            ScenePatchOperation::RemoveEntity {
                entity: id("victim"),
                from: ScenePlacement::current(&document, &id("victim")).unwrap(),
            },
            field(
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("existing"),
                }),
            ),
        ]),
    );
}

fn assert_both_kind_roundtrip(
    document: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
    patch: &ScenePatchDocument,
) {
    let mut scene = open_with_bundle(document, bundle, SceneAuthoringHistoryLimits::default());
    let prepared = scene
        .prepare_authoring_edit(scene.revision(), patch)
        .unwrap();
    let expected = prepared.document().clone();
    assert!(scene.commit_authoring_edit(prepared).applied);
    assert_eq!(scene.document(), &expected);
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), document);
    assert!(scene.redo_authoring().applied);
    assert_eq!(scene.document(), &expected);

    let document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/mixed.prefab.json").unwrap()),
                bundle,
                &adapters(bundle),
            )
            .unwrap()
            .publish();
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    let prepared = prefab
        .prepare_authoring_edit(prefab.revision(), patch)
        .unwrap();
    let expected = prepared.document().clone();
    assert!(prefab.commit_authoring_edit(prepared).applied);
    assert_eq!(prefab.document(), &expected);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(prefab.document(), &document);
    assert!(prefab.redo_authoring().applied);
    assert_eq!(prefab.document(), &expected);
}

#[test]
#[ignore = "decision-local prepare/commit observation; run this exact test alone"]
fn explicit_construction_prepare_and_commit_cost_observation() {
    use std::{hint::black_box, time::Instant};

    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let bundle = link_bundle();
    let mut prepare_ns = Vec::new();
    let mut commit_ns = Vec::new();
    for iteration in 0..72 {
        let mut session =
            open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
        let template = session
            .authoring_initial_component(&ComponentTypeId::new("game.Link"))
            .unwrap();
        let patch = linked_construction(template, 7);
        let start = Instant::now();
        let prepared = session
            .prepare_authoring_edit(session.revision(), black_box(&patch))
            .unwrap();
        let prepare = start.elapsed().as_nanos();
        let start = Instant::now();
        let report = session.commit_authoring_edit(black_box(prepared));
        let commit = start.elapsed().as_nanos();
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_eq!(session.document().entities.len(), 3);
        if iteration >= 8 {
            prepare_ns.push(prepare);
            commit_ns.push(commit);
        }
    }
    prepare_ns.sort_unstable();
    commit_ns.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "measurement": "explicit-construction-prepare-commit-v1",
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
            "warmups": 8,
            "samples": prepare_ns.len(),
            "entities_before": 1,
            "entities_created": 2,
            "components_per_created_entity": 1,
            "field_writes": 3,
            "prepare_median_ns": prepare_ns[32],
            "prepare_p95_ns": prepare_ns[60],
            "commit_median_ns": commit_ns[32],
            "commit_p95_ns": commit_ns[60],
            "prepare_samples_ns": prepare_ns,
            "commit_samples_ns": commit_ns,
            "scope": "owner preparation and consuming commit; excludes session admission, caller patch construction, report disposal, native projection, Save and rendering"
        })
    );
}

#[test]
#[ignore = "decision-local retained/control comparison; run alone"]
fn retained_construction_prepare_and_commit_cost_comparison() {
    use std::{hint::black_box, time::Instant};

    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("existing"))]);
    let bundle = link_bundle();
    let component = ComponentTypeId::new("game.Link");
    for retained in [false, true] {
        let mut prepare_ns = Vec::new();
        let mut commit_ns = Vec::new();
        for iteration in 0..72 {
            let mut session =
                open_with_bundle(&document, &bundle, SceneAuthoringHistoryLimits::default());
            let (prepare, commit) = if retained {
                let mut draft = session
                    .begin_authoring_construction(session.revision())
                    .unwrap();
                let first = draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .unwrap();
                let second = draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .unwrap();
                for local in [&first, &second] {
                    draft.add_component(local, component.clone()).unwrap();
                }
                draft
                    .set_entity_reference(
                        &first,
                        component.clone(),
                        root_property("target"),
                        &second,
                    )
                    .unwrap();
                draft
                    .set_entity_reference(
                        &second,
                        component.clone(),
                        root_property("target"),
                        &first,
                    )
                    .unwrap();
                draft
                    .set_property(
                        &second,
                        component.clone(),
                        root_property("amount"),
                        ComponentValue::U64(7),
                    )
                    .unwrap();
                let start = Instant::now();
                let prepared = session
                    .prepare_authoring_construction(black_box(&draft))
                    .unwrap();
                let prepare = start.elapsed().as_nanos();
                let start = Instant::now();
                let committed = session
                    .commit_authoring_construction(black_box(prepared))
                    .unwrap();
                let commit = start.elapsed().as_nanos();
                assert!(committed.report().applied);
                (prepare, commit)
            } else {
                let patch = linked_construction(
                    session.authoring_initial_component(&component).unwrap(),
                    7,
                );
                let start = Instant::now();
                let prepared = session
                    .prepare_authoring_edit(session.revision(), black_box(&patch))
                    .unwrap();
                let prepare = start.elapsed().as_nanos();
                let start = Instant::now();
                let report = session.commit_authoring_edit(black_box(prepared));
                let commit = start.elapsed().as_nanos();
                assert!(report.applied);
                (prepare, commit)
            };
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(session.document().entities.len(), 3);
            if iteration >= 8 {
                prepare_ns.push(prepare);
                commit_ns.push(commit);
            }
        }
        prepare_ns.sort_unstable();
        commit_ns.sort_unstable();
        println!(
            "{}",
            serde_json::json!({
                "measurement": "construction-prepare-commit-comparison-v1",
                "mode": if retained { "retained" } else { "explicit-control" },
                "build": if cfg!(debug_assertions) { "debug" } else { "release" },
                "warmups": 8, "samples": prepare_ns.len(),
                "entities_before": 1, "entities_created": 2, "components_per_created_entity": 1, "field_writes": 3,
                "prepare_median_ns": prepare_ns[32], "prepare_p95_ns": prepare_ns[60],
                "commit_median_ns": commit_ns[32], "commit_p95_ns": commit_ns[60],
                "prepare_samples_ns": prepare_ns, "commit_samples_ns": commit_ns,
                "scope": "owner preparation and consuming commit; excludes session admission, caller patch/draft construction, result disposal, native projection, Save and rendering; retained includes fresh 22-byte ID allocation, explicit control uses fixed 13/14-byte IDs"
            })
        );
    }
}
