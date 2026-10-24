#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, EntityReference, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, PrefabAuthoringSession,
    PrefabDocument, PrefabDocumentCandidate, SceneAuthoringHistoryLimits, SceneAuthoringSession,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
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

    let session = open(&document);
    let prepared = session
        .prepare_authoring_edit(session.revision(), &add(&["old-incarnation"]))
        .unwrap();
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
