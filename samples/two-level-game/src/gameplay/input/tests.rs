use nara::{
    gameplay::{GameplayCommandPayload, GameplayCommandTypeId, GameplayCommandValue},
    input::{ActionContextDefinition, ActionTransform, ActionValueKind, InputBinding, InputSeatId},
};

use super::*;

#[test]
fn movement_contract_matches_installed_bindings_and_complete_commands() {
    let app = desktop_input_app();
    let maps = app.world().resource::<ActionMaps>();
    let map = maps.get(InputSeatId::PRIMARY).unwrap();
    let commands = app.world().resource::<ActionCommandMap>();
    let context = ActionContext::gameplay();
    assert_eq!(map.bindings().len(), 10);
    assert_eq!(commands.bindings().len(), 14);
    assert_eq!(
        map.definition().contexts(),
        &[ActionContextDefinition {
            context: context.clone(),
            enabled: true,
        }],
    );
    assert!(map.is_context_enabled(&context));

    for (direction, (key, action)) in [
        (KeyCode::Character('a'), "two-level-game.move-a"),
        (KeyCode::Character('d'), "two-level-game.move-d"),
        (KeyCode::Character('w'), "two-level-game.move-w"),
        (KeyCode::Character('s'), "two-level-game.move-s"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = ActionId::new(action).unwrap();
        let expected = ActionBinding {
            action: id.clone(),
            input: InputBinding::Key(key),
            context: context.clone(),
            transform: ActionTransform::Button,
        };
        assert_eq!(MOVE_KEYS[direction], key);
        assert_eq!(movement_action(direction), Some(id.clone()));
        assert_eq!(movement_binding(direction).as_ref(), Some(&expected));
        assert_eq!(map.binding_at(direction), Some(&expected));
        assert_eq!(
            map.definition().value_kind(&id, &context),
            Some(ActionValueKind::Button),
        );

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
fn one_shot_bindings_keep_their_order_and_complete_commands() {
    let app = desktop_input_app();
    let maps = app.world().resource::<ActionMaps>();
    let map = maps.get(InputSeatId::PRIMARY).unwrap();
    let commands = app.world().resource::<ActionCommandMap>();

    for (offset, (key, action, command)) in [
        ('e', "two-level-game.e", "two-level-game.travel"),
        ('q', "two-level-game.q", "two-level-game.instance"),
        ('m', "two-level-game.m", "two-level-game.marker"),
        ('k', "two-level-game.k", "two-level-game.save"),
        ('l', "two-level-game.l", "two-level-game.restore"),
        ('r', "two-level-game.r", "two-level-game.retry"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = ActionId::new(action).unwrap();
        assert_eq!(
            map.binding_at(4 + offset),
            Some(&ActionBinding {
                action: id.clone(),
                input: InputBinding::Key(KeyCode::Character(key)),
                context: ActionContext::gameplay(),
                transform: ActionTransform::Button,
            }),
        );
        assert_eq!(
            commands.bindings().get(8 + offset),
            Some(&ActionCommandBinding::new(
                id,
                ActionPhase::Started,
                GameplayCommandTypeId::new(command).unwrap(),
            )),
        );
    }
}

#[test]
fn invalid_movement_directions_have_no_action_or_binding() {
    for direction in [4, 5, usize::MAX] {
        assert_eq!(movement_action(direction), None);
        assert_eq!(movement_binding(direction), None);
    }
}

fn desktop_input_app() -> App {
    let mut app = App::new();
    app.insert_resource(ActionCommandMap::default()).unwrap();
    DesktopInputPlugin.build(&mut app).unwrap();
    app
}
