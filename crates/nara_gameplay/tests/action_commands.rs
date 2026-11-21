#![cfg(feature = "input")]

use nara_app::{App, CoreStage, FixedTime, RuntimeInstance};
use nara_core::ByteLimit;
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_gameplay::{
    ActionCommandBinding, ActionCommandMap, ActionCommandMapError, GameplayActionPlugin,
    GameplayCommandBatch, GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandPayload,
    GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandSet, GameplayCommandSource,
    GameplayCommandTarget, GameplayCommandTypeId, GameplayCommandValue, RuntimeEntityReference,
};
use nara_identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    RuntimeEntityIncarnation, SceneEntityId, SceneInstanceId,
};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionPhase, ButtonDriverInput, InputPlugin, InputSeatId, InputWindowId, KeyCode,
    apply_keyboard_driver_input_at, connect_input_endpoint,
};

fn movement(pressed: bool) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(2))
        .unwrap();
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .unwrap();
    GameplayCommandDraft::new(GameplayCommandTypeId::new("player.move").unwrap())
        .with_target(GameplayCommandTarget::named("player.two").unwrap())
        .with_payload(payload)
}

fn runtime_targets() -> [GameplayCommandTarget; 2] {
    [
        GameplayCommandTarget::Entity(RuntimeEntityReference::scene(
            SceneInstanceId::new(7).unwrap(),
            SceneEntityId::new("player").unwrap(),
            RuntimeEntityIncarnation::new(1).unwrap(),
        )),
        GameplayCommandTarget::Entity(RuntimeEntityReference::persistent(
            PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new("runtime").unwrap(),
                PersistentRuntimeId::parse_str("a1b2c3d4-e5f6-47a8-90ab-1234567890cd").unwrap(),
            ),
            RuntimeEntityIncarnation::new(1).unwrap(),
        )),
    ]
}

#[test]
fn complete_drafts_preserve_targets_and_refusal_leaves_existing_bindings_indexed() {
    let action = ActionId::new("move").unwrap();
    let draft = movement(true);
    let binding =
        ActionCommandBinding::from_command(action.clone(), ActionPhase::Started, draft.clone());
    assert_eq!(binding.command(), &draft);
    assert_eq!(binding.seat(), InputSeatId::PRIMARY);
    assert_eq!(binding.context(), &ActionContext::gameplay());

    let seat = InputSeatId::new(1);
    let context = ActionContext::new("menu").unwrap();
    let mut commands = ActionCommandMap::default();
    commands
        .bind(binding.with_seat(seat).with_context(context.clone()))
        .unwrap();
    let untargeted = GameplayCommandDraft::new(GameplayCommandTypeId::new("player.audit").unwrap());
    commands
        .bind(
            ActionCommandBinding::from_command(
                action.clone(),
                ActionPhase::Started,
                untargeted.clone(),
            )
            .with_seat(seat)
            .with_context(context.clone()),
        )
        .unwrap();
    assert_eq!(commands.bindings()[1].command(), &untargeted);
    let before = commands.clone();

    for target in runtime_targets() {
        let binding = ActionCommandBinding::from_command(
            action.clone(),
            ActionPhase::Started,
            draft.clone().with_target(target.clone()),
        )
        .with_seat(seat)
        .with_context(context.clone());
        assert_eq!(binding.command().target(), Some(&target));
        assert_eq!(
            commands.bind(binding),
            Err(ActionCommandMapError::RuntimeEntityTarget),
        );
        assert_eq!(commands, before);
        assert_eq!(
            commands
                .matching_bindings(seat, &action, &context, ActionPhase::Started)
                .collect::<Vec<_>>(),
            before.bindings().iter().collect::<Vec<_>>(),
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn complete_drafts_keep_the_wire_shape_and_runtime_targets_still_fail_map_admission() {
    use serde_json::json;

    let action = ActionId::new("move").unwrap();
    let context = ActionContext::new("menu").unwrap();
    let seat = InputSeatId::new(1);
    let binding =
        ActionCommandBinding::from_command(action.clone(), ActionPhase::Released, movement(false))
            .with_seat(seat)
            .with_context(context.clone());
    let wire = json!({
        "seat": 1,
        "action": "move",
        "context": "menu",
        "phase": "Released",
        "command": {
            "command_type": "player.move",
            "target": { "Named": "player.two" },
            "payload": { "direction": { "I64": 2 }, "pressed": { "Bool": false } }
        }
    });
    assert_eq!(serde_json::to_value(&binding).unwrap(), wire);
    let mut commands = ActionCommandMap::default();
    commands.bind(binding.clone()).unwrap();
    let encoded = serde_json::to_value(&commands).unwrap();
    assert_eq!(encoded, json!({ "bindings": [wire] }));
    let decoded: ActionCommandMap = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, commands);
    assert_eq!(
        decoded
            .matching_bindings(seat, &action, &context, ActionPhase::Released)
            .collect::<Vec<_>>(),
        vec![&binding],
    );

    for target in runtime_targets() {
        let binding = ActionCommandBinding::from_command(
            action.clone(),
            ActionPhase::Released,
            movement(false).with_target(target.clone()),
        );
        let encoded = serde_json::to_value(&binding).unwrap();
        let decoded: ActionCommandBinding = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded.command().target(), Some(&target));
        let error = serde_json::from_value::<ActionCommandMap>(json!({
            "bindings": [encoded]
        }))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&ActionCommandMapError::RuntimeEntityTarget.to_string())
        );
    }
}

#[derive(Default, Resource)]
struct Dispatched(Vec<GameplayCommandEnvelope>);

#[test]
fn complete_drafts_dispatch_in_binding_order_with_the_original_seat_context_and_phase() {
    let action = ActionId::new("move").unwrap();
    let context = ActionContext::new("menu").unwrap();
    let seat = InputSeatId::new(1);
    let window = InputWindowId::new(10);
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder.declare_context(context.clone(), true).unwrap();
    builder
        .bind(ActionBinding::key(action.clone(), KeyCode::Enter).with_context(context.clone()))
        .unwrap();
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut maps = ActionMaps::default();
    maps.insert(seat, map).unwrap();

    let started = [
        movement(true),
        GameplayCommandDraft::new(GameplayCommandTypeId::new("player.audit").unwrap()),
    ];
    let released = movement(false);
    let mut commands = ActionCommandMap::default();
    for draft in &started {
        commands
            .bind(
                ActionCommandBinding::from_command(
                    action.clone(),
                    ActionPhase::Started,
                    draft.clone(),
                )
                .with_seat(seat)
                .with_context(context.clone()),
            )
            .unwrap();
    }
    commands
        .bind(
            ActionCommandBinding::from_command(
                action.clone(),
                ActionPhase::Released,
                released.clone(),
            )
            .with_seat(seat)
            .with_context(context.clone()),
        )
        .unwrap();
    commands
        .bind(
            ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("primary.should-not-match").unwrap(),
            )
            .with_context(context),
        )
        .unwrap();
    #[cfg(feature = "serde")]
    let commands: ActionCommandMap =
        serde_json::from_value(serde_json::to_value(&commands).unwrap()).unwrap();

    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.insert_resource(maps).unwrap();
    app.insert_resource(commands).unwrap();
    app.add_plugin(GameplayActionPlugin).unwrap();
    app.insert_resource(Dispatched::default()).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (|batch: Res<GameplayCommandBatch>, mut observed: ResMut<Dispatched>| {
            observed.0.extend_from_slice(batch.commands());
        })
        .in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let outcome = runtime
        .with_driver_scope(|scope| {
            connect_input_endpoint(scope, window, seat)?;
            apply_keyboard_driver_input_at(
                scope,
                window,
                seat,
                ButtonDriverInput::Press(KeyCode::Enter),
            )
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let outcome = runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                window,
                seat,
                ButtonDriverInput::Release(KeyCode::Enter),
            )
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

    let observed = &runtime.world().resource::<Dispatched>().0;
    assert_eq!(observed.len(), 3);
    for (index, (actual, expected)) in observed
        .iter()
        .zip(started.iter().chain([&released]))
        .enumerate()
    {
        assert_eq!(actual.command(), expected);
        assert_eq!(actual.source(), &GameplayCommandSource::LocalAction);
        assert_eq!(actual.source_sequence().get(), index as u64 + 1);
    }
    assert_eq!(
        observed
            .iter()
            .map(|command| command.tick().get())
            .collect::<Vec<_>>(),
        [1, 1, 2],
    );
    let queue = runtime.world().resource::<GameplayCommandQueue>();
    assert!(queue.is_idle());
    assert_eq!(queue.stats().acknowledged, 3);
    runtime.finish().unwrap();
}
