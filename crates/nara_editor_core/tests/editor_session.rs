#![cfg(any(unix, windows))]

use std::fs;

use nara_core::{Affine2, AssetId, DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{
    EditorCommand, EditorDocument, EditorError, EditorProject, EditorRegistry, EditorSession,
    HierarchyModel, InspectorModel, InspectorValue, PrefabFieldOverride,
    viewport::{Gizmo2D, GizmoDelta2D, GizmoSelection2D, WorldPoint},
};
use nara_reflect::{
    AssetReferenceDemand, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
};
use nara_scene::{
    authoring::{
        PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneEntity,
        SpriteSource, UnknownComponent, builtin_component_schemas, prefab::PrefabError,
    },
    spatial::Transform2d,
};

fn id(n: u64) -> EntityUuid {
    format!("fc46193e-2e80-4a44-934a-{n:012x}").parse().unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::new(
        "97270f99-fd97-483f-a605-a1e7acaea745"
            .parse::<DocumentId>()
            .unwrap(),
    )
}

fn prefab_asset(number: u64, hint: &str) -> SceneAsset {
    SceneAsset {
        id: AssetId::try_from_uuid(id(number).as_uuid()).unwrap(),
        hint: AssetPath::new(hint).unwrap(),
    }
}

fn prefab_mount(number: u64, asset: AssetId) -> SceneEntity {
    let mut row = SceneEntity::new(id(number), format!("Instance {number}"));
    row.prefab = Some(PrefabInstance::new(asset));
    row
}

fn scene_row(scene: &SceneDocument, entity: EntityUuid) -> &SceneEntity {
    scene.entities.iter().find(|row| row.id == entity).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("night_relay.RunRules")
}

fn field() -> ComponentFieldPath {
    ComponentFieldPath::from_fields(["shield"])
}

fn registry() -> EditorRegistry {
    registry_with(rules_schema(
        ComponentSchemaVersion::ONE,
        ComponentValueKind::U64,
    ))
}

fn rules_schema(version: ComponentSchemaVersion, kind: ComponentValueKind) -> ComponentSchema {
    ComponentSchema::new(component(), "Run Rules", version)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("shield"),
            "Shield",
            field(),
            kind,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn registry_with(schema: ComponentSchema) -> EditorRegistry {
    EditorRegistry::new(
        builtin_component_schemas().unwrap(),
        ComponentSchemaCatalog::from_components([schema]).unwrap(),
    )
    .unwrap()
}

fn actor(n: u64, value: u64) -> SceneEntity {
    let mut row = SceneEntity::new(id(n), format!("Actor {n}"));
    row.components.insert(
        component(),
        SceneComponent::known(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("shield", ComponentValue::U64(value))]),
        ),
    );
    row.set(&Transform2d::from_translation(Vec2::new(n as f32, 0.0)))
        .unwrap();
    row
}

fn edit(value: u64) -> EditorCommand {
    EditorCommand::SetField {
        entities: vec![id(1), id(2)],
        component: component(),
        path: field(),
        value: ComponentValue::U64(value),
    }
}

fn value(session: &EditorSession) -> InspectorValue {
    InspectorModel::for_session(session)
        .components
        .into_iter()
        .find(|entry| entry.id == component())
        .unwrap()
        .fields
        .remove(0)
        .value
}

#[test]
fn gizmo_applies_one_world_space_edit_across_selected_roots() {
    let mut scene = document();
    let mut separate = actor(1, 1);
    let outer = Transform2d {
        translation: Vec2::new(8.0, 0.0),
        rotation: std::f32::consts::FRAC_PI_2,
        scale: Vec2::new(2.0, 1.0),
        ..Default::default()
    };
    separate.parent = Some(id(4));
    separate.set(&Transform2d::from_xy(2.0, 0.0)).unwrap();
    let mut inherited = actor(2, 1);
    inherited.parent = Some(id(5));
    let mut pivot = actor(3, 1);
    pivot.set(&Transform2d::from_xy(10.0, 0.0)).unwrap();
    let mut other_parent = actor(4, 1);
    other_parent.set(&outer).unwrap();
    let mut middle = actor(5, 1);
    middle.parent = Some(id(3));
    scene.entities = vec![separate, inherited, pivot, other_parent, middle];
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry());
    session.selection_mut().replace([id(1), id(2), id(3)]);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::new(10.0, 0.0)), 0.0).unwrap();
    let selection = GizmoSelection2D::capture(&session, gizmo, |entity| {
        Some(if entity == id(1) {
            outer.affine()
        } else {
            Affine2::IDENTITY
        })
    })
    .unwrap()
    .unwrap();
    let transform = |session: &EditorSession, entity| {
        session
            .preview_scene()
            .entities
            .iter()
            .find(|row| row.id == entity)
            .unwrap()
            .typed::<Transform2d>()
            .unwrap()
            .unwrap()
    };
    let assert_position = |session: &EditorSession, entity, expected: Vec2| {
        let local = transform(session, entity).translation;
        let world = if entity == id(1) {
            outer.affine().transform_point2(local)
        } else {
            local
        };
        assert!(
            world.abs_diff_eq(expected, 0.0001),
            "{world:?} vs {expected:?}"
        );
    };

    let gesture = session.begin_gesture().unwrap();
    for distance in [Vec2::new(1.0, 0.0), Vec2::new(4.0, -2.0)] {
        let command = selection
            .command(GizmoDelta2D::Translation(distance))
            .unwrap();
        assert!(session.apply_gesture(&gesture, command).unwrap());
    }
    assert!(session.end_gesture(&gesture));
    assert_position(&session, id(1), Vec2::new(12.0, 2.0));
    assert_position(&session, id(3), Vec2::new(14.0, -2.0));
    assert_eq!(transform(&session, id(2)), Transform2d::from_xy(2.0, 0.0));
    assert!(session.undo().unwrap());
    assert!(!session.can_undo());
    assert_position(&session, id(1), Vec2::new(8.0, 4.0));
    assert_position(&session, id(3), Vec2::new(10.0, 0.0));

    let gesture = session.begin_gesture().unwrap();
    let command = selection
        .command(GizmoDelta2D::Rotation(std::f32::consts::FRAC_PI_2))
        .unwrap();
    assert!(session.apply_gesture(&gesture, command).unwrap());
    assert_position(&session, id(1), Vec2::new(6.0, -2.0));
    assert_position(&session, id(3), Vec2::new(10.0, 0.0));
    assert!((transform(&session, id(1)).rotation - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    assert!(session.cancel_gesture(&gesture));

    let gesture = session.begin_gesture().unwrap();
    let factors = Vec2::new(2.0, 0.5);
    let command = selection.command(GizmoDelta2D::Scale(factors)).unwrap();
    assert!(session.apply_gesture(&gesture, command).unwrap());
    assert_position(&session, id(1), Vec2::new(6.0, 2.0));
    assert_position(&session, id(3), Vec2::new(10.0, 0.0));
    assert!(
        transform(&session, id(1))
            .scale
            .abs_diff_eq(Vec2::new(0.5, 2.0), 0.0001)
    );
    assert!(
        transform(&session, id(3))
            .scale
            .abs_diff_eq(factors, 0.0001)
    );
    assert!(session.cancel_gesture(&gesture));
    assert_position(&session, id(1), Vec2::new(8.0, 4.0));
}

#[test]
fn local_gizmo_ticks_keep_authored_and_preview_tables_and_roll_back_failed_batches() {
    let asset = prefab_asset(104, "prop.prefab.ron");
    let mut source = document();
    source.entities.push(actor(11, 5));
    let mut child = actor(2, 2);
    child.parent = Some(id(1));
    let mut scene = document();
    scene.assets.insert("prop".into(), asset.clone());
    scene.entities = vec![
        actor(1, 1),
        child,
        SceneEntity::new(id(3), "No rules"),
        prefab_mount(4, asset.id),
    ];
    let mut session = EditorSession::new(EditorDocument::new(scene.clone()).unwrap(), registry());
    let rename = |name: &str| EditorCommand::Rename {
        entity: id(2),
        name: name.into(),
    };
    assert!(matches!(
        session.apply(rename("Pending prefab")),
        Err(EditorError::Prefab(PrefabError::Load { asset: missing, .. })) if missing == asset.id
    ));
    assert_eq!(session.document().scene(), &scene);
    session.set_prefabs([(asset.id, source)].into()).unwrap();
    let baseline = session.preview_scene().clone();
    let storage = |session: &EditorSession| {
        (
            session.document().scene().entities.as_ptr(),
            session.preview_scene().entities.as_ptr(),
            scene_row(session.preview_scene(), id(4)).name.as_ptr(),
        )
    };
    let before = storage(&session);
    assert!(
        !session
            .apply(EditorCommand::Batch(vec![
                rename("Temporary name"),
                EditorCommand::Batch(vec![rename("Actor 2")]),
            ]))
            .unwrap()
    );
    assert!(!session.can_undo());

    session.selection_mut().replace([id(2)]);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::ZERO), 0.0).unwrap();
    let selected = GizmoSelection2D::capture(&session, gizmo, |_| Some(Affine2::IDENTITY))
        .unwrap()
        .unwrap();
    let gesture = session.begin_gesture().unwrap();
    for distance in [1.0, 2.0, 3.0] {
        let command = selected
            .command(GizmoDelta2D::Translation(Vec2::X * distance))
            .unwrap();
        assert!(session.apply_gesture(&gesture, command).unwrap());
        assert_eq!(storage(&session), before);
    }
    let committed_tick = session.preview_scene().clone();
    assert!(matches!(
        session.apply_gesture(&gesture, EditorCommand::SetField {
            entities: vec![id(2), id(3)],
            component: component(),
            path: field(),
            value: ComponentValue::U64(9),
        }),
        Err(EditorError::MissingComponent { entity, .. }) if entity == id(3)
    ));
    assert_eq!(session.preview_scene(), &committed_tick);
    let authored = scene_row(session.document().scene(), id(2));
    assert_eq!(authored, scene_row(&committed_tick, id(2)));
    assert!(session.end_gesture(&gesture));
    assert!(session.undo().unwrap());
    assert!(!session.can_undo());
    assert_eq!(session.preview_scene(), &baseline);
    assert!(session.redo().unwrap());
    assert_eq!(session.preview_scene(), &committed_tick);

    let gesture = session.begin_gesture().unwrap();
    assert!(session.apply_gesture(&gesture, rename("Rollback")).unwrap());
    assert!(session.cancel_gesture(&gesture));
    assert_eq!(session.preview_scene(), &committed_tick);
}

#[test]
fn structural_gesture_edits_reenroll_rows_and_keep_one_rollback_boundary() {
    let mut scene = document();
    scene.entities = vec![actor(2, 2), actor(4, 4)];
    let mut session = EditorSession::new(EditorDocument::new(scene.clone()).unwrap(), registry());
    let rename = |entity, name: &str| EditorCommand::Rename {
        entity,
        name: name.into(),
    };
    for cancel in [true, false] {
        let gesture = session.begin_gesture().unwrap();
        assert!(
            session
                .apply_gesture(&gesture, rename(id(4), "First tick"))
                .unwrap()
        );
        assert!(
            session
                .apply_gesture(
                    &gesture,
                    EditorCommand::Batch(vec![
                        EditorCommand::Create(actor(1, 1)),
                        EditorCommand::Reparent {
                            entities: vec![id(2)],
                            parent: Some(id(1))
                        },
                    ])
                )
                .unwrap()
        );
        let storage = session.document().scene().entities.as_ptr();
        assert!(
            session
                .apply_gesture(&gesture, rename(id(1), "New parent"))
                .unwrap()
        );
        assert_eq!(session.document().scene().entities.as_ptr(), storage);
        assert!(matches!(
            session.apply_gesture(
                &gesture,
                EditorCommand::Batch(vec![
                    rename(id(1), "Uncommitted"),
                    EditorCommand::Reparent {
                        entities: vec![id(1)],
                        parent: Some(id(2))
                    },
                ])
            ),
            Err(EditorError::Scene(
                nara_scene::authoring::SceneDocumentError::HierarchyCycle { .. }
            ))
        ));
        assert_eq!(
            scene_row(session.document().scene(), id(1)).name,
            "New parent"
        );
        if cancel {
            assert!(session.cancel_gesture(&gesture));
        } else {
            assert!(session.end_gesture(&gesture));
            assert!(session.undo().unwrap());
        }
        assert_eq!(session.document().scene(), &scene);
        assert!(!session.can_undo());
    }
    assert!(session.redo().unwrap());
    assert_eq!(
        scene_row(session.document().scene(), id(2)).parent,
        Some(id(1))
    );
    assert_eq!(
        scene_row(session.document().scene(), id(4)).name,
        "First tick"
    );
}

#[test]
fn mixed_fields_gestures_and_unknown_components_survive_save_and_reopen() {
    let unknown = ComponentTypeId::new("third_party.CustomComponent");
    let payload = r#"(message: "保留原值", items: [1,  /* keep spacing */ 2])"#;
    let staying = SceneComponent::Unknown(UnknownComponent {
        type_id: unknown.clone(),
        version: ComponentSchemaVersion(27),
        raw: RawRon::from_ron(payload).unwrap(),
    });
    let mut original = document();
    let mut first = actor(1, 2);
    first.components.insert(unknown.clone(), staying.clone());
    original.entities = vec![actor(2, 3), first];

    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("night-relay.scene.ron");
    let registry = registry();
    let schemas = registry.catalog().components();
    original.write_with(&path, schemas).unwrap();
    let mut session =
        EditorSession::new(EditorDocument::open_with(&path, schemas).unwrap(), registry);
    session.selection_mut().replace([id(1), id(2)]);
    assert_eq!(value(&session), InspectorValue::Mixed);
    assert!(!session.is_dirty());

    let gesture = session.begin_gesture().unwrap();
    let first_shield = |number| EditorCommand::SetField {
        entities: vec![id(1)],
        component: component(),
        path: field(),
        value: ComponentValue::U64(number),
    };
    assert!(session.apply_gesture(&gesture, first_shield(9)).unwrap());
    assert!(session.is_dirty());
    assert!(session.apply_gesture(&gesture, first_shield(2)).unwrap());
    assert!(!session.is_dirty());
    assert!(!session.end_gesture(&gesture));
    assert!(!session.can_undo());

    let gesture = session.begin_gesture().unwrap();
    assert!(session.apply_gesture(&gesture, edit(10)).unwrap());
    assert!(matches!(
        session.apply(EditorCommand::Delete(vec![id(1)])),
        Err(EditorError::GestureActive)
    ));
    assert!(session.apply_gesture(&gesture, edit(12)).unwrap());
    assert!(session.end_gesture(&gesture));
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(12))
    );
    assert!(session.undo().unwrap());
    assert!(!session.can_undo());
    assert!(!session.is_dirty());
    assert_eq!(value(&session), InspectorValue::Mixed);
    assert!(session.redo().unwrap());

    let next = session.begin_gesture().unwrap();
    assert!(matches!(
        session.apply_gesture(&gesture, edit(19)),
        Err(EditorError::StaleGesture)
    ));
    assert!(!session.cancel_gesture(&gesture));
    assert!(!session.end_gesture(&gesture));
    session.apply_gesture(&next, edit(20)).unwrap();
    assert!(session.cancel_gesture(&next));
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(12))
    );
    session.save().unwrap();
    assert!(!session.is_dirty());

    let loaded =
        SceneDocument::read_with(&path, session.registry().catalog().components()).unwrap();
    assert_eq!(&loaded, session.document().scene());
    assert_eq!(loaded.entities[0].components[&unknown], staying);
    assert!(fs::read_to_string(&path).unwrap().contains(payload));
    assert_eq!(
        loaded.entities[0]
            .typed::<Transform2d>()
            .unwrap()
            .unwrap()
            .translation,
        Vec2::X
    );
    session
        .apply(EditorCommand::RemoveComponent {
            entities: vec![id(1)],
            component: unknown.clone(),
        })
        .unwrap();
    assert!(
        !session.document().scene().entities[0]
            .components
            .contains_key(&unknown)
    );
    assert!(session.undo().unwrap());
    assert_eq!(
        session.document().scene().entities[0].components[&unknown],
        staying
    );
}

#[test]
fn hierarchy_reparent_delete_and_undo_are_atomic() {
    let mut scene = document();
    let mut child = actor(2, 3);
    child.parent = Some(id(1));
    scene.entities = vec![actor(1, 2), child, actor(3, 4)];
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry());
    let batch = EditorCommand::Batch(vec![
        EditorCommand::Rename {
            entity: id(1),
            name: "changed".into(),
        },
        EditorCommand::Reparent {
            entities: vec![id(1)],
            parent: Some(id(2)),
        },
    ]);
    assert!(session.apply(batch).is_err());
    assert_eq!(session.document().scene().entities[0].name, "Actor 1");
    assert!(!session.can_undo());

    session.selection_mut().replace([id(2), id(3)]);
    session.apply(EditorCommand::Delete(vec![id(1)])).unwrap();
    assert_eq!(session.selection().entities(), &[id(3)]);
    assert_eq!(session.document().scene().entities.len(), 1);
    assert!(session.undo().unwrap());
    let rows = HierarchyModel::default().rows(session.document().scene());
    assert_eq!(
        rows.iter()
            .map(|row| (row.entity, row.depth))
            .collect::<Vec<_>>(),
        [(id(1), 0), (id(2), 1), (id(3), 0)]
    );
    assert!(session.redo().unwrap());
    assert_eq!(session.document().scene().entities.len(), 1);
}

#[test]
fn inherited_root_and_nested_field_edits_revert_and_undo_as_sparse_overrides() {
    let outer = prefab_asset(101, "outer.prefab.ron");
    let inner = prefab_asset(102, "inner.prefab.ron");
    let mut inner_doc = SceneDocument::new(DocumentId::try_from_uuid(id(201).as_uuid()).unwrap());
    let mut leaf = actor(22, 6);
    leaf.parent = Some(id(21));
    inner_doc.entities = vec![actor(21, 4), leaf];
    let mut outer_doc = SceneDocument::new(DocumentId::try_from_uuid(id(202).as_uuid()).unwrap());
    let mut nested = prefab_mount(12, inner.id);
    nested.parent = Some(id(11));
    outer_doc.entities = vec![actor(11, 2), nested];
    outer_doc.assets.insert("inner".into(), inner.clone());

    let mut scene = document();
    scene.entities.push(prefab_mount(1, outer.id));
    scene.assets.insert("outer".into(), outer.clone());
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry());
    session
        .set_prefabs([(outer.id, outer_doc), (inner.id, inner_doc)].into())
        .unwrap();
    let root = id(1);
    let child = session
        .preview_scene()
        .entities
        .iter()
        .find(|row| row.name == "Actor 22")
        .unwrap()
        .id;
    let shield = |session: &EditorSession, entity| {
        scene_row(session.preview_scene(), entity).components[&component()]
            .value()
            .unwrap()
            .get_path(&field())
            .unwrap()
            .as_u64()
            .unwrap()
    };
    assert_eq!([shield(&session, root), shield(&session, child)], [2, 6]);

    assert!(
        session
            .apply(EditorCommand::SetField {
                entities: vec![root, child],
                component: component(),
                path: field(),
                value: ComponentValue::U64(9),
            })
            .unwrap()
    );
    let authored = session.document().scene().clone();
    assert_eq!(authored.entities.len(), 1);
    assert!(authored.entities[0].components.is_empty());
    let [root_edit]: [PrefabFieldOverride; 1] = session.prefab_overrides(root).try_into().unwrap();
    let [child_edit]: [PrefabFieldOverride; 1] =
        session.prefab_overrides(child).try_into().unwrap();
    assert_eq!(root_edit.address, "night_relay.RunRules.shield");
    assert_eq!(
        child_edit.address,
        format!("child:{}/{}/night_relay.RunRules.shield", id(12), id(22))
    );
    assert_eq!([shield(&session, root), shield(&session, child)], [9, 9]);

    assert!(
        session
            .apply(EditorCommand::Batch(vec![
                EditorCommand::RevertPrefabOverride(root_edit),
                EditorCommand::RevertPrefabOverride(child_edit),
            ]))
            .unwrap()
    );
    assert!(session.prefab_overrides(root).is_empty());
    assert!(session.prefab_overrides(child).is_empty());
    assert_eq!([shield(&session, root), shield(&session, child)], [2, 6]);
    assert!(session.undo().unwrap());
    assert_eq!(session.document().scene(), &authored);
    assert_eq!([shield(&session, root), shield(&session, child)], [9, 9]);
    assert!(session.undo().unwrap());
    assert_eq!([shield(&session, root), shield(&session, child)], [2, 6]);
    assert!(!session.can_undo());
}

#[test]
fn applying_an_override_validates_every_instance_before_publishing() {
    let component = ComponentTypeId::new("example.Inventory");
    let field = ComponentFieldPath::from_fields(["slots"]);
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    let schema = ComponentSchema::new(component.clone(), "Inventory", ComponentSchemaVersion::ONE)
        .with_capabilities(capabilities)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("slots"),
            "Slots",
            field.clone(),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        )
        .with_capabilities(capabilities)]);
    let registry = EditorRegistry::new(
        [],
        ComponentSchemaCatalog::from_components([schema]).unwrap(),
    )
    .unwrap();
    let slots = |values: &[u64]| {
        ComponentValue::List(values.iter().copied().map(ComponentValue::U64).collect())
    };
    let asset = prefab_asset(103, "inventory.prefab.ron");
    let mut source = SceneDocument::new(DocumentId::try_from_uuid(id(203).as_uuid()).unwrap());
    let mut root = SceneEntity::new(id(11), "Inventory");
    root.components.insert(
        component.clone(),
        SceneComponent::known(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("slots", slots(&[1, 2]))]),
        ),
    );
    source.entities.push(root);
    let mut other = prefab_mount(2, asset.id);
    let other_address = "example.Inventory.slots[1]";
    other
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .insert(other_address.into(), RawRon::from_ron("20").unwrap());
    let mut scene = document();
    scene.entities = vec![prefab_mount(1, asset.id), other];
    scene.assets.insert("inventory".into(), asset.clone());
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry);
    session
        .set_prefabs([(asset.id, source.clone())].into())
        .unwrap();

    let replace_first = |values: &[u64]| EditorCommand::SetField {
        entities: vec![id(1)],
        component: component.clone(),
        path: field.clone(),
        value: slots(values),
    };
    let read_slots = |scene: &SceneDocument, entity| {
        scene_row(scene, entity).components[&component]
            .value()
            .unwrap()
            .get_path(&field)
            .unwrap()
            .clone()
    };
    assert!(session.apply(replace_first(&[10])).unwrap());
    let too_short = session.document().scene().clone();
    let projection = session.preview_scene().clone();
    let [edit]: [PrefabFieldOverride; 1] = session.prefab_overrides(id(1)).try_into().unwrap();
    assert!(matches!(
        session.apply_to_prefab(&edit, |_, _, _| -> Result<(), &str> { panic!("invalid projection reached publisher") }),
        Err(EditorError::Prefab(PrefabError::Override { instance, address, .. }))
            if instance == id(2) && address == other_address
    ));
    assert_eq!(session.document().scene(), &too_short);
    assert_eq!(session.preview_scene(), &projection);

    assert!(session.apply(replace_first(&[10, 11])).unwrap());
    let before = session.document().scene().clone();
    let projection = session.preview_scene().clone();
    let [edit]: [PrefabFieldOverride; 1] = session.prefab_overrides(id(1)).try_into().unwrap();
    let check_publication = |published, expected: &SceneDocument, updated: &SceneDocument| {
        assert_eq!(published, asset.id);
        assert_eq!(expected, &source);
        assert_eq!(read_slots(updated, id(11)), slots(&[10, 11]));
    };
    assert!(matches!(
        session.apply_to_prefab(&edit, |id, expected, updated| {
            check_publication(id, expected, updated);
            Err("fixture write failed")
        }),
        Err(EditorError::PublishPrefab { asset: failed, .. }) if failed == asset.id
    ));
    assert_eq!(session.document().scene(), &before);
    assert_eq!(session.preview_scene(), &projection);

    assert!(
        session
            .apply_to_prefab(&edit, |id, expected, updated| {
                check_publication(id, expected, updated);
                Ok::<_, &str>(())
            })
            .unwrap()
    );
    assert!(session.prefab_overrides(id(1)).is_empty());
    assert_eq!(session.prefab_overrides(id(2))[0].address, other_address);
    assert_eq!(read_slots(session.preview_scene(), id(1)), slots(&[10, 11]));
    assert_eq!(read_slots(session.preview_scene(), id(2)), slots(&[10, 20]));
    assert!(session.undo().unwrap());
    assert_eq!(session.document().scene(), &before);
    assert!(session.undo().unwrap());
    assert_eq!(session.document().scene(), &too_short);
}

#[test]
fn an_incompatible_cached_schema_does_not_block_opening_or_saving() {
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("mismatch.scene.ron");
    let payload = r#"(shield: /* unreleased type */ "unknown")"#;
    let text = format!(
        "Scene(version: 1, id: \"{}\", entities: [(id: \"{}\", name: \"Actor\", components: {{\"{}\": {payload}}})])",
        document().id,
        id(1),
        component()
    );
    fs::write(&path, text).unwrap();
    let registry = registry();
    let schemas = registry.catalog().components();
    assert!(SceneDocument::read_with(&path, schemas).is_err());
    let loaded = EditorDocument::open_with(&path, schemas).unwrap();
    assert!(matches!(
        &loaded.scene().entities[0].components[&component()],
        SceneComponent::Unknown(unknown) if unknown.raw.as_ron() == payload
    ));
    let mut session = EditorSession::new(loaded, registry);
    session
        .apply(EditorCommand::Rename {
            entity: id(1),
            name: "Edited name".to_owned(),
        })
        .unwrap();
    session.save().unwrap();
    assert!(fs::read_to_string(path).unwrap().contains(payload));
}

#[test]
fn schema_refresh_preserves_an_edited_previous_version_and_its_history() {
    let original_registry = registry();
    let schemas = original_registry.catalog().components();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("refresh.scene.ron");
    let future = ComponentTypeId::new("third_party.CustomComponent");
    let payload = "/* retain bytes */ Future(0x0a, [1,  2])";
    let mut original = document();
    original.entities = vec![actor(1, 1), actor(2, 2)];
    original.entities[0].components.insert(
        future.clone(),
        SceneComponent::Unknown(UnknownComponent {
            type_id: future.clone(),
            version: ComponentSchemaVersion(27),
            raw: RawRon::from_ron(payload).unwrap(),
        }),
    );
    original.write_with(&path, schemas).unwrap();
    let mut session = EditorSession::new(
        EditorDocument::open_with(&path, schemas).unwrap(),
        original_registry,
    );
    assert!(session.apply(edit(3)).unwrap());
    assert!(session.apply(edit(4)).unwrap());
    assert!(session.undo().unwrap());

    let upgraded = registry_with(rules_schema(
        ComponentSchemaVersion(2),
        ComponentValueKind::String,
    ));
    assert!(session.set_registry(upgraded).unwrap().is_empty());
    assert!(session.can_undo() && session.can_redo() && session.is_dirty());
    let assert_payload = |scene: &SceneDocument, shield| {
        let row = scene_row(scene, id(1));
        assert!(matches!(
            &row.components[&component()],
            SceneComponent::Unknown(unknown)
                if unknown.version == ComponentSchemaVersion::ONE
                    && unknown.raw.as_ron() == format!("(shield: {shield})")
        ));
        assert_eq!(
            row.components[&future],
            original.entities[0].components[&future]
        );
    };
    assert_payload(session.document().scene(), 3);
    assert!(session.redo().unwrap());
    assert_payload(session.document().scene(), 4);
    assert!(
        session
            .apply(EditorCommand::Rename {
                entity: id(2),
                name: "Renamed actor".into(),
            })
            .unwrap()
    );
    session.save().unwrap();
    let reopened =
        SceneDocument::read_with(&path, session.registry().catalog().components()).unwrap();
    assert_eq!(scene_row(&reopened, id(2)).name, "Renamed actor");
    assert_payload(&reopened, 4);
    assert!(session.undo().unwrap());
    assert!(session.undo().unwrap());
    assert_payload(session.document().scene(), 3);
    session.save().unwrap();
    assert_payload(&SceneDocument::read(&path).unwrap(), 3);

    assert!(session.set_registry(registry()).unwrap().is_empty());
    session.selection_mut().replace([id(1), id(2)]);
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(3))
    );
    assert!(!session.is_dirty());
    assert!(session.undo().unwrap());
    assert_eq!(value(&session), InspectorValue::Mixed);
    assert!(session.redo().unwrap());
    assert!(session.redo().unwrap());
    assert_eq!(
        value(&session),
        InspectorValue::Shared(ComponentValue::U64(4))
    );
}

#[test]
fn same_version_refresh_preserves_old_defaults_and_historical_asset_aliases() {
    let version = ComponentSchemaVersion::ONE;
    let mut old = rules_schema(version, ComponentValueKind::U64);
    old.fields[0].default_value = Some(ComponentValue::U64(1));
    let image_field = ComponentFieldPath::from_fields(["image"]);
    old = old.with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("image"),
        "Image",
        image_field.clone(),
        ComponentValueShape::stable_asset_ref(AssetReferenceDemand::Immediate),
    )
    .with_reference_capabilities()]);
    let image = prefab_asset(87, "cell.png");
    let mut row = actor(1, 1);
    row.components
        .get_mut(&component())
        .unwrap()
        .value_mut()
        .unwrap()
        .set_path(&image_field, ComponentValue::AssetReference(image.id))
        .unwrap();
    let mut scene = document();
    scene.entities.push(row);
    let mut session = EditorSession::new(
        EditorDocument::new(scene).unwrap(),
        registry_with(old.clone()),
    );
    assert!(
        session
            .apply(EditorCommand::Batch(vec![
                EditorCommand::DeclareAsset(image.clone()),
                EditorCommand::Rename {
                    entity: id(1),
                    name: "Actor with an alias".into()
                },
            ]))
            .unwrap()
    );
    assert!(session.undo().unwrap());

    let mut next = rules_schema(version, ComponentValueKind::String);
    next.fields[0].default_value = Some(ComponentValue::String("new default".into()));
    assert_eq!(session.set_registry(registry_with(next)).unwrap().len(), 1);
    let assert_payload = |session: &EditorSession, image: &str| {
        let SceneComponent::Unknown(unknown) =
            &scene_row(session.document().scene(), id(1)).components[&component()]
        else {
            panic!("the incompatible component should be opaque");
        };
        assert_eq!(unknown.version, version);
        assert_eq!(
            unknown.raw.as_ron(),
            format!(r#"(shield: 1, image: "{image}")"#)
        );
    };
    assert_payload(&session, &image.id.to_string());
    assert!(session.redo().unwrap());
    assert_payload(&session, "@cell");
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("shape-change.scene.ron");
    session.save_as(&path).unwrap();
    let reopened =
        EditorDocument::open_with(&path, session.registry().catalog().components()).unwrap();
    assert_eq!(reopened.scene(), session.document().scene());
    assert!(session.undo().unwrap());
    assert_payload(&session, &image.id.to_string());

    assert!(session.set_registry(registry_with(old)).unwrap().is_empty());
    let assert_known = |session: &EditorSession| {
        let value = scene_row(session.document().scene(), id(1)).components[&component()]
            .value()
            .unwrap();
        assert_eq!(value.get_path(&field()).unwrap(), &ComponentValue::U64(1));
        assert_eq!(
            value.get_path(&image_field).unwrap(),
            &ComponentValue::AssetReference(image.id)
        );
    };
    assert_known(&session);
    assert!(session.redo().unwrap());
    assert_known(&session);
}

#[test]
fn project_settings_and_asset_documents_round_trip_on_the_same_mount() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::write(
        root.join("nara.toml"),
        "[project]\nname = 'Night Relay'\nversion = '0.1'\n",
    )
    .unwrap();
    fs::create_dir(root.join("assets")).unwrap();
    let path = AssetPath::new("night-relay.scene.ron").unwrap();
    document()
        .write(root.join("assets").join(path.as_str()))
        .unwrap();

    let mut project = EditorProject::open(root).unwrap();
    assert_eq!(project.assets().entries()[0].path, path);
    project.set_entry_scene(Some(path.clone())).unwrap();
    let reopened = EditorProject::open(root).unwrap();
    assert_eq!(reopened.entry_scene(), Some(&path));
    assert_eq!(reopened.open_document(&path).unwrap().scene(), &document());
}

#[test]
fn saving_after_moving_a_png_and_its_sidecar_updates_the_hint_and_keeps_the_id() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(
        root.join("nara.toml"),
        "[project]\nname = 'Asset Move'\nversion = '0.1'\n",
    )
    .unwrap();
    let assets = root.join("assets");
    fs::create_dir_all(assets.join("sprites")).unwrap();
    let old = AssetPath::new("power-cell.png").unwrap();
    let new = AssetPath::new("sprites/charge.png").unwrap();
    fs::write(
        assets.join(old.as_str()),
        include_bytes!("../../../reference-game/assets/power-cell.png"),
    )
    .unwrap();
    let mut project = EditorProject::open(root).unwrap();
    let image = project.asset_reference(&old).unwrap();
    let old_meta = assets.join("power-cell.png.meta");
    let new_meta = assets.join("sprites/charge.png.meta");
    let sidecar = fs::read(&old_meta).unwrap();

    let mut scene = document();
    scene.assets.insert("cell".into(), image.clone());
    let mut sprite = SceneEntity::new(id(1), "Charge");
    sprite
        .set(&SpriteSource {
            image: Some(image.id),
            ..Default::default()
        })
        .unwrap();
    scene.entities.push(sprite);
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry());
    let path = assets.join("charge.scene.ron");
    session.save_as(&path).unwrap();

    fs::rename(assets.join(old.as_str()), assets.join(new.as_str())).unwrap();
    fs::rename(old_meta, &new_meta).unwrap();
    project.refresh_assets().unwrap();
    assert_eq!(project.asset_reference(&old), None);
    assert_eq!(project.asset_reference(&new).unwrap().id, image.id);
    assert_eq!(project.asset_hint(image.id).as_ref(), Some(&new));
    assert_eq!(fs::read(new_meta).unwrap(), sidecar);

    session.refresh_asset_hints(|id| project.asset_hint(id));
    session.save().unwrap();
    let saved = SceneDocument::read_with(&path, session.registry().catalog().components()).unwrap();
    assert_eq!(
        saved.assets["cell"],
        SceneAsset {
            id: image.id,
            hint: new
        }
    );
    assert_eq!(
        saved.entities[0]
            .typed::<SpriteSource>()
            .unwrap()
            .unwrap()
            .image,
        Some(image.id)
    );
}
