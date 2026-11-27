use std::{collections::BTreeMap, rc::Rc};

use nara_core::{AssetId, Color, DocumentId, EntityUuid};
use nara_editor_core::{
    EditorCommand, EditorDocument, EditorRegistry, EditorSession, InspectorValue, field_draft,
    value_draft,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldEditorHint, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, Schema,
};
use nara_scene::authoring::{
    RawRon, SceneComponent, SceneDocument, SceneEntity, SpriteSource, UnknownComponent,
    builtin_component_schemas,
};
use open_gpui::Rgba;

use super::color::ColorSeed;
use super::target::{
    AddComponent, CHANGED, FieldKey, FieldTarget, SELECTION_CHANGED, Selection, agreement,
    field_value, project,
};

const NIGHT_RELAY: &str = include_str!("../../../../reference-game/assets/night-relay.scene.ron");
const POWER_CELL: &str = include_str!("../../../../reference-game/assets/power-cell.prefab.ron");

fn relay_prefabs(scene: &SceneDocument) -> BTreeMap<AssetId, SceneDocument> {
    [(
        scene.assets["power_cell"].id,
        SceneDocument::parse(POWER_CELL).unwrap(),
    )]
    .into()
}

fn relay_session() -> EditorSession {
    let scene = SceneDocument::parse(NIGHT_RELAY).unwrap();
    let prefabs = relay_prefabs(&scene);
    let document = EditorDocument::new(scene).unwrap();
    let registry =
        EditorRegistry::new(builtin_component_schemas().unwrap(), Default::default()).unwrap();
    let mut session = EditorSession::new(document, registry);
    session.set_prefabs(prefabs).unwrap();
    session
}

fn bind(
    session: &EditorSession,
    component: &str,
    field: &str,
    member: &[&str],
) -> (FieldTarget, Vec<Option<ComponentValue>>) {
    let component = ComponentTypeId::new(component);
    let schema = session.registry().schema(&component).unwrap();
    let path = ComponentFieldPath::from_fields([field]);
    let field = schema
        .fields()
        .iter()
        .find(|field| *field.path() == path)
        .unwrap();
    let field = Rc::new(field.clone());
    let mut shape = field.shape();
    for name in member {
        let ComponentValueShape::Record { members } = shape else {
            panic!("expected a record")
        };
        shape = &members
            .iter()
            .find(|member| &member.name == name)
            .unwrap()
            .shape;
    }
    let target = FieldTarget {
        selection: Rc::new(Selection::new(session)),
        version: schema.version(),
        key: FieldKey {
            component,
            path,
            member: ComponentFieldPath::from_fields(member.iter().copied()),
        },
        shape: shape.clone(),
        field,
    };
    let values = bound_values(session, &target);
    (target, values)
}

fn bound_values(session: &EditorSession, target: &FieldTarget) -> Vec<Option<ComponentValue>> {
    target
        .selection
        .rows(session)
        .unwrap()
        .iter()
        .map(|row| {
            let record = &row.components[&target.key.component];
            project(field_value(record, &target.field), &target.key.member).cloned()
        })
        .collect()
}

fn select_relay(session: &mut EditorSession, names: &[&str]) -> Vec<EntityUuid> {
    let selected: Vec<_> = names
        .iter()
        .map(|name| {
            session
                .preview_scene()
                .entities
                .iter()
                .find(|row| &row.name == name)
                .unwrap()
                .id
        })
        .collect();
    session.selection_mut().replace(selected.iter().copied());
    selected
}

#[test]
fn night_relay_tints_decode_with_the_catalog_color_hint() {
    let session = relay_session();
    let schema = SpriteSource::type_schema().unwrap();
    let tint = schema
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "tint")
        .unwrap();
    assert_eq!(
        tint.editor_hint(),
        Some(ComponentFieldEditorHint::LinearRgba)
    );
    let catalog = ComponentSchemaCatalog::from_components([schema.clone()]).unwrap();
    let encoded = catalog.to_json_string().unwrap();
    assert!(encoded.contains("\"linear_rgba\""));
    let recovered = ComponentSchemaCatalog::from_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(recovered, catalog);
    let mut without_hint = schema;
    without_hint
        .fields
        .iter_mut()
        .find(|field| field.id().as_str() == "tint")
        .unwrap()
        .editor_hint = None;
    assert_ne!(
        catalog.fingerprint(),
        ComponentSchemaCatalog::from_components([without_hint])
            .unwrap()
            .fingerprint()
    );

    let sprites: Vec<_> = session
        .preview_scene()
        .entities
        .iter()
        .filter_map(|row| {
            row.typed::<SpriteSource>()
                .unwrap()
                .map(|sprite| (&row.name, sprite.tint()))
        })
        .collect();
    assert_eq!(sprites.len(), 20);
    assert_eq!(
        sprites
            .iter()
            .find(|(name, _)| name.as_str() == "Courier")
            .unwrap()
            .1,
        Color::rgba(0.65, 0.9, 1., 1.)
    );
}

#[test]
fn catalog_rejects_a_color_hint_on_a_non_rgba_field() {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("text"),
        "Text",
        ComponentFieldPath::from_fields(["text"]),
        ComponentValueKind::String,
    )
    .with_editor_hint(ComponentFieldEditorHint::LinearRgba);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.BadColor"),
        "BadColor",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field]);
    assert!(matches!(
        ComponentSchemaCatalog::from_components([schema]),
        Err(ComponentRegistryError::InvalidFieldEditorHint { .. })
    ));
}

#[test]
fn srgb_picker_preserves_unedited_channels_and_excludes_hdr() {
    let sprite = SpriteSource {
        tint: Color::rgba(0.18, 0.003, 0.72, 0.37),
        ..Default::default()
    };
    let value = sprite
        .encode_component()
        .unwrap()
        .get("tint")
        .unwrap()
        .clone();
    let seed = ColorSeed::admit(&[Some(value.clone())], Some(&value)).unwrap();
    let display = seed.display();
    assert!((display.r - 0.461_356_13).abs() < 1e-7);
    assert!((display.g - 0.038_76).abs() < 1e-7);
    assert_eq!(seed.value(display).unwrap(), value);
    let adjusted = seed.value(Rgba { r: 0.5, ..display }).unwrap();
    assert!((adjusted.get("r").unwrap().as_f64().unwrap() - 0.214_041_140_48).abs() < 1e-10);
    for unchanged in ["g", "b", "a"] {
        assert_eq!(adjusted.get(unchanged), value.get(unchanged));
    }

    let hdr = SpriteSource {
        tint: Color::rgba(1.5, -0.2, 0.2, 0.4),
        ..Default::default()
    };
    let mut entity = SceneEntity::new(
        "4f99b200-2a5e-4872-8d3d-000000000001".parse().unwrap(),
        "HDR",
    );
    entity.set(&hdr).unwrap();
    assert_eq!(entity.typed::<SpriteSource>().unwrap(), Some(hdr));
    let authored = entity.components[&ComponentTypeId::new(SpriteSource::COMPONENT_ID)]
        .value()
        .unwrap();
    assert!(ColorSeed::admit(&[authored.get("tint").cloned()], authored.get("tint")).is_none());
}

#[test]
fn applying_a_mixed_night_relay_tint_is_one_undoable_edit() {
    let mut session = relay_session();
    select_relay(&mut session, &["Courier", "Power cell A"]);
    let (target, before) = bind(&session, SpriteSource::COMPONENT_ID, "tint", &[]);
    assert_eq!(agreement(&before), InspectorValue::Mixed);
    let seed = ColorSeed::admit(&before, None).unwrap();
    let neutral = Rgba {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.,
    };
    assert_eq!(seed.display(), neutral);
    assert_eq!(bound_values(&session, &target), before);
    let chosen = seed
        .value(Rgba {
            r: 1.,
            g: 0.,
            b: 0.,
            a: 0.25,
        })
        .unwrap();
    let command = target
        .command(&session, &before, chosen.clone())
        .unwrap()
        .unwrap();
    assert!(session.apply(command).unwrap());
    assert_eq!(
        bound_values(&session, &target),
        [Some(chosen.clone()), Some(chosen)]
    );
    assert!(session.undo().unwrap());
    assert_eq!(bound_values(&session, &target), before);
    assert!(!session.can_undo());
}

#[test]
fn a_mixed_night_relay_vector_edit_changes_only_its_axis_and_undoes_as_one_action() {
    let mut session = relay_session();
    select_relay(&mut session, &["Courier", "Power cell C"]);
    let (x_target, before_x) = bind(&session, "nara.Transform2d", "translation", &["x"]);
    let (y_target, before_y) = bind(&session, "nara.Transform2d", "translation", &["y"]);
    assert_eq!(agreement(&before_x), InspectorValue::Mixed);
    assert_eq!(agreement(&before_y), InspectorValue::Mixed);

    let next = ComponentValue::f64(42.5).unwrap();
    let command = x_target
        .command(&session, &before_x, next.clone())
        .unwrap()
        .unwrap();
    assert!(session.apply(command).unwrap());
    assert_eq!(
        bound_values(&session, &x_target),
        [Some(next.clone()), Some(next)]
    );
    assert_eq!(bound_values(&session, &y_target), before_y);
    assert!(session.undo().unwrap());
    assert!(!session.can_undo());
    assert_eq!(bound_values(&session, &x_target), before_x);
}

#[test]
fn stale_drafts_detect_rebound_entities_and_external_value_changes() {
    let mut session = relay_session();
    select_relay(&mut session, &["Courier"]);
    let (target, expected) = bind(&session, "nara.Transform2d", "translation", &["x"]);
    let proposed = ComponentValue::f64(6.).unwrap();
    select_relay(&mut session, &["Power cell A"]);
    assert_eq!(
        target
            .command(&session, &expected, proposed.clone())
            .unwrap_err(),
        SELECTION_CHANGED
    );
    select_relay(&mut session, &["Courier"]);

    let new_live = ComponentValue::f64(10.).unwrap();
    let command = target
        .command(&session, &expected, new_live.clone())
        .unwrap()
        .unwrap();
    assert!(session.apply(command).unwrap());
    assert_eq!(
        target.command(&session, &expected, proposed).unwrap_err(),
        CHANGED
    );
    assert_eq!(bound_values(&session, &target), [Some(new_live)]);
}

#[test]
fn a_rename_keeps_committed_cjk_text_and_stays_bound_to_the_original_document() {
    let mut session = relay_session();
    let selected = select_relay(&mut session, &["Courier"]);
    let selection = Selection::new(&session);
    let command = selection
        .rename_command(&session, "Courier", "夜航配達員".into())
        .unwrap()
        .unwrap();
    assert!(session.apply(command).unwrap());
    assert_eq!(selection.rows(&session).unwrap()[0].name, "夜航配達員");
    assert!(session.undo().unwrap());

    let mut other = session.document().scene().clone();
    other.id = "681a78dc-b6ab-4f3a-8eb6-4742d2ced238".parse().unwrap();
    let prefabs = relay_prefabs(&other);
    session
        .open_document_with_prefabs(EditorDocument::new(other).unwrap(), prefabs)
        .unwrap();
    session.selection_mut().replace(selected);
    assert_eq!(
        selection
            .rename_command(&session, "Courier", "別名".into())
            .unwrap_err(),
        SELECTION_CHANGED
    );
}

#[test]
fn add_component_uses_schema_initial_values_for_missing_targets_only() {
    let mut session = relay_session();
    let selected = select_relay(&mut session, &["Courier", "Power cell C"]);
    let component = ComponentTypeId::new("nara.Visibility");
    let authored = SceneComponent::known(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("mode", ComponentValue::String("hidden".into()))]),
    );
    let set = EditorCommand::SetComponent {
        entities: vec![selected[0]],
        component: component.clone(),
        value: authored.clone(),
    };
    assert!(session.apply(set).unwrap());
    let selection = Selection::new(&session);
    let options = AddComponent::options(&session, &selection.rows(&session).unwrap());
    let option = options
        .iter()
        .find(|option| option.id == component)
        .unwrap();
    assert_eq!(option.label, "Visibility · 1 missing");
    let command = option.command(&session, &selection).unwrap();
    assert!(session.apply(command).unwrap());
    let rows = selection.rows(&session).unwrap();
    assert_eq!(rows[0].components[&component], authored);
    assert_eq!(
        rows[1].components[&component].value(),
        session
            .registry()
            .schema(&component)
            .unwrap()
            .authoring_initial_value()
    );
    assert!(option.command(&session, &selection).is_none());
}

#[test]
fn removing_an_opaque_component_uses_its_bound_entities_and_undo_restores_the_payload() {
    use super::target::ComponentRemoval;

    let mut session = relay_session();
    let selected = select_relay(&mut session, &["Courier", "Power cell C"]);
    let id = ComponentTypeId::new("project.Unavailable");
    let record = SceneComponent::Unknown(UnknownComponent {
        type_id: id.clone(),
        version: ComponentSchemaVersion::ONE,
        raw: RawRon::from_ron(r#"(unknown: "夜航", extra: Some([1,  2]))"#).unwrap(),
    });
    let mut scene = session.document().scene().clone();
    scene
        .entities
        .iter_mut()
        .find(|row| row.id == selected[0])
        .unwrap()
        .components
        .insert(id.clone(), record.clone());
    let prefabs = relay_prefabs(&scene);
    session
        .open_document_with_prefabs(EditorDocument::new(scene).unwrap(), prefabs)
        .unwrap();
    session.selection_mut().replace(selected.iter().copied());
    let selection = Selection::new(&session);
    let removal = ComponentRemoval::new(id.clone(), vec![(selected[0], record.version())]);

    session.selection_mut().replace([selected[1]]);
    assert!(removal.command(&session, &selection).is_none());
    session.selection_mut().replace(selected.iter().copied());
    let stale = ComponentRemoval::new(id.clone(), vec![(selected[1], record.version())]);
    assert!(stale.command(&session, &selection).is_none());

    let command = removal.command(&session, &selection).unwrap();
    assert!(session.apply(command).unwrap());
    assert!(removal.command(&session, &selection).is_none());
    assert!(session.undo().unwrap());
    let rows = selection.rows(&session).unwrap();
    assert_eq!(rows[0].components[&id], record);
    assert!(!rows[1].components.contains_key(&id));
}

#[test]
fn optional_fields_use_the_core_draft_and_keep_null_distinct_from_a_missing_value() {
    let mut session = relay_session();
    select_relay(&mut session, &["Courier"]);
    let (target, expected) = bind(&session, "nara.ColliderSource", "radius", &[]);
    let source = expected[0].as_ref();
    let mut draft = value_draft::RootDraft {
        node: Some(value_draft::ValueDraft::from_value(&target.shape, source)),
    };
    let null_action = field_draft::rows(&draft, &target.shape)
        .into_iter()
        .find_map(|row| match row {
            field_draft::StructuredRow::Button { action, .. }
                if action.operation == field_draft::StructureOperation::SetNull =>
            {
                Some(action)
            }
            _ => None,
        })
        .unwrap();
    field_draft::apply(&mut draft, &target.shape, source, &null_action).unwrap();
    let value = draft.node.unwrap().value(&target.shape).unwrap();
    let command = target.command(&session, &expected, value).unwrap().unwrap();
    assert!(session.apply(command).unwrap());
    assert_eq!(
        bound_values(&session, &target),
        [Some(ComponentValue::Null)]
    );
}

#[test]
fn a_project_schema_can_commit_full_width_integers_without_float_conversion() {
    let id = ComponentTypeId::new("example.Counters");
    let capability = ComponentCapability::SCENE_AUTHORING;
    let fields = [
        ("unsigned", ComponentValueKind::U64),
        ("signed", ComponentValueKind::I64),
    ]
    .map(|(name, kind)| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            kind,
        )
        .with_capabilities(capability)
    });
    let schema = ComponentSchema::new(id.clone(), "Counters", ComponentSchemaVersion::ONE)
        .with_fields(fields)
        .with_capabilities(capability);
    let mut scene = SceneDocument::new(
        "681a78dc-b6ab-4f3a-8eb6-4742d2ced237"
            .parse::<DocumentId>()
            .unwrap(),
    );
    let entity = "4f99b200-2a5e-4872-8d3d-000000000001"
        .parse::<EntityUuid>()
        .unwrap();
    let mut row = SceneEntity::new(entity, "Counter");
    row.components.insert(
        id,
        SceneComponent::known(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("unsigned", ComponentValue::U64(1)),
                ("signed", ComponentValue::I64(1)),
            ]),
        ),
    );
    scene.entities.push(row);
    let registry = EditorRegistry::new(
        [],
        nara_reflect::ComponentSchemaCatalog::from_components([schema]).unwrap(),
    )
    .unwrap();
    let mut session = EditorSession::new(EditorDocument::new(scene).unwrap(), registry);
    session.selection_mut().replace([entity]);
    for (field, expected_value) in [
        ("unsigned", ComponentValue::U64(u64::MAX)),
        ("signed", ComponentValue::I64(i64::MIN)),
    ] {
        let (target, expected) = bind(&session, "example.Counters", field, &[]);
        assert!(value_draft::parse_scalar(&target.shape, "-").is_err());
        assert_eq!(bound_values(&session, &target), expected);
        let text = match expected_value {
            ComponentValue::U64(value) => value.to_string(),
            ComponentValue::I64(value) => value.to_string(),
            _ => unreachable!(),
        };
        let value = value_draft::parse_scalar(&target.shape, &text).unwrap();
        let command = target.command(&session, &expected, value).unwrap().unwrap();
        assert!(session.apply(command).unwrap());
        assert_eq!(bound_values(&session, &target), [Some(expected_value)]);
    }
}

#[cfg(windows)]
mod suspended_drafts {
    use std::fs;

    use nara_project::workflow::ProjectTrust;
    use open_gpui::{App, AppContext as _, Context, Entity, TestAppContext};
    use open_gpui_base::{TextInput, TextInputEvent};

    use super::*;
    use crate::{inspector::InspectorPanel, workspace::Workspace};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Change {
        Unrelated,
        Opaque,
        NoSchema,
        NoField,
        ReadOnly,
        NoComponent,
    }

    fn install(
        workspace: &mut Workspace,
        schemas: impl IntoIterator<Item = ComponentSchema>,
        cx: &mut Context<Workspace>,
    ) {
        let catalog = ComponentSchemaCatalog::from_components(schemas).unwrap();
        let registry = EditorRegistry::new(builtin_component_schemas().unwrap(), catalog).unwrap();
        workspace.session.set_registry(registry).unwrap();
        workspace.revision += 1;
        cx.notify();
    }

    fn editor(panel: &InspectorPanel, component: &ComponentTypeId, cx: &App) -> Entity<TextInput> {
        panel
            .sections
            .iter()
            .find(|section| &section.id == component)
            .unwrap()
            .fields[0]
            .1
            .read(cx)
            .text_input()
            .unwrap()
            .clone()
    }

    #[test]
    fn suspended_field_drafts_resume_only_while_their_schema_and_rows_are_editable() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("assets")).unwrap();
        fs::write(
            directory.path().join("nara.toml"),
            "[project]\nname = 'Drafts'\nversion = '0.1'\n",
        )
        .unwrap();
        let entity = "4f99b200-2a5e-4872-8d3d-000000000001"
            .parse::<EntityUuid>()
            .unwrap();
        let id = ComponentTypeId::new("test.DraftFixture");
        let field = ComponentFieldSchema::required(
            ComponentFieldId::new("text"),
            "Text",
            ComponentFieldPath::from_fields(["text"]),
            ComponentValueKind::String,
        )
        .with_default_value(ComponentValue::String("Original".to_owned()));
        let editable = field
            .clone()
            .with_capabilities(ComponentCapability::SCENE_AUTHORING);
        let schema_for = |version| {
            ComponentSchema::new(id.clone(), "Fixture", version)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        };
        let schema = schema_for(ComponentSchemaVersion::ONE).with_fields([editable.clone()]);
        let cx = TestAppContext::single();
        for change in [
            Change::Unrelated,
            Change::Opaque,
            Change::NoSchema,
            Change::NoField,
            Change::ReadOnly,
            Change::NoComponent,
        ] {
            let (workspace, inspector, input) = cx.update(|cx| {
                let workspace =
                    cx.new(|_| Workspace::open(directory.path(), ProjectTrust::Untrusted).unwrap());
                workspace.update(cx, |workspace, cx| {
                    install(workspace, [schema.clone()], cx);
                    let mut row = SceneEntity::new(entity, "Original");
                    row.components.insert(
                        id.clone(),
                        SceneComponent::known(
                            ComponentSchemaVersion::ONE,
                            ComponentValue::Map(BTreeMap::new()),
                        ),
                    );
                    assert!(workspace.apply(EditorCommand::Create(row), cx));
                    workspace.select([entity], cx);
                });
                let inspector = cx.new(|cx| InspectorPanel::new(workspace.clone(), cx));
                let input = editor(inspector.read(cx), &id, cx);
                input.update(cx, |input, cx| {
                    input.set_value("Pending", cx);
                    cx.emit(TextInputEvent::Change("Pending".into()));
                });
                (workspace, inspector, input)
            });
            cx.update(|cx| {
                assert!(!inspector.read(cx).drafts(cx).is_empty());
                workspace.update(cx, |workspace, cx| workspace.select([], cx));
            });
            cx.update(|cx| {
                assert_eq!(inspector.read(cx).suspended_fields.len(), 1);
                workspace.update(cx, |workspace, cx| match change {
                    Change::Unrelated => install(
                        workspace,
                        [
                            schema.clone(),
                            ComponentSchema::new(
                                ComponentTypeId::new("test.Unrelated"),
                                "Unrelated",
                                ComponentSchemaVersion::ONE,
                            ),
                        ],
                        cx,
                    ),
                    Change::Opaque => install(
                        workspace,
                        [schema_for(ComponentSchemaVersion::new(2).unwrap())
                            .with_fields([editable.clone()])],
                        cx,
                    ),
                    Change::NoSchema => install(workspace, [], cx),
                    Change::NoField => {
                        install(workspace, [schema_for(ComponentSchemaVersion::ONE)], cx)
                    }
                    Change::ReadOnly => install(
                        workspace,
                        [schema_for(ComponentSchemaVersion::ONE).with_fields([field
                            .clone()
                            .with_capabilities([
                                ComponentCapability::Scene,
                                ComponentCapability::Inspect,
                            ])])],
                        cx,
                    ),
                    Change::NoComponent => assert!(workspace.apply(
                        EditorCommand::RemoveComponent {
                            entities: vec![entity],
                            component: id.clone(),
                        },
                        cx
                    )),
                });
            });
            cx.update(|cx| {
                let retained = change == Change::Unrelated;
                assert_eq!(
                    inspector.read(cx).drafts(cx).is_empty(),
                    !retained,
                    "{change:?}"
                );
                workspace.update(cx, |workspace, cx| {
                    if !retained {
                        if change == Change::NoComponent {
                            workspace.undo(cx);
                        } else {
                            install(workspace, [schema.clone()], cx);
                        }
                    }
                    workspace.select([entity], cx);
                });
            });
            cx.update(|cx| {
                let restored = editor(inspector.read(cx), &id, cx);
                if change == Change::Unrelated {
                    assert_eq!(restored.entity_id(), input.entity_id());
                    assert_eq!(restored.read(cx).value(), "Pending");
                    restored.update(cx, |_, cx| {
                        cx.emit(TextInputEvent::Submit("Pending".into()))
                    });
                } else {
                    assert_ne!(restored.entity_id(), input.entity_id(), "{change:?}");
                    assert_eq!(restored.read(cx).value(), "Original");
                }
            });
            cx.update(|cx| {
                assert!(inspector.read(cx).drafts(cx).is_empty());
                if change == Change::Unrelated {
                    let (target, value) =
                        bind(&workspace.read(cx).session, id.as_str(), "text", &[]);
                    assert!(target.selection.matches(&workspace.read(cx).session));
                    assert_eq!(value, [Some(ComponentValue::String("Pending".to_owned()))]);
                }
            });
        }
    }
}
