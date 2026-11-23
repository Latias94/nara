use super::*;

const LEAF: &str = "prefabs/inspection-leaf.prefab.json";
const LEAF_ANCHOR: &str = "leaf";
const DEFAULT_ENDPOINT: &str = "right";
const SHADOW_ENDPOINT: &str = "left";

#[test]
fn a_registered_panel_writes_components_with_contextual_defaults_on_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_writes_components_with_contextual_defaults_on_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let cases = [
        (EMPTY, SetMode::Add, None),
        (INHERITED, SetMode::Replace, None),
        ("empty-shadow", SetMode::Add, Some(SHADOW_ENDPOINT)),
        ("base-shadow", SetMode::Replace, Some(SHADOW_ENDPOINT)),
    ];
    let mount = PrefabInstance::forest(AssetRef::path("inspection-base.prefab.json").unwrap());
    let key = base_key();
    let SourceDocumentOrigin::ProjectLocation { universe, .. } = key.origin() else {
        unreachable!();
    };
    let leaf_key = SourceDocumentKey::project_location(
        *universe,
        AssetSourcePath::new(LEAF).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let leaf_target = |entity| PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(LEAF_ANCHOR)),
            expected_base: leaf_key.clone(),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    };

    let mut leaf_anchor = SceneEntityRecord::new(id(LEAF_ANCHOR));
    leaf_anchor.prefab = Some(PrefabInstance::forest(
        AssetRef::path("inspection-leaf.prefab.json").unwrap(),
    ));
    let base = PrefabDocument::from_ordered_roots([leaf_anchor]);
    let default = local_reference(DEFAULT_ENDPOINT);
    let inherited = settings(INITIAL_SCORE, Some(1));
    let inherited_record = component_record(&inherited)
        .with_contextual_default(ComponentFieldId::new(TARGET_FIELD), default.clone());
    let leaf = PrefabDocument::from_ordered_roots(
        [DEFAULT_ENDPOINT, SHADOW_ENDPOINT]
            .map(|entity| SceneEntityRecord::new(id(entity)))
            .into_iter()
            .chain(cases.map(|(entity, mode, _)| {
                let target = SceneEntityRecord::new(id(entity));
                if mode == SetMode::Replace {
                    target.with_component(ComponentTypeId::new(COMPONENT), inherited_record.clone())
                } else {
                    target
                }
            })),
    );
    let mut fixture = ComponentFixture::with_sources(
        kind,
        source,
        &mount,
        [
            (BASE, base.to_json_string().unwrap()),
            (LEAF, leaf.to_json_string().unwrap()),
            (
                SCHEMA,
                component_schema(Some(default.clone()))
                    .to_json_string()
                    .unwrap(),
            ),
        ],
    );

    let document = fixture.document;
    let mut intents = Vec::new();
    let mut history = vec![mount.clone()];
    for (index, (entity, mode, explicit)) in cases.into_iter().enumerate() {
        let target = leaf_target(entity);
        let inherited_value = (mode == SetMode::Replace).then_some(&inherited);
        let (inspection, selected) = fixture.inspect_target(
            "left",
            &mounted_target("left", &target),
            None,
            inherited_value,
        );
        if inherited_value.is_some() {
            assert_reference(
                &mut fixture,
                inspection,
                &selected,
                mounted_target("left", &leaf_target(DEFAULT_ENDPOINT)),
                true,
            );
        }

        let mut value = settings(51, None);
        if let Some(endpoint) = explicit {
            let ComponentValue::Map(fields) = &mut value else {
                unreachable!();
            };
            fields.insert(TARGET_STORAGE.into(), local_reference(endpoint));
        }
        intents.push(PrefabComponentIntent::Set {
            mode,
            target,
            component: ComponentTypeId::new(COMPONENT),
            record: component_record(&value)
                .with_contextual_default(ComponentFieldId::new(TARGET_FIELD), default.clone()),
        });
        let before = &history[index];
        let after = with_intents(&mount, intents.iter().cloned());
        let captured = basis(&fixture.app, document);
        fixture.review(inspection, selected, set(mode, &value), before, &after);
        assert_eq!(basis(&fixture.app, document), captured);
        fixture.assert_live(before, index != 0, index, 0);
        assert_eq!(
            fs::read(&fixture.source).unwrap(),
            fixture.original.as_bytes()
        );

        let outcome = match mode {
            SetMode::Add => EditKind::Added,
            SetMode::Replace => EditKind::Replaced,
        };
        fixture.commit(inspection, outcome);
        fixture.assert_live(&after, true, index + 1, 0);
        history.push(after);
    }

    let count = cases.len();
    for (undo, expected) in history[..count].iter().enumerate().rev() {
        replay(&mut fixture.app, document, false);
        fixture.assert_live(expected, undo != 0, undo, count - undo);
    }
    for (undo, expected) in history.iter().enumerate().skip(1) {
        replay(&mut fixture.app, document, true);
        fixture.assert_live(expected, true, undo, count - undo);
    }
    assert_eq!(
        fs::read(&fixture.source).unwrap(),
        fixture.original.as_bytes()
    );

    let edited = &history[count];
    let saved = fixture.save(edited, count);
    assert_eq!(saved, containing_json(kind, edited, &mount).as_bytes());
    replay(&mut fixture.app, document, false);
    fixture.assert_live(&history[count - 1], true, count - 1, 1);
    fixture.reopen(edited, &saved);

    for ((entity, mode, explicit), intent) in cases.into_iter().zip(&intents) {
        let PrefabComponentIntent::Set { record, .. } = intent else {
            unreachable!();
        };
        let expected_kind = match mode {
            SetMode::Add => OverrideKind::Added,
            SetMode::Replace => OverrideKind::Replaced,
        };
        let (inspection, selected) = fixture.inspect_target(
            "left",
            &mounted_target("left", &leaf_target(entity)),
            Some(expected_kind),
            Some(record.value()),
        );
        assert_reference(
            &mut fixture,
            inspection,
            &selected,
            PrefabEntityTarget::source(id(explicit.unwrap_or(DEFAULT_ENDPOINT))),
            explicit.is_none(),
        );
        cancel(&mut fixture.panel, &mut fixture.app, inspection);
    }

    let (sibling, selected) = fixture.inspect_target(
        "right",
        &mounted_target("right", &leaf_target(INHERITED)),
        None,
        Some(&inherited),
    );
    assert_reference(
        &mut fixture,
        sibling,
        &selected,
        mounted_target("right", &leaf_target(DEFAULT_ENDPOINT)),
        true,
    );
    cancel(&mut fixture.panel, &mut fixture.app, sibling);
    fixture.assert_untouched_files();
    assert!(fixture.app.close());
}

fn local_reference(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(entity) })
}

fn assert_reference(
    fixture: &mut ComponentFixture,
    inspection: EditorPrefabInspectionId,
    selected: &EditorPrefabComponentAddress,
    endpoint: PrefabEntityTarget,
    defaulted: bool,
) {
    fixture.panel.observe(&mut fixture.app, |context| {
        let snapshot = context.prefab_effective_observation(inspection).unwrap();
        let component = snapshot
            .target(&selected.entity)
            .unwrap()
            .component(&selected.component)
            .unwrap();
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(16).unwrap(),
            ByteLimit::new(1024).unwrap(),
        );
        let read = component
            .property_value(&property(TARGET_FIELD), &mut budget)
            .unwrap();
        assert!(read.controlling_claim().is_none());
        let value = match (defaulted, read.value) {
            (true, AuthoringPrefabPropertyValue::Defaulted(value))
            | (false, AuthoringPrefabPropertyValue::Present(value)) => value,
            (_, actual) => panic!(
                "unexpected reference presence at {:?}: {actual:?}",
                selected.entity
            ),
        };
        assert_eq!(
            value.copy_value(Default::default()).unwrap(),
            authored_reference(endpoint)
        );
    });
}
