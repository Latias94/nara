use std::time::Duration;

use nara::{
    app::RuntimeInstance,
    core::ByteLimit,
    gameplay::{
        GameplayCommandPayload, GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    input::{
        ActionBinding, ActionContextDefinition, ActionMap, ActionMapFileCandidate,
        ButtonDriverInput, InputPlugin, KeyCode, SemanticActions, apply_keyboard_driver_input,
    },
};
use serde_json::{Value, json};

use super::*;

const AUTHORED_INPUT: &[u8] = include_bytes!("../../../input/explorer.actions.json");

#[test]
fn movement_commands_are_semantic_and_do_not_install_physical_bindings() {
    let app = semantic_input_app();
    assert!(!app.world().contains_resource::<ActionMaps>());
    let commands = app.world().resource::<ActionCommandMap>();
    assert_eq!(commands.bindings().len(), 14);
    let context = ActionContext::gameplay();

    for (direction, name) in [
        "two-level-game.move-a",
        "two-level-game.move-d",
        "two-level-game.move-w",
        "two-level-game.move-s",
    ]
    .into_iter()
    .enumerate()
    {
        let id = ActionId::new(name).unwrap();
        for (offset, (phase, pressed)) in
            [(ActionPhase::Started, true), (ActionPhase::Released, false)]
                .into_iter()
                .enumerate()
        {
            let mut payload = GameplayCommandPayload::new();
            payload
                .insert("direction", GameplayCommandValue::I64(direction as i64))
                .unwrap();
            payload
                .insert("pressed", GameplayCommandValue::Bool(pressed))
                .unwrap();
            let expected = ActionCommandBinding::new(
                id.clone(),
                phase,
                GameplayCommandTypeId::new("two-level-game.move").unwrap(),
            )
            .with_payload(payload);
            assert_eq!(
                commands.bindings().get(direction * 2 + offset),
                Some(&expected),
            );
            assert_eq!(
                commands
                    .matching_bindings(InputSeatId::PRIMARY, &id, &context, phase)
                    .collect::<Vec<_>>(),
                vec![&expected],
            );
        }
    }
}

#[test]
fn one_shot_commands_keep_their_semantic_ids_and_complete_drafts() {
    let app = semantic_input_app();
    let commands = app.world().resource::<ActionCommandMap>();
    for (offset, (name, command)) in [
        ("two-level-game.e", "two-level-game.travel"),
        ("two-level-game.q", "two-level-game.instance"),
        ("two-level-game.m", "two-level-game.marker"),
        ("two-level-game.k", "two-level-game.save"),
        ("two-level-game.l", "two-level-game.restore"),
        ("two-level-game.r", "two-level-game.retry"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            commands.bindings().get(8 + offset),
            Some(&ActionCommandBinding::new(
                ActionId::new(name).unwrap(),
                ActionPhase::Started,
                GameplayCommandTypeId::new(command).unwrap(),
            )),
        );
    }
}

#[test]
fn source_and_cooked_recipes_share_game_input_composition() {
    let source = crate::product_recipe();
    let cooked = crate::cooked_recipe();
    let source_entries = source
        .entries()
        .map(|entry| (entry.plugin_id(), entry.configuration_fingerprint()))
        .collect::<Vec<_>>();
    let cooked_entries = cooked
        .entries()
        .map(|entry| (entry.plugin_id(), entry.configuration_fingerprint()))
        .collect::<Vec<_>>();
    assert_eq!(source_entries, cooked_entries);
    assert_eq!(
        source
            .entries()
            .filter(|entry| entry.plugin_id() == ID)
            .count(),
        1,
    );
    assert_eq!(ID, PluginId::new("two-level-game.input"));
}

#[test]
fn authored_defaults_decode_with_intrinsic_primary_identity_and_original_keys() {
    let candidate = default_candidate();
    assert_eq!(candidate.product(), input_product());
    assert_eq!(candidate.product().product(), ImportedProductId::PRIMARY);
    assert_eq!(INPUT_PATH, "input/explorer.actions.json");
    assert_eq!(
        candidate.definition().contexts(),
        &[ActionContextDefinition {
            context: ActionContext::gameplay(),
            enabled: true,
        }],
    );
    let expected = [
        ("two-level-game.move-a", 'a'),
        ("two-level-game.move-d", 'd'),
        ("two-level-game.move-w", 'w'),
        ("two-level-game.move-s", 's'),
        ("two-level-game.e", 'e'),
        ("two-level-game.q", 'q'),
        ("two-level-game.m", 'm'),
        ("two-level-game.k", 'k'),
        ("two-level-game.l", 'l'),
        ("two-level-game.r", 'r'),
    ]
    .map(|(name, key)| ActionBinding::key(ActionId::new(name).unwrap(), KeyCode::Character(key)));
    assert_eq!(candidate.definition().bindings(), expected.as_slice());
    let world = world_with_candidate(candidate);
    assert_eq!(
        movement_bindings(&world)
            .unwrap()
            .map(|binding| binding.key),
        ['a', 'd', 'w', 's'].map(KeyCode::Character),
    );
}

#[test]
fn edited_definition_resolves_actual_primary_keys_in_semantic_direction_order() {
    let mut world = world_with_candidate(default_candidate());
    let original = movement_bindings(&world).unwrap();
    let keys = [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
    ];
    let edited = edited_candidate(|value| {
        for (binding, key) in value["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .take(4)
            .zip(keys)
        {
            binding["input"] = json!({ "Key": key });
        }
        value["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .reverse();
    });
    let (_, definition) = edited.into_parts();
    let mut maps = world.resource_mut::<ActionMaps>();
    maps.get_mut(InputSeatId::PRIMARY)
        .unwrap()
        .replace_bindings(definition.into_bindings());
    maps.insert(InputSeatId::new(1), map_from_candidate(default_candidate()))
        .unwrap();
    drop(maps);

    let actual = movement_bindings(&world).unwrap();
    assert_eq!(actual.each_ref().map(|binding| binding.key), keys,);
    for (binding, original) in actual.iter().zip(original) {
        assert_eq!(binding.action, original.action);
        assert_eq!(binding.context, original.context);
    }
}

#[test]
fn capture_resolves_normalized_keyboard_codes_without_platform_restrictions() {
    for key in [
        KeyCode::Escape,
        KeyCode::Space,
        KeyCode::Enter,
        KeyCode::Character('j'),
        KeyCode::Character('J'),
        KeyCode::Character('0'),
        KeyCode::Character('é'),
    ] {
        let candidate = edited_candidate(|value| {
            value["payload"]["bindings"][0]["input"] = json!({ "Key": key });
        });
        let world = world_with_candidate(candidate);
        assert_eq!(movement_bindings(&world).unwrap()[0].key, key);
    }
}

#[test]
fn capture_requires_primary_mapping_and_never_borrows_another_seat() {
    let mut world = World::new();
    assert!(movement_bindings(&world).unwrap_err().contains("PRIMARY"));
    world.insert_resource(ActionMaps::new(map_from_candidate(default_candidate())));
    let mut maps = world.resource_mut::<ActionMaps>();
    let original = maps.remove(InputSeatId::PRIMARY).unwrap();
    let owner = original.bindings_handle();
    maps.insert(InputSeatId::new(1), original).unwrap();
    drop(maps);
    assert!(movement_bindings(&world).unwrap_err().contains("PRIMARY"));
    assert!(
        owner.shares_owner_with(
            &world
                .resource::<ActionMaps>()
                .get(InputSeatId::new(1))
                .unwrap()
                .bindings_handle()
        )
    );
}

#[test]
fn capture_requires_each_semantic_movement_action_in_gameplay_context() {
    for replacement in [None, Some(json!("two-level-game.foreign"))] {
        let candidate = edited_candidate(|value| {
            if let Some(name) = replacement {
                value["payload"]["bindings"][0]["action"] = name;
            } else {
                value["payload"]["bindings"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
        });
        assert_capture_refused(&world_with_candidate(candidate), "move-a is missing");
    }
    let candidate = edited_candidate(|value| {
        value["payload"]["contexts"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "context": "menu",
                "enabled": true
            }));
        value["payload"]["bindings"][0]["context"] = json!("menu");
    });
    assert_capture_refused(&world_with_candidate(candidate), "move-a is missing");
}

#[test]
fn capture_reads_live_context_state_and_does_not_reenable_it() {
    let mut world = world_with_candidate(default_candidate());
    world
        .resource_mut::<ActionMaps>()
        .get_mut(InputSeatId::PRIMARY)
        .unwrap()
        .disable_context(&ActionContext::gameplay())
        .unwrap();
    assert_capture_refused(&world, "context is disabled");
    world
        .resource_mut::<ActionMaps>()
        .get_mut(InputSeatId::PRIMARY)
        .unwrap()
        .enable_context(&ActionContext::gameplay())
        .unwrap();
    assert!(movement_bindings(&world).is_ok());
}

#[test]
fn multiple_sources_start_and_drive_gameplay_but_refuse_exact_capture() {
    let candidate = edited_candidate(|value| {
        let mut alternative = value["payload"]["bindings"][0].clone();
        alternative["input"]["Key"]["Character"] = json!("j");
        value["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .push(alternative);
    });
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.insert_resource(ActionMaps::new(map_from_candidate(candidate)))
        .unwrap();
    GameInputPlugin.build(&mut app).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_capture_refused(runtime.world(), "multiple sources");

    let admitted = runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Character('j')))
        })
        .unwrap();
    assert!(admitted.post_operation_fault.is_none());
    admitted.output.unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("two-level-game.move-a").unwrap(),
        &ActionContext::gameplay(),
    ));
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        1,
    );
    assert_capture_refused(runtime.world(), "multiple sources");
}

#[test]
fn capture_refuses_duplicate_movement_keys() {
    let candidate = edited_candidate(|value| {
        value["payload"]["bindings"][1]["input"]["Key"]["Character"] = json!("a");
    });
    assert_capture_refused(&world_with_candidate(candidate), "must be distinct");
}

#[test]
fn capture_refuses_additional_nonkeyboard_sources_for_movement() {
    for input in [json!({ "Mouse": "Left" }), json!({ "GamepadButton": 0 })] {
        let candidate = edited_candidate(|value| {
            let mut alternative = value["payload"]["bindings"][0].clone();
            alternative["input"] = input;
            value["payload"]["bindings"]
                .as_array_mut()
                .unwrap()
                .push(alternative);
        });
        assert_capture_refused(&world_with_candidate(candidate), "multiple sources");
    }
}

#[test]
fn capture_refuses_foreign_key_fanout_even_in_a_disabled_context() {
    let candidate = edited_candidate(|value| {
        value["payload"]["contexts"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "context": "menu",
                "enabled": false
            }));
        value["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "action": "two-level-game.foreign",
                "input": { "Key": { "Character": "a" } },
                "context": "menu",
                "transform": "Button"
            }));
    });
    assert_capture_refused(&world_with_candidate(candidate), "must not be shared");
}

#[test]
fn capture_refuses_nonkeyboard_sources_and_nonbutton_transforms() {
    for (input, transform, error) in [
        (
            json!({ "Mouse": "Left" }),
            json!("Button"),
            "keyboard Button",
        ),
        (
            json!({ "GamepadButton": 0 }),
            json!("Button"),
            "keyboard Button",
        ),
        (
            json!({ "Key": { "Character": "a" } }),
            json!({ "AxisButton": { "direction": "Negative" } }),
            "keyboard Button",
        ),
    ] {
        let candidate = edited_candidate(|value| {
            value["payload"]["bindings"][0]["input"] = input;
            value["payload"]["bindings"][0]["transform"] = transform;
        });
        assert_capture_refused(&world_with_candidate(candidate), error);
    }
}

fn semantic_input_app() -> App {
    let mut app = App::new();
    app.insert_resource(ActionCommandMap::default()).unwrap();
    GameInputPlugin.build(&mut app).unwrap();
    app
}

fn default_candidate() -> ActionMapFileCandidate {
    ActionMapFileCandidate::decode_json_bytes(AUTHORED_INPUT).unwrap()
}

fn edited_candidate(edit: impl FnOnce(&mut Value)) -> ActionMapFileCandidate {
    let mut value = serde_json::from_slice(AUTHORED_INPUT).unwrap();
    edit(&mut value);
    ActionMapFileCandidate::decode_json_bytes(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn map_from_candidate(candidate: ActionMapFileCandidate) -> ActionMap {
    let (_, definition) = candidate.into_parts();
    ActionMap::new(
        definition.into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap()
}

fn world_with_candidate(candidate: ActionMapFileCandidate) -> World {
    let mut world = World::new();
    world.insert_resource(ActionMaps::new(map_from_candidate(candidate)));
    world
}

fn assert_capture_refused(world: &World, expected: &str) {
    let map = world
        .resource::<ActionMaps>()
        .get(InputSeatId::PRIMARY)
        .unwrap();
    let owner = map.bindings_handle();
    let contexts = map
        .definition()
        .contexts()
        .iter()
        .map(|entry| map.is_context_enabled(&entry.context))
        .collect::<Vec<_>>();
    let error = movement_bindings(world).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    let map = world
        .resource::<ActionMaps>()
        .get(InputSeatId::PRIMARY)
        .unwrap();
    assert!(owner.shares_owner_with(&map.bindings_handle()));
    assert_eq!(
        contexts,
        map.definition()
            .contexts()
            .iter()
            .map(|entry| map.is_context_enabled(&entry.context))
            .collect::<Vec<_>>(),
    );
}
