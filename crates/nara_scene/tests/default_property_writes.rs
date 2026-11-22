#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentFloat, ComponentPropertyKey,
    ComponentPropertyPath, ComponentPropertyReadBudget, ComponentPropertyStep,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringEntityView,
    AuthoringPropertyEdit, AuthoringPropertyValue, AuthoringSchemaBasis, PrefabAuthoringSession,
    PrefabDocument, PrefabDocumentCandidate, SceneAuthoringHistoryLimits, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFileLimits, SceneFormatError, ScenePatchDocument, ScenePatchDocumentCandidate,
    ScenePatchOperation, ScenePlacement, ScenePlacementParent, SchemaReclassificationKind,
    SchemaReclassificationWork,
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
    sessions_with_file_limits(bundle, document, limits, SceneFileLimits::default())
}

fn sessions_with_file_limits(
    bundle: AuthoringSchemaBundle,
    document: SceneDocument,
    limits: SceneAuthoringHistoryLimits,
    file_limits: SceneFileLimits,
) -> (SceneAuthoringSession, PrefabAuthoringSession) {
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/defaults.json").unwrap());
    let scene = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            document.to_json_string().unwrap().as_bytes(),
            file_limits,
        )
        .unwrap()
        .admit_authoring(source.clone(), &bundle, &adapters)
        .unwrap()
        .publish(),
        limits,
    );
    let prefab = PrefabDocument::new(document.placements, document.entities);
    let prefab = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            prefab.to_json_string().unwrap().as_bytes(),
            file_limits,
        )
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
            let edits = [
                edit("score", ComponentValue::U64(15)),
                edit("tag", ComponentValue::String("changed".into())),
            ];
            let cancelled = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            assert!(matches!(
                cancelled.patch().operations.as_slice(),
                [
                    ScenePatchOperation::DefaultProperty(_),
                    ScenePatchOperation::ReplaceProperty { .. }
                ]
            ));
            assert!(matches!(
                cancelled.inverse().operations.as_slice(),
                [
                    ScenePatchOperation::ReplaceProperty { .. },
                    ScenePatchOperation::DefaultProperty(_)
                ]
            ));
            drop(cancelled);
            assert_eq!($session.document(), &original);
            assert_eq!($session.history_status().undo_depth, 0);
            assert_eq!($session.revision(), revision);
            let prepared = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            let stale = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            let accepted = $session.document().clone();
            let value = accepted.entities[0].components[&component()].value();
            assert_eq!(
                value.get("defaults"),
                Some(&ComponentValue::map([
                    ("wire_score", ComponentValue::U64(15)),
                    ("wire_tag", ComponentValue::String("changed".into())),
                ]))
            );
            assert!(value.get("unrelated").is_none());
            assert_eq!(
                accepted.entities[0].components[&ComponentTypeId::new("game.Unknown")],
                original.entities[0].components[&ComponentTypeId::new("game.Unknown")]
            );
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
fn copied_default_property_history_refuses_raw_native_and_wire_replay() {
    let (mut scene, mut prefab) = sessions();
    let edits = [edit("score", ComponentValue::U64(15))];
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    macro_rules! refuse_public_replay {
        ($session:expr) => {{
            let before = $session.document().clone();
            let revision = $session.revision();
            let prepared = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            for patch in [prepared.patch().clone(), prepared.inverse().clone()] {
                assert!(matches!(
                    patch.operations.as_slice(),
                    [ScenePatchOperation::DefaultProperty(_)]
                ));
                let report = $session
                    .prepare_authoring_edit(revision, &patch)
                    .unwrap_err();
                assert_eq!(
                    report.diagnostics.iter().next().unwrap().code().as_str(),
                    "scene.patch-session-only-operation",
                );
                assert!(!$session.apply_authoring_patch(&patch).applied);
                assert!(serde_json::to_value(&patch).is_err());
                for result in [patch.to_json_string(), patch.to_ron_string()] {
                    assert!(matches!(
                        result,
                        Err(SceneFormatError::SessionOnlyPatchOperation { operation_index: 0 })
                    ));
                }
                let mut native = SceneDocument::default();
                let report = patch.apply_to_scene(&mut native, &registry);
                assert!(!report.applied);
                assert_eq!(
                    report.diagnostics.iter().next().unwrap().code().as_str(),
                    "scene.patch-session-only-operation",
                );
                assert_eq!(native, SceneDocument::default());
            }
            assert_eq!($session.document(), &before);
            assert_eq!($session.revision(), revision);
            assert_eq!($session.history_status().undo_depth, 0);
            assert!($session.commit_authoring_edit(prepared).applied);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &before);
        }};
    }
    refuse_public_replay!(scene);
    refuse_public_replay!(prefab);

    let forged = r#"{
        "format_version": 1,
        "operations": [{"op": "default_property", "args": {}}]
    }"#;
    assert!(
        serde_json::from_str::<ScenePatchOperation>(r#"{"op": "default_property", "args": {}}"#,)
            .is_err()
    );
    let encoded_empty = nara_scene::ScenePatchDocument::default()
        .to_json_string()
        .unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&encoded_empty).unwrap();
    wire["payload"] = serde_json::from_str(forged).unwrap();
    assert!(ScenePatchDocumentCandidate::decode_json_str(&wire.to_string()).is_err());
}

fn keyed_bundle(zero: f64, row_count: u64) -> AuthoringSchemaBundle {
    let rows = ComponentValue::List(
        (1..=row_count)
            .map(|row| {
                ComponentValue::map([
                    ("wire_key", ComponentValue::String(format!("row-{row}"))),
                    ("wire_score", ComponentValue::U64(row)),
                    (
                        "wire_zero",
                        ComponentValue::F64(ComponentFloat::new(zero).unwrap()),
                    ),
                ])
            })
            .collect(),
    );
    let schema = ComponentSchema::new(component(), "Rows", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            field("rows"),
            "Rows",
            ComponentFieldPath::from_fields(["rows"]),
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("wire_key", ComponentValueKind::String)
                        .with_id(field("id")),
                    ComponentShapeMember::new("wire_score", ComponentValueKind::U64)
                        .with_id(field("score")),
                    ComponentShapeMember::new("wire_zero", ComponentValueKind::F64)
                        .with_id(field("zero")),
                ]),
                field("id"),
            ),
            rows.clone(),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([("rows", rows)]));
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

fn keyed_edit(key: &str, member: &str, value: ComponentValue) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        property: ComponentPropertyPath::new(field("rows")).with_steps([
            ComponentPropertyStep::ListItem {
                by: field("id"),
                key: ComponentPropertyKey::String(key.into()),
            },
            ComponentPropertyStep::Member(field(member)),
        ]),
        ..edit(member, value)
    }
}

#[test]
fn exact_stored_property_batches_preserve_revision_and_history() {
    let (mut scene, mut prefab) = sessions_with(
        keyed_bundle(0.0, 2),
        document(),
        SceneAuthoringHistoryLimits::default(),
    );
    let set_score = |score| keyed_edit("row-1", "score", ComponentValue::U64(score));
    let same_leaf = [set_score(10)];
    let restore_through_renamed_key = [
        keyed_edit("row-1", "id", ComponentValue::String("renamed".into())),
        keyed_edit("renamed", "score", ComponentValue::U64(20)),
        keyed_edit("renamed", "score", ComponentValue::U64(10)),
        keyed_edit("renamed", "id", ComponentValue::String("row-1".into())),
    ];

    macro_rules! check {
        ($session:ident) => {{
            for score in [10, 11] {
                let prepared = $session
                    .prepare_authoring_property_edit($session.revision(), &[set_score(score)])
                    .unwrap();
                assert!($session.commit_authoring_edit(prepared).applied);
            }
            let redo_document = $session.document().clone();
            assert!($session.undo_authoring().applied);
            let before = $session.document().clone();
            let revision = $session.revision();
            let history = $session.history_status();
            let usage = $session.history_usage();
            let retained = $session.shared_document();
            assert_eq!((history.undo_depth, history.redo_depth), (1, 1));
            let same_root = [AuthoringPropertyEdit {
                property: ComponentPropertyPath::new(field("rows")),
                value: before.entities[0].components[&component()]
                    .value()
                    .get("rows")
                    .unwrap()
                    .clone(),
                ..same_leaf[0].clone()
            }];
            for edits in [
                same_root.as_slice(),
                same_leaf.as_slice(),
                restore_through_renamed_key.as_slice(),
            ] {
                let prepared = $session
                    .prepare_authoring_property_edit(revision, edits)
                    .unwrap();
                assert_eq!(prepared.document(), &before);
                assert!(prepared.patch().is_empty(), "{:?}", prepared.patch());
                assert!(prepared.inverse().is_empty(), "{:?}", prepared.inverse());

                let report = $session.commit_authoring_edit(prepared);
                assert!(report.is_unchanged());
                assert_eq!(report.inverse, Some(ScenePatchDocument::default()));
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                assert_eq!($session.document(), &before);
                assert_eq!($session.revision(), revision);
                assert_eq!($session.history_status(), history);
                assert_eq!($session.history_usage(), usage);
                assert!(std::sync::Arc::ptr_eq(
                    &retained,
                    &$session.shared_document()
                ));
            }
            let mut draft = $session.begin_authoring_construction(revision).unwrap();
            draft
                .set_property(
                    entity(),
                    component(),
                    same_leaf[0].property.clone(),
                    same_leaf[0].value.clone(),
                )
                .unwrap();
            let prepared = $session.prepare_authoring_construction(&draft).unwrap();
            assert!(prepared.patch().is_empty());
            let committed = $session.commit_authoring_construction(prepared).unwrap();
            assert!(committed.report().is_unchanged());
            assert_eq!($session.revision(), revision);
            assert_eq!($session.history_status(), history);
            let stale = $session
                .prepare_authoring_property_edit(revision, &same_leaf)
                .unwrap();
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &redo_document);
            assert!($session.undo_authoring().applied);
            let restored_revision = $session.revision();
            assert_ne!(restored_revision, revision);
            assert_eq!($session.document(), &before);
            let rejected = $session.commit_authoring_edit(stale);
            assert!(!rejected.applied);
            assert!(!rejected.is_unchanged());
            assert!(rejected.diagnostics.has_errors());
            assert_eq!($session.revision(), restored_revision);
            assert_eq!($session.history_status(), history);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &redo_document);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn same_effective_default_property_write_materializes_storage_and_replays() {
    let (mut scene, mut prefab) = sessions();
    let edits = [edit("score", ComponentValue::U64(13))];
    let assert_score = |view: AuthoringEntityView<'_>, expected: AuthoringPropertyValue<'_>| {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(16).unwrap(),
            ByteLimit::new(1024).unwrap(),
        );
        assert_eq!(
            view.component(&component())
                .unwrap()
                .property_value(&edits[0].property, &mut budget)
                .unwrap(),
            expected,
        );
    };
    let stored_default = ComponentValue::map([
        ("wire_score", ComponentValue::U64(13)),
        ("wire_tag", ComponentValue::String("retained".into())),
    ]);

    macro_rules! check {
        ($session:ident) => {{
            let before = $session.document().clone();
            let revision = $session.revision();
            let stored = before.entities[0].components[&component()].value();
            assert!(stored.get("defaults").is_none());
            assert_score(
                $session.authoring_entity_view(&entity()).unwrap(),
                AuthoringPropertyValue::Defaulted(&edits[0].value),
            );

            let prepared = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            assert!(!prepared.patch().is_empty());
            assert!(!prepared.inverse().is_empty());
            assert_score(
                prepared.authoring_entity_view(&entity()).unwrap(),
                AuthoringPropertyValue::Present(&edits[0].value),
            );
            let materialized = prepared.document().clone();
            let stored = materialized.entities[0].components[&component()].value();
            assert_eq!(stored.get("defaults"), Some(&stored_default));

            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!($session.document(), &materialized);
            assert_ne!($session.revision(), revision);
            assert_eq!($session.history_status().undo_depth, 1);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &before);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &materialized);
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn changing_stored_zero_sign_creates_a_revision_and_replays_exact_bits() {
    let (mut scene, mut prefab) = sessions_with(
        keyed_bundle(0.0, 2),
        document(),
        SceneAuthoringHistoryLimits::default(),
    );
    let seed = [keyed_edit("row-1", "score", ComponentValue::U64(10))];
    let negative_zero = [keyed_edit(
        "row-1",
        "zero",
        ComponentValue::F64(ComponentFloat::new(-0.0).unwrap()),
    )];
    let zero_bits = |entities: &[SceneEntityRecord]| {
        let stored = entities[0].components[&component()].value();
        let Some(ComponentValue::List(rows)) = stored.get("rows") else {
            panic!("stored rows must be present");
        };
        let Some(ComponentValue::F64(zero)) = rows[0].get("wire_zero") else {
            panic!("the first row must contain a stored zero");
        };
        zero.get().to_bits()
    };

    macro_rules! check {
        ($session:ident) => {{
            let prepared = $session
                .prepare_authoring_property_edit($session.revision(), &seed)
                .unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!(zero_bits(&$session.document().entities), 0.0_f64.to_bits());

            let revision = $session.revision();
            let prepared = $session
                .prepare_authoring_property_edit(revision, &negative_zero)
                .unwrap();
            assert!(!prepared.patch().is_empty());
            assert!(!prepared.inverse().is_empty());
            assert_eq!(
                zero_bits(&prepared.document().entities),
                (-0.0_f64).to_bits()
            );

            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!(
                zero_bits(&$session.document().entities),
                (-0.0_f64).to_bits()
            );
            assert_ne!($session.revision(), revision);
            assert_eq!($session.history_status().undo_depth, 2);
            assert!($session.undo_authoring().applied);
            assert_eq!(zero_bits(&$session.document().entities), 0.0_f64.to_bits());
            assert!($session.redo_authoring().applied);
            assert_eq!(
                zero_bits(&$session.document().entities),
                (-0.0_f64).to_bits()
            );
        }};
    }
    check!(scene);
    check!(prefab);
}

#[test]
fn large_keyed_default_replays_compactly_across_exact_schema_readmission() {
    let bundle = keyed_bundle(-0.0, 86);
    let signed_zero_drift = keyed_bundle(0.0, 86);
    assert_eq!(
        bundle.owners()[0].current().fingerprint(),
        signed_zero_drift.owners()[0].current().fingerprint(),
    );
    let basis = |bundle: &AuthoringSchemaBundle| {
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        AuthoringSchemaBasis::admit(bundle, &[adapter]).unwrap()
    };
    let drift = basis(&signed_zero_drift);
    let history = SceneAuthoringHistoryLimits::new(4, 128, 4, 4096, 128, 2, 4096).unwrap();
    let (mut scene, mut prefab) = sessions_with(bundle.clone(), document(), history);
    let edits = [
        keyed_edit("row-86", "id", ComponentValue::String("renamed".into())),
        keyed_edit("renamed", "score", ComponentValue::U64(85)),
    ];
    let mut expected = bundle.owners()[0].current().components[0].fields()[0]
        .default_value()
        .unwrap()
        .clone();
    let ComponentValue::List(rows) = &mut expected else {
        panic!("a keyed-list default must contain rows");
    };
    rows[85] = ComponentValue::map([
        ("wire_key", ComponentValue::String("renamed".into())),
        ("wire_score", ComponentValue::U64(85)),
        (
            "wire_zero",
            ComponentValue::F64(ComponentFloat::new(-0.0).unwrap()),
        ),
    ]);

    macro_rules! readmit {
        ($session:ident) => {{
            let before = $session.document().clone();
            let status = ($session.revision(), $session.history_status());
            let current = $session.authoring_schema_basis().unwrap();
            let refusal = $session
                .prepare_schema_reclassification(
                    &drift,
                    &SchemaReclassificationKind::DeclarationExtension,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap_err();
            assert_eq!(
                refusal.diagnostics.iter().next().unwrap().code().as_str(),
                "scene.authoring-default-property-basis-mismatch",
            );
            assert_eq!($session.authoring_schema_basis(), Some(current.clone()));
            let same = basis(&bundle);
            assert_ne!(current, same);
            let prepared = $session
                .prepare_schema_reclassification(
                    &same,
                    &SchemaReclassificationKind::DeclarationExtension,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap();
            $session
                .bind_schema_reclassification(prepared)
                .unwrap()
                .commit();
            assert_eq!($session.authoring_schema_basis(), Some(same));
            assert_eq!($session.document(), &before);
            assert_eq!(($session.revision(), $session.history_status()), status);
        }};
    }
    macro_rules! qualify {
        ($session:ident) => {{
            let original = $session.document().clone();
            let mut edited = original.clone();
            edited.entities[0]
                .components
                .get_mut(&component())
                .unwrap()
                .value_mut()
                .set_path(&ComponentFieldPath::from_fields(["rows"]), expected.clone())
                .unwrap();
            let prepared = $session
                .prepare_authoring_property_edit($session.revision(), &edits)
                .unwrap();
            assert!(matches!(
                prepared.patch().operations.as_slice(),
                [
                    ScenePatchOperation::DefaultProperty(_),
                    ScenePatchOperation::ReplaceProperty { .. },
                ]
            ));
            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!($session.document(), &edited);
            readmit!($session);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &original);
            readmit!($session);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &edited);
        }};
    }
    qualify!(scene);
    qualify!(prefab);
}

#[test]
fn construction_materializes_omitted_defaults_alongside_staged_component_edits() {
    let (mut scene, mut prefab) = sessions_with(
        keyed_bundle(-0.0, 86),
        document(),
        SceneAuthoringHistoryLimits::default(),
    );
    let edit = keyed_edit("row-86", "score", ComponentValue::U64(85));
    macro_rules! qualify {
        ($session:ident) => {{
            let original = $session.document().clone();
            let mut draft = $session
                .begin_authoring_construction($session.revision())
                .unwrap();
            let local = draft
                .create_entity(ScenePlacement::last(ScenePlacementParent::Root))
                .unwrap();
            draft.add_component(&local, component()).unwrap();
            draft
                .set_property(
                    entity(),
                    component(),
                    edit.property.clone(),
                    edit.value.clone(),
                )
                .unwrap();
            draft
                .set_property(
                    &local,
                    component(),
                    edit.property.clone(),
                    ComponentValue::U64(42),
                )
                .unwrap();
            let prepared = $session.prepare_authoring_construction(&draft).unwrap();
            assert!(matches!(
                prepared.patch().operations.as_slice(),
                [
                    ScenePatchOperation::AddEntity { .. },
                    ScenePatchOperation::SetPlacements { .. },
                    ScenePatchOperation::AddComponent { .. },
                    ScenePatchOperation::DefaultProperty(_),
                    ScenePatchOperation::ReplaceProperty { .. },
                ]
            ));
            let staged = prepared.entity(&local).unwrap().clone();
            let expected = prepared.document().clone();
            for (id, score) in [(entity(), 85), (staged, 42)] {
                let record = expected
                    .entities
                    .iter()
                    .find(|record| record.id == id)
                    .unwrap();
                let rows = record.components[&component()].value().get("rows");
                let Some(ComponentValue::List(rows)) = rows else {
                    panic!("both edited component records must contain rows");
                };
                assert_eq!(
                    rows[85].get("wire_score"),
                    Some(&ComponentValue::U64(score))
                );
            }
            assert!(
                $session
                    .commit_authoring_construction(prepared)
                    .unwrap()
                    .report()
                    .applied
            );
            assert_eq!($session.document(), &expected);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &original);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &expected);
        }};
    }
    qualify!(scene);
    qualify!(prefab);
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
    let value = document.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value_mut();
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
            .value()
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
                    .value()
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
                .value_mut()
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
fn large_default_uses_leaf_history_cost_and_the_full_source_byte_limit() {
    let bundle = bundle_with_default(
        ComponentValue::map([
            ("wire_score", ComponentValue::U64(13)),
            ("wire_tag", ComponentValue::String("x".repeat(80 * 1024))),
        ]),
        ComponentFieldPath::from_fields(["defaults"]),
    );
    let (mut scene, mut prefab) = sessions_with(
        bundle.clone(),
        document(),
        SceneAuthoringHistoryLimits::default(),
    );
    let (mut tight_scene, mut tight_prefab) = sessions_with_file_limits(
        bundle,
        document(),
        SceneAuthoringHistoryLimits::default(),
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(4096).unwrap()),
    );
    macro_rules! check {
        ($session:expr, $tight:expr) => {{
            let before = $session.document().clone();
            let score = $session.prepare_authoring_property_edit(
                $session.revision(),
                &[edit("score", ComponentValue::U64(15))],
            );
            if $tight {
                let report = score.unwrap_err();
                assert_eq!(
                    report.diagnostics.iter().next().unwrap().code().as_str(),
                    "scene.authoring-field-edit-format-invalid",
                );
            } else {
                assert!($session.commit_authoring_edit(score.unwrap()).applied);
                let value = $session.document().entities[0].components[&component()].value();
                assert_eq!(
                    value.get("defaults").unwrap().get("wire_tag"),
                    Some(&ComponentValue::String("x".repeat(80 * 1024))),
                );
                assert!($session.undo_authoring().applied);
                assert_eq!($session.document(), &before);
                assert!($session.redo_authoring().applied);
                assert!($session.undo_authoring().applied);
            }
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
    check!(scene, false);
    check!(prefab, false);
    check!(tight_scene, true);
    check!(tight_prefab, true);
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
                .value_mut()
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
