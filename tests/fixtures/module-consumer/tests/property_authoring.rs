//! Data-only property authoring through the ordinary public Editor Host and source files.

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetRef;
use nara_engine::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorEditPreparationError, EditorProjectCloseProgress, EditorProjectOpenHost,
        EditorProjectOpenProgress, EditorProjectSession,
    },
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentCodecError,
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentPropertyError,
    ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep, ComponentRegistry,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId, replace_component_property, resolve_component_property,
};
use nara_scene::{
    AuthoringDocumentReadiness, AuthoringPropertyEdit, AuthoringRecordLocation,
    AuthoringRecordReadiness, PrefabDocument, PrefabDocumentCandidate, PrefabInstance,
    SceneAuthoringRevision, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
    ScenePlacementOwner, UnavailableRecordManifest, UnavailableRecordManifestEntry,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentKind,
    EditorDocumentMutation, EditorDocumentOpenResult, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, PrefabCompositionCommand,
    PrefabCompositionEntityDraft, PrefabCompositionOutcome, SceneCompositionCommand,
    SceneCompositionEntityKind,
};

const COMPONENT: &str = "consumer.property.Settings";
const FUTURE: &str = "consumer.property.Future";
const UNKNOWN: &str = "consumer.unknown.Opaque";
const SCENE: &str = "scenes/startup.scene.json";
const PREFAB: &str = "prefabs/settings.prefab.json";
const MAP_KEY: &str = "literal.key[0]";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn member(wire: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(wire, shape).with_id(ComponentFieldId::new(stable))
}

fn bundle() -> AuthoringSchemaBundle {
    let shape = ComponentValueShape::record_members([
        member(
            "wire_stats",
            "stats",
            ComponentValueShape::record_members([
                member("health", "hp", ComponentValueKind::U64),
                member("energy", "mp", ComponentValueKind::U64),
            ]),
        ),
        member(
            "wire_rows",
            "rows",
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    member("wire_key", "id", ComponentValueKind::String),
                    member("wire_score", "score", ComponentValueKind::U64),
                ]),
                ComponentFieldId::new("id"),
            ),
        ),
        member(
            "wire_labels",
            "labels",
            ComponentValueShape::map(ComponentValueKind::String.into()),
        ),
    ]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("consumer.property").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Settings",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("settings"),
                    "Settings",
                    ComponentFieldPath::from_fields(["payload"]),
                    shape,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                ComponentSchema::new(
                    ComponentTypeId::new(FUTURE),
                    "Future",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("token"),
                    "Token",
                    ComponentFieldPath::from_fields(["token"]),
                    ComponentValueKind::String,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn row(key: &str, score: u64) -> ComponentValue {
    ComponentValue::map([
        ("wire_key", ComponentValue::String(key.to_owned())),
        ("wire_score", ComponentValue::U64(score)),
    ])
}

fn settings(hp: u64, mp: u64, key: &str, score: u64, label: &str) -> ComponentValue {
    ComponentValue::map([
        (
            "wire_stats",
            ComponentValue::map([
                ("health", ComponentValue::U64(hp)),
                ("energy", ComponentValue::U64(mp)),
            ]),
        ),
        (
            "wire_rows",
            ComponentValue::List(vec![row(key, score), row("untouched", 99)]),
        ),
        (
            "wire_labels",
            ComponentValue::map([
                (MAP_KEY, ComponentValue::String(label.to_owned())),
                ("untouched", ComponentValue::String("keep".to_owned())),
            ]),
        ),
    ])
}

fn initial() -> ComponentValue {
    settings(10, 20, "a", 1, "before")
}

fn records(root: ComponentValue) -> Vec<SceneEntityRecord> {
    let opaque = ComponentValue::map([
        (
            "token",
            ComponentValue::String("preserve exactly".to_owned()),
        ),
        (
            "future_payload",
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::map([("nested", ComponentValue::U64(u64::MAX))]),
                ComponentValue::Bool(true),
            ]),
        ),
    ]);
    vec![
        SceneEntityRecord::new(id("actor"))
            .with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("payload", root)]),
                ),
            )
            .with_component(
                ComponentTypeId::new(UNKNOWN),
                SceneComponentRecord::new(ComponentSchemaVersion(7), opaque.clone()).with_owner(
                    PersistentComponentSchemaOwnerId::try_new("consumer.unknown").unwrap(),
                ),
            )
            .with_component(
                ComponentTypeId::new(FUTURE),
                SceneComponentRecord::new(ComponentSchemaVersion(9), opaque).with_owner(
                    PersistentComponentSchemaOwnerId::try_new("consumer.property").unwrap(),
                ),
            ),
        SceneEntityRecord::new(id("observer")),
    ]
}

fn step(stable: &str) -> ComponentPropertyStep {
    ComponentPropertyStep::Member(ComponentFieldId::new(stable))
}

fn item(key: &str) -> ComponentPropertyStep {
    ComponentPropertyStep::ListItem {
        by: ComponentFieldId::new("id"),
        key: ComponentPropertyKey::String(key.to_owned()),
    }
}

fn edit(
    steps: impl IntoIterator<Item = ComponentPropertyStep>,
    value: ComponentValue,
) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        entity: id("actor"),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps(steps),
        value,
    }
}

fn patch(root: ComponentValue) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("actor"),
        component: ComponentTypeId::new(COMPONENT),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("settings"),
        value: root,
    }])
}

fn instance_patch(expected: Option<PrefabInstance>, value: Option<PrefabInstance>) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("actor"),
        expected,
        value,
    }])
}

#[derive(bevy_ecs::prelude::Component)]
struct NativeSettings(ComponentValue);

#[test]
fn ordinary_creation_retains_generated_identity_and_unavailable_records_through_saved_reopen() {
    for source in [SCENE, PREFAB] {
        let project = Project::new();
        let mut editor = open_project(&project);
        let document = open_source(&mut editor, source);
        let manifest = manifest(&editor);
        let base_revision = revision(&editor, document);
        let placement = ScenePlacement::last(ScenePlacementOwner::Root);
        let mutation = if source == SCENE {
            EditorDocumentMutation::ComposeScene {
                document,
                base_revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement,
                },
            }
        } else {
            EditorDocumentMutation::ComposePrefab {
                document,
                base_revision,
                command: PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default()).unwrap(),
                    placement,
                },
            }
        };
        let outcome = dispatch(&mut editor, EditorAction::Mutate(mutation));
        let frame = editor.current_frame();
        let created = frame
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .top_entity()
            .unwrap()
            .clone();
        assert_ne!(created, id("actor"));
        assert_ne!(created, id("observer"));
        let EditorActionOutcome::Workspace(report) = outcome else {
            panic!("{outcome:?}")
        };
        assert!(report.applied, "{:?}", report.diagnostics);
        if source == PREFAB {
            assert_eq!(
                report.prefab_composition_outcome,
                Some(PrefabCompositionOutcome::Created {
                    entity: created.clone()
                })
            );
        }
        assert_eq!(
            frame
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            1
        );
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(&manifest)
        );
        for redo in [false, true] {
            let base_revision = revision(&editor, document);
            let mutation = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            };
            assert_applied(dispatch(&mut editor, EditorAction::Mutate(mutation)));
            let frame = editor.current_frame();
            let scene = frame.workspace().active_scene.as_ref().unwrap();
            assert_eq!(
                scene
                    .editor
                    .inspector
                    .entities
                    .iter()
                    .any(|row| row.id == created),
                redo
            );
            assert_eq!(
                frame
                    .workspace()
                    .document(document)
                    .unwrap()
                    .history
                    .undo_depth,
                usize::from(redo)
            );
            assert_eq!(
                editor.active_authoring_unavailable_record_manifest(),
                Some(&manifest)
            );
        }
        let saved_revision = revision(&editor, document);
        assert!(matches!(
            dispatch(
                &mut editor,
                EditorAction::Persistence(EditorPersistenceAction::Save {
                    document,
                    base_revision: saved_revision
                })
            ),
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        ));
        wait(&mut editor, |editor| {
            editor.current_frame().persistence().result().is_some()
        });
        assert!(
            matches!(editor.current_frame().persistence().result(), Some(EditorPersistenceResult::Saved { document: saved, revision, .. }) if saved == document && revision == saved_revision)
        );
        let mut expected = records(initial());
        expected.push(SceneEntityRecord::new(created.clone()));
        let expected = if source == SCENE {
            SceneDocument::from_ordered_roots(expected)
                .to_json_string()
                .unwrap()
        } else {
            PrefabDocument::from_ordered_roots(expected)
                .to_json_string()
                .unwrap()
        };
        assert_eq!(
            fs::read(project.root.join(source)).unwrap(),
            expected.as_bytes()
        );
        let attempt = editor
            .current_frame()
            .persistence()
            .attempt()
            .expect("the test observed a terminal persistence result");
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
        );
        let target = editor.current_frame().reopen_target();
        assert!(matches!(
            dispatch(
                &mut editor,
                EditorAction::Persistence(EditorPersistenceAction::Reopen { target })
            ),
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        ));
        wait(&mut editor, |editor| {
            editor.current_frame().persistence().result().is_some()
        });
        assert!(matches!(
            editor.current_frame().persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ));
        let attempt = editor
            .current_frame()
            .persistence()
            .attempt()
            .expect("the test observed a terminal persistence result");
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
        );
        select(&mut editor, document, created.as_str());
        let reopened = editor.current_frame();
        let tab = reopened.workspace().document(document).unwrap();
        assert!(!tab.dirty);
        assert_eq!(tab.history.undo_depth, 0);
        assert_ne!(tab.revision.source_id(), saved_revision.source_id());
        assert_eq!(
            reopened
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection
                .top_entity(),
            Some(&created)
        );
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(&manifest)
        );
        close_editor(&mut editor);
    }
}

#[test]
fn captured_property_survives_explicit_wire_migration_and_keyed_reorder() {
    let schemas = bundle();
    let previous = schemas.owners()[0]
        .current()
        .components
        .iter()
        .find(|schema| schema.id.as_str() == COMPONENT)
        .unwrap();
    let value = ComponentValue::map([("payload", initial())]);
    let target = ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
        step("rows"),
        item("a"),
        step("score"),
    ]);
    assert_eq!(
        resolve_component_property(previous, previous.version(), &value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(1)
    );
    let mut current = previous.clone();
    current.version = ComponentSchemaVersion(2);
    let ComponentValueShape::Record { members } = &mut current.fields[0].shape else {
        panic!("settings are a record");
    };
    let ComponentValueShape::KeyedList {
        item: item_shape, ..
    } = &mut members[1].shape
    else {
        panic!("rows are keyed");
    };
    let ComponentValueShape::Record { members } = item_shape.as_mut() else {
        panic!("each row is a record");
    };
    members[1].name = "points".to_owned();
    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(current.clone()).unwrap();
    registry
        .register_native_component_with_codec::<NativeSettings, _, _>(
            &current.id,
            |value| Ok(NativeSettings(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry
        .register_component_migration(
            &current.id,
            previous.version(),
            current.version(),
            |mut value| {
                let ComponentValue::List(rows) = value
                    .get_path_mut(&ComponentFieldPath::from_fields(["payload", "wire_rows"]))
                    .map_err(|_| ComponentCodecError::missing_field("wire_rows"))?
                else {
                    return Err(ComponentCodecError::invalid_field("wire_rows", "list"));
                };
                for row in rows.iter_mut() {
                    let ComponentValue::Map(row) = row else {
                        return Err(ComponentCodecError::invalid_field("row", "record"));
                    };
                    let score = row
                        .remove("wire_score")
                        .ok_or_else(|| ComponentCodecError::missing_field("wire_score"))?;
                    row.insert("points".to_owned(), score);
                }
                rows.reverse();
                Ok(value)
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let schema = registry.schema(&current.id).unwrap();
    for version in [previous.version(), ComponentSchemaVersion(3)] {
        assert!(matches!(
            resolve_component_property(schema, version, &value, &target),
            Err(ComponentPropertyError::VersionMismatch { .. })
        ));
    }
    assert!(resolve_component_property(schema, schema.version(), &value, &target).is_err());
    let migrated = registry
        .migrate_component_value(&schema.id, previous.version(), &value)
        .unwrap();
    let mut value = migrated.value;
    assert_eq!(
        value
            .get_path(&ComponentFieldPath::from_fields(["payload", "wire_rows"]))
            .unwrap(),
        &ComponentValue::List(vec![
            ComponentValue::map([
                ("wire_key", ComponentValue::String("untouched".into())),
                ("points", ComponentValue::U64(99)),
            ]),
            ComponentValue::map([
                ("wire_key", ComponentValue::String("a".into())),
                ("points", ComponentValue::U64(1)),
            ]),
        ])
    );
    assert_eq!(
        replace_component_property(
            schema,
            migrated.version,
            &mut value,
            &target,
            ComponentValue::U64(7)
        )
        .unwrap(),
        ComponentValue::U64(1)
    );
    assert_eq!(
        resolve_component_property(schema, migrated.version, &value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(7)
    );
    let untouched = ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
        step("rows"),
        item("untouched"),
        step("score"),
    ]);
    assert_eq!(
        resolve_component_property(schema, migrated.version, &value, &untouched)
            .unwrap()
            .value,
        &ComponentValue::U64(99)
    );
}

#[test]
fn nested_writes_coalesce_and_keep_exact_basis_history_and_saved_records() {
    for source in [SCENE, PREFAB] {
        let project = Project::new();
        let mut editor = open_project(&project);
        let document = open_source(&mut editor, source);
        select(&mut editor, document, "actor");
        let manifest = manifest(&editor);
        let before = editor.current_frame();
        let basis = revision(&editor, document);
        let edits = [
            edit([step("stats"), step("hp")], ComponentValue::U64(30)),
            edit([step("stats"), step("mp")], ComponentValue::U64(40)),
        ];
        let expected = settings(30, 40, "a", 1, "before");

        let cancelled = editor
            .prepare_authoring_property_edit(document, basis, &edits)
            .unwrap();
        assert_eq!(cancelled.patch(), &patch(expected.clone()));
        assert_eq!(cancelled.inverse(), &patch(initial()));
        drop(cancelled);
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.current_frame().workspace(), before.workspace());

        let prepared = editor
            .prepare_authoring_property_edit(document, basis, &edits)
            .unwrap();
        let stale = editor
            .prepare_authoring_property_edit(document, basis, &edits)
            .unwrap();
        assert_eq!(prepared.document(), document);
        assert_eq!(
            prepared.document_kind(),
            if source == SCENE {
                EditorDocumentKind::Scene
            } else {
                EditorDocumentKind::Prefab
            }
        );
        assert_eq!(prepared.base_revision(), basis);
        assert_eq!(prepared.patch(), &patch(expected.clone()));
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.current_frame().workspace(), before.workspace());
        assert_applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(root(&editor), expected);
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(&manifest)
        );
        let committed = editor.current_frame();
        let tab = committed.workspace().document(document).unwrap();
        assert!(tab.dirty);
        assert_eq!(tab.history.undo_depth, 1);
        assert_eq!(tab.history.redo_depth, 0);
        assert_ne!(tab.revision, basis);
        assert_eq!(
            fs::read(project.root.join(source)).unwrap(),
            project.original(source)
        );

        assert!(
            editor
                .prepare_authoring_property_edit(document, basis, &edits)
                .is_err()
        );
        let rejected = editor.commit_authoring_edit(stale);
        assert!(
            matches!(rejected, EditorActionOutcome::Workspace(ref report) if !report.applied),
            "{rejected:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().workspace().documents,
            committed.workspace().documents
        );
        assert_eq!(
            editor.current_frame().workspace().active_scene,
            committed.workspace().active_scene
        );
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(&manifest)
        );

        for (redo, expected_root) in [(false, initial()), (true, expected.clone())] {
            let base_revision = revision(&editor, document);
            let mutation = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            };
            assert_applied(dispatch(&mut editor, EditorAction::Mutate(mutation)));
            assert_eq!(root(&editor), expected_root);
            assert_eq!(
                editor.active_authoring_unavailable_record_manifest(),
                Some(&manifest)
            );
            let frame = editor.current_frame();
            let tab = frame.workspace().document(document).unwrap();
            assert_eq!(tab.history.undo_depth, usize::from(redo));
            assert_eq!(tab.history.redo_depth, usize::from(!redo));
            assert_eq!(tab.dirty, redo);
        }
        save_and_reopen(
            &mut editor,
            &project,
            source,
            &SceneDocument::from_ordered_roots(records(expected)),
            &manifest,
        );
        close_editor(&mut editor);
    }
}

#[test]
fn key_changes_resolve_in_order_and_a_final_failure_publishes_nothing() {
    for source in [SCENE, PREFAB] {
        let project = Project::new();
        let mut editor = open_project(&project);
        let document = open_source(&mut editor, source);
        select(&mut editor, document, "actor");
        let manifest = manifest(&editor);
        let edits = [
            edit(
                [step("rows"), item("a"), step("id")],
                ComponentValue::String("b".to_owned()),
            ),
            edit(
                [step("rows"), item("b"), step("score")],
                ComponentValue::U64(8),
            ),
            edit(
                [
                    step("labels"),
                    ComponentPropertyStep::MapEntry(MAP_KEY.to_owned()),
                ],
                ComponentValue::String("after".to_owned()),
            ),
        ];
        let expected = settings(10, 20, "b", 8, "after");
        let prepared = editor
            .prepare_authoring_property_edit(document, revision(&editor, document), &edits)
            .unwrap();
        assert_eq!(prepared.patch(), &patch(expected.clone()));
        assert_applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(root(&editor), expected);
        let accepted = editor.current_frame();
        let basis = revision(&editor, document);

        let failing = [
            edit(
                [step("rows"), item("b"), step("id")],
                ComponentValue::String("c".to_owned()),
            ),
            edit(
                [step("rows"), item("c"), step("score")],
                ComponentValue::U64(500),
            ),
            edit(
                [
                    step("labels"),
                    ComponentPropertyStep::MapEntry(MAP_KEY.to_owned()),
                ],
                ComponentValue::String("must not leak".to_owned()),
            ),
            // The old key no longer exists in this private candidate; preceding writes must roll back.
            edit(
                [step("rows"), item("b"), step("score")],
                ComponentValue::U64(900),
            ),
        ];
        let prefix = editor
            .prepare_authoring_property_edit(document, basis, &failing[..3])
            .unwrap();
        assert_eq!(
            prefix.patch(),
            &patch(settings(10, 20, "c", 500, "must not leak"))
        );
        drop(prefix);
        assert!(
            editor
                .prepare_authoring_property_edit(document, basis, &failing)
                .is_err()
        );
        for (component, version) in [(UNKNOWN, 7), (FUTURE, 9)] {
            let unavailable = AuthoringPropertyEdit {
                component: ComponentTypeId::new(component),
                component_version: ComponentSchemaVersion(version),
                property: ComponentPropertyPath::new(ComponentFieldId::new("token")),
                ..edit([], ComponentValue::String("must not overwrite".to_owned()))
            };
            assert!(
                editor
                    .prepare_authoring_property_edit(
                        document,
                        basis,
                        &[
                            edit([step("stats"), step("hp")], ComponentValue::U64(999)),
                            unavailable,
                        ],
                    )
                    .is_err()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().workspace().documents,
            accepted.workspace().documents
        );
        assert_eq!(
            editor.current_frame().workspace().active_scene,
            accepted.workspace().active_scene
        );
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(&manifest)
        );
        assert_eq!(root(&editor), expected);
        assert_eq!(
            fs::read(project.root.join(source)).unwrap(),
            project.original(source)
        );
        save_and_reopen(
            &mut editor,
            &project,
            source,
            &SceneDocument::from_ordered_roots(records(expected)),
            &manifest,
        );
        close_editor(&mut editor);
    }
}

#[test]
fn explicitly_targeted_inactive_scene_and_prefab_keep_tabs_and_selections() {
    let project = Project::new();
    let mut editor = open_project(&project);
    let scene = editor.current_frame().workspace().active_document.unwrap();
    select(&mut editor, scene, "observer");
    let prefab = open_source(&mut editor, PREFAB);
    select(&mut editor, prefab, "observer");

    for (document, active) in [(scene, prefab), (prefab, scene)] {
        assert_eq!(
            editor.current_frame().workspace().active_document,
            Some(active)
        );
        let before = editor.current_frame();
        let edits = [edit([step("stats"), step("hp")], ComponentValue::U64(42))];
        let prepared = editor
            .prepare_authoring_property_edit(document, revision(&editor, document), &edits)
            .unwrap();
        assert_eq!(prepared.patch(), &patch(settings(42, 20, "a", 1, "before")));
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.current_frame().workspace(), before.workspace());
        assert_applied(editor.commit_authoring_edit(prepared));
        editor.drive_editor_frame(Duration::ZERO);
        let after = editor.current_frame();
        assert_eq!(after.workspace().active_document, Some(active));
        assert_eq!(
            after.workspace().active_scene,
            before.workspace().active_scene
        );
        assert_eq!(
            after.workspace().document(active),
            before.workspace().document(active)
        );
        assert_eq!(
            after
                .workspace()
                .documents
                .iter()
                .map(|tab| tab.document)
                .collect::<Vec<_>>(),
            before
                .workspace()
                .documents
                .iter()
                .map(|tab| tab.document)
                .collect::<Vec<_>>()
        );
        let target = after.workspace().document(document).unwrap();
        assert!(!target.active);
        assert!(target.dirty);
        assert_eq!(target.history.undo_depth, 1);

        assert_applied(dispatch(
            &mut editor,
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
        ));
        assert_eq!(
            editor
                .current_frame()
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection
                .top_entity(),
            Some(&id("observer"))
        );
        select(&mut editor, document, "actor");
        assert_eq!(root(&editor), settings(42, 20, "a", 1, "before"));
        manifest(&editor);
        select(&mut editor, document, "observer");
    }
    close_editor(&mut editor);
}

#[test]
fn inactive_source_instance_attach_replace_and_detach_preserve_local_records_and_history() {
    for source in [SCENE, PREFAB] {
        let project = Project::new();
        let mut local = records(initial());
        local[0].children.push(id("observer"));
        let mut expected = SceneDocument::new([id("actor")], local);
        fs::write(project.root.join(source), source_bytes(source, &expected)).unwrap();
        let mut editor = open_project(&project);
        let scene = editor.current_frame().workspace().active_document.unwrap();
        select(&mut editor, scene, "observer");
        let prefab = open_source(&mut editor, PREFAB);
        select(&mut editor, prefab, "observer");
        let (document, active) = if source == SCENE {
            (scene, prefab)
        } else {
            (prefab, scene)
        };

        // No base file exists: this exercises source intent, not nested preview or expansion.
        let first = PrefabInstance {
            source: AssetRef::path("prefabs/not-installed.prefab.json").unwrap(),
            overrides: patch(settings(70, 80, "a", 1, "base override")),
        };
        let replacement = PrefabInstance {
            overrides: patch(settings(71, 80, "a", 1, "replacement override")),
            ..first.clone()
        };
        let mut previous = None;
        for value in [Some(first), Some(replacement), None] {
            assert_applied(dispatch(
                &mut editor,
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
            ));
            select(&mut editor, document, "observer");
            let manifest = manifest(&editor);
            assert_applied(dispatch(
                &mut editor,
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                    document: active,
                }),
            ));
            let before = editor.current_frame();
            let basis = revision(&editor, document);
            let change = instance_patch(previous.clone(), value.clone());
            let inverse = instance_patch(value.clone(), previous.clone());
            let cancelled = editor
                .prepare_authoring_edit(document, basis, &change)
                .unwrap();
            assert_eq!(cancelled.patch(), &change);
            assert_eq!(cancelled.inverse(), &inverse);
            drop(cancelled);
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.current_frame().workspace(), before.workspace());

            let prepared = editor
                .prepare_authoring_edit(document, basis, &change)
                .unwrap();
            assert_eq!(prepared.document(), document);
            assert_eq!(
                prepared.document_kind(),
                if source == SCENE {
                    EditorDocumentKind::Scene
                } else {
                    EditorDocumentKind::Prefab
                }
            );
            assert_eq!(prepared.base_revision(), basis);
            assert_eq!(prepared.patch(), &change);
            assert_eq!(prepared.inverse(), &inverse);
            assert_applied(editor.commit_authoring_edit(prepared));
            editor.drive_editor_frame(Duration::ZERO);
            let committed = editor.current_frame();
            assert_eq!(committed.workspace().active_document, Some(active));
            assert_eq!(
                committed.workspace().active_scene,
                before.workspace().active_scene
            );
            assert_eq!(
                committed.workspace().document(active),
                before.workspace().document(active)
            );
            assert_eq!(
                committed.workspace().document(document).unwrap().history.undo_depth,
                1
            );
            for redo in [false, true] {
                let base_revision = revision(&editor, document);
                let mutation = if redo {
                    EditorDocumentMutation::Redo {
                        document,
                        base_revision,
                    }
                } else {
                    EditorDocumentMutation::Undo {
                        document,
                        base_revision,
                    }
                };
                assert_applied(dispatch(&mut editor, EditorAction::Mutate(mutation)));
                // Observe the exact declaration restored by history without publishing a probe.
                let probe = if redo { &inverse } else { &change };
                drop(
                    editor
                        .prepare_authoring_edit(document, revision(&editor, document), probe)
                        .unwrap(),
                );
                let frame = editor.current_frame();
                assert_eq!(frame.workspace().active_document, Some(active));
                assert_eq!(
                    frame.workspace().active_scene,
                    before.workspace().active_scene
                );
                assert_eq!(
                    frame.workspace().document(active),
                    before.workspace().document(active)
                );
                assert_eq!(
                    frame.workspace().documents.iter().map(|tab| tab.document).collect::<Vec<_>>(),
                    before.workspace().documents.iter().map(|tab| tab.document).collect::<Vec<_>>()
                );
                let target = frame.workspace().document(document).unwrap();
                assert_eq!(target.history.undo_depth, usize::from(redo));
                assert_eq!(target.history.redo_depth, usize::from(!redo));
                assert_eq!(target.dirty, redo);
            }
            assert_eq!(
                fs::read(project.root.join(source)).unwrap(),
                source_bytes(source, &expected)
            );
            expected
                .entities
                .iter_mut()
                .find(|row| row.id == id("actor"))
                .unwrap()
                .prefab = value.clone();
            assert_applied(dispatch(
                &mut editor,
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document }),
            ));
            assert_eq!(
                editor
                    .current_frame()
                    .workspace()
                    .active_scene
                    .as_ref()
                    .unwrap()
                    .selection
                    .top_entity(),
                Some(&id("observer"))
            );
            // Compare complete source data: detachment must not synthesize converted records.
            save_and_reopen(&mut editor, &project, source, &expected, &manifest);
            drop(
                editor
                    .prepare_authoring_edit(document, revision(&editor, document), &inverse)
                    .unwrap(),
            );
            previous = value;
        }
        close_editor(&mut editor);
    }
}

#[test]
fn exact_instance_mismatch_is_atomic_and_unknown_override_records_cannot_be_detached() {
    let project = Project::new();
    let mut local = records(initial());
    let declaration = PrefabInstance {
        source: AssetRef::path("prefabs/not-installed.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: id("base-node"),
            component: ComponentTypeId::new(UNKNOWN),
            value: local[0].components[&ComponentTypeId::new(UNKNOWN)].clone(),
        }]),
    };
    local[0].prefab = Some(declaration.clone());
    let mut expected = SceneDocument::from_ordered_roots(local);
    let original = source_bytes(PREFAB, &expected);
    fs::write(project.root.join(PREFAB), &original).unwrap();
    let mut editor = open_project(&project);
    let scene = editor.current_frame().workspace().active_document.unwrap();
    select(&mut editor, scene, "observer");
    let prefab = open_source(&mut editor, PREFAB);
    select(&mut editor, prefab, "observer");
    let manifest = editor
        .active_authoring_unavailable_record_manifest()
        .unwrap()
        .clone();
    assert_eq!(manifest.total_count(), 3);
    assert!(manifest.entries().iter().any(|entry| matches!(
        entry,
        UnavailableRecordManifestEntry::Record(record)
            if record.component().as_str() == UNKNOWN
                && matches!(record.location(), AuthoringRecordLocation::PatchComponent { anchor, .. } if anchor == &id("actor"))
    )));
    assert_applied(dispatch(
        &mut editor,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: scene }),
    ));
    let before = editor.current_frame();
    let basis = revision(&editor, prefab);
    let changed = settings(42, 20, "a", 1, "before");
    let prefix = patch(changed.clone());
    drop(
        editor
            .prepare_authoring_edit(prefab, basis, &prefix)
            .unwrap(),
    );
    let wrong = PrefabInstance {
        overrides: ScenePatchDocument::default(),
        ..declaration.clone()
    };
    assert_eq!(wrong.source, declaration.source);
    assert_ne!(wrong.overrides, declaration.overrides);
    let mut failing = prefix.clone();
    failing.operations.extend(
        instance_patch(Some(wrong.clone()), Some(declaration.clone())).operations,
    );
    let rejection = editor
        .prepare_authoring_edit(prefab, basis, &failing)
        .unwrap_err();
    let EditorEditPreparationError::Document(report) = rejection else {
        panic!("{rejection:?}")
    };
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-prefab-instance-context-mismatch"
    }));
    for value in [None, Some(wrong)] {
        let rejection = editor
            .prepare_authoring_edit(
                prefab,
                basis,
                &instance_patch(Some(declaration.clone()), value),
            )
            .unwrap_err();
        let EditorEditPreparationError::Document(report) = rejection else {
            panic!("{rejection:?}")
        };
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-structure-unavailable"
        }));
    }
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.current_frame().workspace(), before.workspace());
    assert_eq!(fs::read(project.root.join(PREFAB)).unwrap(), original);

    let prepared = editor
        .prepare_authoring_edit(prefab, basis, &prefix)
        .unwrap();
    assert_eq!(prepared.inverse(), &patch(initial()));
    assert_applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    let committed = editor.current_frame();
    assert_eq!(committed.workspace().active_document, Some(scene));
    assert_eq!(
        committed.workspace().active_scene,
        before.workspace().active_scene
    );
    assert_eq!(
        committed.workspace().document(scene),
        before.workspace().document(scene)
    );
    assert_eq!(
        committed.workspace().document(prefab).unwrap().history.undo_depth,
        1
    );
    assert_applied(dispatch(
        &mut editor,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: prefab }),
    ));
    expected
        .entities
        .iter_mut()
        .find(|row| row.id == id("actor"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value = ComponentValue::map([("payload", changed)]);
    save_and_reopen(&mut editor, &project, PREFAB, &expected, &manifest);
    close_editor(&mut editor);
}

fn revision(editor: &EditorProjectSession, document: EditorDocumentId) -> SceneAuthoringRevision {
    editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .revision
}

fn root(editor: &EditorProjectSession) -> ComponentValue {
    editor
        .current_frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "settings")
        .unwrap()
        .value
        .clone()
        .unwrap()
}

fn manifest(editor: &EditorProjectSession) -> UnavailableRecordManifest {
    let manifest = editor
        .active_authoring_unavailable_record_manifest()
        .unwrap()
        .clone();
    assert_eq!(manifest.total_count(), 2);
    for (component, readiness) in [
        (UNKNOWN, AuthoringRecordReadiness::UnknownSchema),
        (FUTURE, AuthoringRecordReadiness::FutureSchema),
    ] {
        assert!(manifest.entries().iter().any(|entry| matches!(
            entry,
            UnavailableRecordManifestEntry::Record(record)
                if record.component().as_str() == component && record.readiness() == readiness
        )));
    }
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    assert_eq!(
        frame
            .workspace()
            .document(document)
            .unwrap()
            .authoring_readiness,
        Some(AuthoringDocumentReadiness::Degraded)
    );
    manifest
}

fn assert_applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(outcome, EditorActionOutcome::Workspace(ref report) if report.applied),
        "{outcome:?}"
    );
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn select(editor: &mut EditorProjectSession, document: EditorDocumentId, entity: &str) {
    assert_applied(dispatch(
        editor,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document,
            entity: Some(id(entity)),
        }),
    ));
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "public Editor operation did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_project(project: &Project) -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new();
    let mut open = host.begin(project.capability(), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(open) {
            EditorProjectOpenProgress::Opened(editor) => return editor,
            EditorProjectOpenProgress::Pending(next) => open = next,
            EditorProjectOpenProgress::Rejected(failure) => panic!("{:?}", failure.diagnostics()),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_source(editor: &mut EditorProjectSession, source: &str) -> EditorDocumentId {
    if source == SCENE {
        return editor.current_frame().workspace().active_document.unwrap();
    }
    wait(editor, |editor| {
        !editor.current_frame().project_browser().has_pending_work()
    });
    let opening = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: source.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = opening
    else {
        panic!("{opening:?}");
    };
    wait(editor, |editor| {
        editor.current_frame().document_open().result().is_some()
    });
    let frame = editor.current_frame();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = frame.document_open().result().unwrap()
    else {
        panic!("{:?}", frame.document_open());
    };
    assert_eq!(completed, request);
    let acknowledged = dispatch(
        editor,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
    );
    assert!(matches!(
        acknowledged,
        EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged {
            request: acknowledged,
        }) if acknowledged == request
    ));
    document
}

fn save_and_reopen(
    editor: &mut EditorProjectSession,
    project: &Project,
    source: &str,
    expected: &SceneDocument,
    manifest: &UnavailableRecordManifest,
) {
    let document = editor.current_frame().workspace().active_document.unwrap();
    let base_revision = revision(editor, document);
    let save = dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision,
        }),
    );
    assert!(matches!(
        save,
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: saved,
            revision: saved_revision,
            ..
        }) if saved == document && saved_revision == base_revision
    ));
    let bytes = fs::read(project.root.join(source)).unwrap();
    let actual = if source == SCENE {
        SceneDocumentCandidate::decode_json_bytes(&bytes)
            .unwrap()
            .to_json_string()
            .unwrap()
    } else {
        PrefabDocumentCandidate::decode_json_bytes(&bytes)
            .unwrap()
            .to_json_string()
            .unwrap()
    };
    assert_eq!(actual.as_bytes(), source_bytes(source, expected));
    assert_ne!(bytes, project.original(source));
    assert_eq!(
        editor.active_authoring_unavailable_record_manifest(),
        Some(manifest)
    );
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let target = editor.current_frame().reopen_target();
    let reopen = dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
    );
    assert!(matches!(
        reopen,
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    wait(editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    select(editor, document, "actor");
    let actor = expected.entities.iter().find(|row| row.id == id("actor")).unwrap();
    let expected_root = actor.components[&ComponentTypeId::new(COMPONENT)]
        .value
        .get_path(&ComponentFieldPath::from_fields(["payload"]))
        .unwrap();
    assert_eq!(&root(editor), expected_root);
    assert_eq!(
        editor.active_authoring_unavailable_record_manifest(),
        Some(manifest)
    );
    let frame = editor.current_frame();
    let tab = frame.workspace().document(document).unwrap();
    assert!(!tab.dirty);
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 0);
    assert_ne!(tab.revision, base_revision);
}

fn source_bytes(source: &str, document: &SceneDocument) -> Vec<u8> {
    if source == SCENE {
        document.to_json_string().unwrap().into_bytes()
    } else {
        PrefabDocument::new(document.roots.clone(), document.entities.clone())
            .to_json_string()
            .unwrap()
            .into_bytes()
    }
}

fn close_editor(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-property-authoring-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            concat!(
                "schema_version = 1\n[project]\nsource_universe = '2b67ec37-1dd4-550f-8956-6424db6ffd3b'\nnative_document_identity = 'project_location'\nname = \"Property authoring consumer\"\n",
                "[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n",
                "[startup]\ndefault_scene = \"startup.scene.json\"\n",
                "[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n",
                "[runtime]\npreset = \"local-headless\"\n",
                "[capabilities]\nrequested = [\"runtime-2d\"]\n",
            ),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle().to_json_string().unwrap(),
        )
        .unwrap();
        let project = Self { root };
        for source in [SCENE, PREFAB] {
            fs::write(project.root.join(source), project.original(source)).unwrap();
        }
        project
    }

    fn encoded(&self, source: &str, root: ComponentValue) -> Vec<u8> {
        source_bytes(source, &SceneDocument::from_ordered_roots(records(root)))
    }

    fn original(&self, source: &str) -> Vec<u8> {
        self.encoded(source, initial())
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let directory = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
