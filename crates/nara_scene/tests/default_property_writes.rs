#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringPropertyEdit,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, SceneAuthoringHistoryLimits,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchOperation,
};

fn entity() -> SceneEntityId {
    SceneEntityId::new("item").unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.DefaultProperties")
}
fn field(name: &str) -> ComponentFieldId {
    ComponentFieldId::new(name)
}

fn bundle() -> AuthoringSchemaBundle {
    bundle_with_default(
        ComponentValue::map([
            ("wire_score", ComponentValue::U64(13)),
            ("wire_tag", ComponentValue::String("retained".into())),
        ]),
        ComponentFieldPath::from_fields(["defaults"]),
    )
}

fn bundle_with_default(default: ComponentValue, path: ComponentFieldPath) -> AuthoringSchemaBundle {
    let record = ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_score", ComponentValueKind::U64).with_id(field("score")),
        ComponentShapeMember::new("wire_tag", ComponentValueKind::String).with_id(field("tag")),
    ]);
    let schema = ComponentSchema::new(component(), "Defaults", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(["defaults", "unrelated"].map(|name| {
            let mut declaration = ComponentFieldSchema::optional_with_default(
                field(name),
                name,
                if name == "defaults" {
                    path.clone()
                } else {
                    ComponentFieldPath::from_fields([name])
                },
                record.clone(),
                default.clone(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING);
            declaration.required = name == "defaults";
            declaration
        }))
        .with_fields([
            ComponentFieldSchema::optional(
                field("plain"),
                "Plain",
                ComponentFieldPath::from_fields(["plain"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                field("weights"),
                "Weights",
                ComponentFieldPath::from_fields(["weights"]),
                ComponentValueShape::map(ComponentValueKind::U64.into()),
                ComponentValue::map([("kept", ComponentValue::U64(5))]),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.default-properties").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity())
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ),
        )
        .with_component(
            ComponentTypeId::new("game.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("opaque", ComponentValue::String("preserved".into()))]),
            ),
        )])
}

fn sessions() -> (SceneAuthoringSession, PrefabAuthoringSession) {
    sessions_with(bundle(), document(), SceneAuthoringHistoryLimits::default())
}

fn sessions_with(
    bundle: AuthoringSchemaBundle,
    document: SceneDocument,
    limits: SceneAuthoringHistoryLimits,
) -> (SceneAuthoringSession, PrefabAuthoringSession) {
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/defaults.json").unwrap());
    let scene = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source.clone(), &bundle, &adapters)
            .unwrap()
            .publish(),
        limits,
    );
    let prefab = PrefabDocument::new(document.roots, document.entities);
    let prefab = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source, &bundle, &adapters)
            .unwrap()
            .publish(),
        limits,
    );
    (scene, prefab)
}

fn edit(member: &str, value: ComponentValue) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        entity: entity(),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(field("defaults"))
            .with_steps([ComponentPropertyStep::Member(field(member))]),
        value,
    }
}

#[test]
fn first_nested_write_materializes_only_its_default_root_and_undo_restores_absence() {
    let (mut scene, mut prefab) = sessions();
    macro_rules! check {
        ($session:expr) => {{
            let original = $session.document().clone();
            let revision = $session.revision();
            let edits = [edit("score", ComponentValue::U64(15)),
                edit("tag", ComponentValue::String("changed".into()))];
            let cancelled = $session.prepare_authoring_property_edit(revision, &edits).unwrap();
            assert_eq!(cancelled.patch().operations.len(), 1);
            assert!(matches!(cancelled.inverse().operations.as_slice(),
                [ScenePatchOperation::RemoveField { field: id, .. }] if id == &field("defaults")));
            drop(cancelled);
            assert_eq!($session.document(), &original);
            assert_eq!($session.history_status().undo_depth, 0);
            assert_eq!($session.revision(), revision);
            let prepared = $session.prepare_authoring_property_edit(revision, &edits).unwrap();
            let stale = $session.prepare_authoring_property_edit(revision, &edits).unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            let accepted = $session.document().clone();
            let value = &accepted.entities[0].components[&component()].value;
            assert_eq!(value.get("defaults"), Some(&ComponentValue::map([
                ("wire_score", ComponentValue::U64(15)),
                ("wire_tag", ComponentValue::String("changed".into())),
            ])));
            assert!(value.get("unrelated").is_none());
            assert_eq!(accepted.entities[0].components[&ComponentTypeId::new("game.Unknown")],
                original.entities[0].components[&ComponentTypeId::new("game.Unknown")]);
            assert!(!$session.commit_authoring_edit(stale).applied);
            assert_eq!($session.document(), &accepted);
            assert_eq!($session.history_status().undo_depth, 1);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &original);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &accepted);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn failed_last_defaulted_property_write_leaves_source_revision_and_history_unchanged() {
    let (scene, prefab) = sessions();
    let edits = [
        edit("score", ComponentValue::U64(15)),
        edit("absent", ComponentValue::U64(99)),
    ];
    macro_rules! check {
        ($session:expr) => {{
            let original = $session.document().clone();
            let revision = $session.revision();
            let history = $session.history_status();
            assert!(
                $session
                    .prepare_authoring_property_edit(revision, &edits)
                    .is_err()
            );
            assert_eq!($session.document(), &original);
            assert_eq!($session.revision(), revision);
            assert_eq!($session.history_status(), history);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn explicit_null_and_partial_map_are_not_merged_with_defaults() {
    let mut document = document();
    let value = &mut document.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value;
    value
        .set_path(
            &ComponentFieldPath::from_fields(["unrelated"]),
            ComponentValue::Null,
        )
        .unwrap();
    value
        .set_path(
            &ComponentFieldPath::from_fields(["weights"]),
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
        )
        .unwrap();
    let (scene, prefab) = sessions_with(bundle(), document, SceneAuthoringHistoryLimits::default());
    for property in [
        ComponentPropertyPath::new(field("unrelated"))
            .with_steps([ComponentPropertyStep::Member(field("score"))]),
        ComponentPropertyPath::new(field("weights"))
            .with_steps([ComponentPropertyStep::MapEntry("kept".into())]),
    ] {
        let change = AuthoringPropertyEdit {
            property,
            ..edit("score", ComponentValue::U64(15))
        };
        assert!(
            scene
                .prepare_authoring_property_edit(scene.revision(), std::slice::from_ref(&change))
                .is_err()
        );
        assert!(
            prefab
                .prepare_authoring_property_edit(prefab.revision(), &[change])
                .is_err()
        );
    }
    assert_eq!(scene.document().entities, prefab.document().entities);
    assert_eq!(
        scene.document().entities[0].components[&component()]
            .value
            .get("unrelated"),
        Some(&ComponentValue::Null)
    );
    assert_eq!(scene.history_status().undo_depth, 0);
    assert_eq!(prefab.history_status().undo_depth, 0);
}

#[test]
fn root_assignment_can_create_an_optional_root_without_a_default() {
    let (mut scene, mut prefab) = sessions();
    let change = AuthoringPropertyEdit {
        property: ComponentPropertyPath::new(field("plain")),
        ..edit("score", ComponentValue::U64(8))
    };
    macro_rules! check {
        ($session:expr) => {{
            let before = $session.document().clone();
            let prepared = $session
                .prepare_authoring_property_edit($session.revision(), std::slice::from_ref(&change))
                .unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!(
                $session.document().entities[0].components[&component()]
                    .value
                    .get("plain"),
                Some(&ComponentValue::U64(8))
            );
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &before);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn first_write_requires_an_existing_storage_parent_and_preserves_it_on_undo() {
    let path = ComponentFieldPath::from_fields(["parent", "defaults"]);
    for present in [false, true] {
        let bundle = bundle_with_default(
            ComponentValue::map([
                ("wire_score", ComponentValue::U64(13)),
                ("wire_tag", ComponentValue::String("retained".into())),
            ]),
            path.clone(),
        );
        let mut document = document();
        if present {
            document.entities[0]
                .components
                .get_mut(&component())
                .unwrap()
                .value
                .set_path(
                    &ComponentFieldPath::from_fields(["parent"]),
                    ComponentValue::map([] as [(&str, ComponentValue); 0]),
                )
                .unwrap();
        }
        let (mut scene, mut prefab) =
            sessions_with(bundle, document, SceneAuthoringHistoryLimits::default());
        macro_rules! check {
            ($session:expr) => {{
                let before = $session.document().clone();
                let prepared = $session.prepare_authoring_property_edit(
                    $session.revision(),
                    &[edit("score", ComponentValue::U64(15))],
                );
                if present {
                    assert!($session.commit_authoring_edit(prepared.unwrap()).applied);
                    assert!($session.undo_authoring().applied);
                } else {
                    assert!(prepared.is_err());
                }
                assert_eq!($session.document(), &before);
            }};
        }
        check!(scene);
        check!(prefab);
    }
}

#[test]
fn final_root_replay_budget_can_refuse_expansion_but_allows_shrinking_a_large_default() {
    let bundle = bundle_with_default(
        ComponentValue::map([
            ("wire_score", ComponentValue::U64(13)),
            ("wire_tag", ComponentValue::String("x".repeat(80 * 1024))),
        ]),
        ComponentFieldPath::from_fields(["defaults"]),
    );
    let (mut scene, mut prefab) =
        sessions_with(bundle, document(), SceneAuthoringHistoryLimits::default());
    macro_rules! check {
        ($session:expr) => {{
            let before = $session.document().clone();
            assert!(
                $session
                    .prepare_authoring_property_edit(
                        $session.revision(),
                        &[edit("score", ComponentValue::U64(15))]
                    )
                    .is_err()
            );
            assert_eq!($session.document(), &before);
            let prepared = $session
                .prepare_authoring_property_edit(
                    $session.revision(),
                    &[edit("tag", ComponentValue::String("small".into()))],
                )
                .unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &before);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn default_expansion_still_requires_two_way_history_admission() {
    let limits = SceneAuthoringHistoryLimits::new(4, 1000, 1000, 4096, 1000, 1000, 1).unwrap();
    let (scene, prefab) = sessions_with(bundle(), document(), limits);
    assert!(
        scene
            .prepare_authoring_property_edit(
                scene.revision(),
                &[edit("score", ComponentValue::U64(15))]
            )
            .is_err()
    );
    assert!(
        prefab
            .prepare_authoring_property_edit(
                prefab.revision(),
                &[edit("score", ComponentValue::U64(15))]
            )
            .is_err()
    );
    assert_eq!(scene.document(), &document());
    assert_eq!(scene.history_status().undo_depth, 0);
    assert_eq!(prefab.history_status().undo_depth, 0);
}

#[test]
fn unadmitted_target_and_selector_ids_reject_without_retaining_private_input() {
    let (scene, prefab) = sessions();
    for target in [true, false] {
        let mut change = edit("score", ComponentValue::U64(15));
        let private = "private-unadmitted-identity".repeat(4096);
        if target {
            change.component = ComponentTypeId::new(private);
        } else {
            change.property.root = ComponentFieldId::new(private);
        }
        for report in [
            scene
                .prepare_authoring_property_edit(scene.revision(), std::slice::from_ref(&change))
                .unwrap_err(),
            prefab
                .prepare_authoring_property_edit(prefab.revision(), std::slice::from_ref(&change))
                .unwrap_err(),
        ] {
            assert!(!format!("{:?}", report.diagnostics).contains("private-unadmitted-identity"));
            assert!(report.diagnostics.iter().all(|diagnostic| {
                diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "operation-index"
                        && field.value() == nara_diagnostic::DiagnosticValueRef::Unsigned(0)
                })
            }));
        }
    }
    assert_eq!(scene.document(), &document());
    assert_eq!(scene.history_status().undo_depth, 0);
    assert_eq!(prefab.history_status().undo_depth, 0);
}

#[test]
fn creation_templates_remain_complete_values_not_decode_defaults() {
    let schema = ComponentSchema::new(component(), "Template", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            field("defaults"),
            "Defaults",
            ComponentFieldPath::from_fields(["defaults"]),
            ComponentValueKind::U64,
            ComponentValue::U64(13),
        )])
        .with_authoring_initial_value(ComponentValue::map([] as [(&str, ComponentValue); 0]));
    let result = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.complete-template").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )]);
    let Err(nara_reflect::AuthoringSchemaBundleError::Catalog { source, .. }) = result else {
        panic!("an incomplete creation template must fail catalog admission");
    };
    assert!(matches!(
        source.downcast_ref::<nara_reflect::ComponentRegistryError>(),
        Some(nara_reflect::ComponentRegistryError::InvalidComponentAuthoringInitialValue { .. })
    ));
}

#[test]
fn first_write_does_not_create_a_missing_list_slot_even_at_the_end() {
    use nara_reflect::ComponentFieldPathSegment;
    for present_empty_list in [false, true] {
        let path = ComponentFieldPath::new([
            ComponentFieldPathSegment::field("slots"),
            ComponentFieldPathSegment::index(0),
            ComponentFieldPathSegment::field("defaults"),
        ]);
        let bundle = bundle_with_default(
            ComponentValue::map([
                ("wire_score", ComponentValue::U64(13)),
                ("wire_tag", ComponentValue::String("retained".into())),
            ]),
            path,
        );
        let mut document = document();
        if present_empty_list {
            document.entities[0]
                .components
                .get_mut(&component())
                .unwrap()
                .value
                .set_path(
                    &ComponentFieldPath::from_fields(["slots"]),
                    ComponentValue::List(vec![]),
                )
                .unwrap();
        }
        let (scene, prefab) = sessions_with(
            bundle,
            document.clone(),
            SceneAuthoringHistoryLimits::default(),
        );
        assert!(
            scene
                .prepare_authoring_property_edit(
                    scene.revision(),
                    &[edit("score", ComponentValue::U64(15))]
                )
                .is_err()
        );
        assert!(
            prefab
                .prepare_authoring_property_edit(
                    prefab.revision(),
                    &[edit("score", ComponentValue::U64(15))]
                )
                .is_err()
        );
        assert_eq!(scene.document(), &document);
        assert_eq!(scene.document().entities, prefab.document().entities);
        assert_eq!(scene.history_status().undo_depth, 0);
        assert_eq!(prefab.history_status().undo_depth, 0);
    }
}
