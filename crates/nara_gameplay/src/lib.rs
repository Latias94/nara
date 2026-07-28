//! Bounded authoritative gameplay command ingress and action mapping.

mod action;
mod command;
mod queue;

pub use action::{
    ActionCommandBinding, ActionCommandMap, ActionCommandMapError, MAX_ACTION_COMMAND_BINDINGS,
};
pub use command::{
    GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandIdError,
    GameplayCommandIngressSource, GameplayCommandKey, GameplayCommandPayload,
    GameplayCommandPayloadError, GameplayCommandSource, GameplayCommandSourceId,
    GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTarget,
    GameplayCommandTargetId, GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
    MAX_GAMEPLAY_COMMAND_ID_BYTES, MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES,
    MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS, MAX_GAMEPLAY_COMMAND_PAYLOAD_KEY_BYTES,
    MAX_GAMEPLAY_COMMAND_STRING_BYTES,
};
pub use nara_identity::RuntimeEntityReference;
pub use queue::{
    GameplayCommandBatch, GameplayCommandLifecycleError, GameplayCommandLimitKind,
    GameplayCommandQueue, GameplayCommandQueueSettings, GameplayCommandQueueStats,
    GameplayCommandRejection, GameplayCommandSettingsError, MAX_GAMEPLAY_COMMAND_BYTES,
    MAX_GAMEPLAY_COMMAND_FUTURE_TICKS, MAX_GAMEPLAY_COMMAND_RETAINED_BYTES,
    MAX_GAMEPLAY_COMMAND_RETAINED_COMMANDS,
};

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginError,
    RuntimeDriverScope, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeWorldAccessError,
};
use nara_ecs::{
    Res, ResMut,
    schedule::{IntoScheduleConfigs, SystemSet},
};
use nara_input::{ActionOutcomes, InputSet};

pub const GAMEPLAY_COMMAND_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.gameplay.commands");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum GameplayCommandSet {
    /// Lower local semantic action outcomes into commands for the next open tick.
    ///
    /// This is an engine-owned phase, not a public semantic ordering anchor.
    MapLocalActions,
    /// Close and admit the current authoritative tick.
    ///
    /// This is an engine-owned phase, not a public semantic ordering anchor.
    Admit,
    /// Public joinable phase for reading the immutable current-tick command batch.
    ///
    /// The phase runs in `CoreStage::FixedUpdate` after command admission and only while the batch
    /// belongs to the current fixed tick. Members see the admitted batch for the whole phase; an
    /// admission/lifecycle failure reports a domain fault and makes the phase's run condition false.
    /// Ordinary deferred commands from members are applied before [`GameplayCommandSet::Capture`]
    /// begins.
    ///
    /// The batch remains retained and active after this phase. Member errors follow the App's
    /// configured system-error policy; managed-runtime escalation is owned by `nara_app`.
    Consume,
    /// Public joinable phase for replay/debug capture after command consumption.
    ///
    /// The phase runs in `CoreStage::FixedUpdate` only for a current batch and begins after all
    /// [`GameplayCommandSet::Consume`] members and their ordinary deferred commands complete.
    /// Members still see the same retained immutable batch. Their ordinary deferred commands are
    /// applied before engine acknowledgement; after the complete fixed schedule returns, healthy
    /// acknowledgement has retired the batch and released its retained command budget.
    ///
    /// Lifecycle faults make the phase's run condition false. Member errors follow the App's
    /// configured system-error policy; managed-runtime escalation is owned by `nara_app`.
    Capture,
    /// Retire the batch and release its retained budget.
    ///
    /// This engine-owned phase is not a public semantic ordering anchor.
    Acknowledge,
}

impl __RuntimeDriverPort for GameplayCommandQueue {
    type Input = GameplayCommandSubmission;
    type Output = Result<GameplayCommandKey, GameplayCommandRejection>;

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.submit(input)
    }
}

pub fn submit_gameplay_driver_command(
    scope: &mut RuntimeDriverScope<'_>,
    submission: GameplayCommandSubmission,
) -> Result<Result<GameplayCommandKey, GameplayCommandRejection>, RuntimeWorldAccessError> {
    scope.__apply_port::<GameplayCommandQueue>(submission)
}

#[derive(Debug, Clone, Copy)]
pub struct GameplayCommandPlugin {
    settings: GameplayCommandQueueSettings,
}

impl Default for GameplayCommandPlugin {
    fn default() -> Self {
        Self::new(GameplayCommandQueueSettings::default())
    }
}

impl GameplayCommandPlugin {
    #[must_use]
    pub const fn new(settings: GameplayCommandQueueSettings) -> Self {
        Self { settings }
    }

    #[must_use]
    pub const fn settings(self) -> GameplayCommandQueueSettings {
        self.settings
    }
}

impl Plugin for GameplayCommandPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        const DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
            GAMEPLAY_COMMAND_PLUGIN_ID,
            nara_app::PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if !app.world().contains_resource::<ActionCommandMap>() {
            app.insert_resource(ActionCommandMap::default())?;
        }
        if !app.world().contains_resource::<GameplayCommandQueue>() {
            app.insert_resource(GameplayCommandQueue::new(self.settings))?;
        }
        if !app.world().contains_resource::<GameplayCommandBatch>() {
            app.insert_resource(GameplayCommandBatch::new())?;
        }
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                GameplayCommandSet::Admit.in_set(FixedUpdateSet::Prepare),
                GameplayCommandSet::Consume
                    .in_set(FixedUpdateSet::Simulate)
                    .run_if(gameplay_command_batch_is_current),
                GameplayCommandSet::Capture
                    .in_set(FixedUpdateSet::Finalize)
                    .run_if(gameplay_command_batch_is_current),
                GameplayCommandSet::Acknowledge.in_set(FixedUpdateSet::Finalize),
            )
                .chain(),
        )?
        .add_systems(
            CoreStage::PreUpdate,
            map_action_outcomes_to_commands
                .after(InputSet::ResolveActions)
                .in_set(GameplayCommandSet::MapLocalActions),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            admit_gameplay_commands.in_set(GameplayCommandSet::Admit),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            acknowledge_gameplay_commands.in_set(GameplayCommandSet::Acknowledge),
        )?;
        Ok(())
    }
}

fn map_action_outcomes_to_commands(
    command_map: Res<ActionCommandMap>,
    outcomes: Option<Res<ActionOutcomes>>,
    mut queue: ResMut<GameplayCommandQueue>,
    faults: Res<RuntimeFaultReporter>,
) {
    let Some(outcomes) = outcomes else {
        return;
    };

    for outcome in outcomes.as_slice() {
        for binding in
            command_map.matching_bindings(&outcome.action, &outcome.context, outcome.phase)
        {
            if queue
                .submit_local_for_next_tick(binding.command().clone())
                .is_err()
            {
                faults.report(RuntimeFault::engine(
                    RuntimeFaultKind::LocalIntentLoss,
                    "nara.gameplay.local-action",
                ));
            }
        }
    }
}

fn admit_gameplay_commands(
    fixed_time: Res<FixedTime>,
    mut queue: ResMut<GameplayCommandQueue>,
    mut batch: ResMut<GameplayCommandBatch>,
    faults: Res<RuntimeFaultReporter>,
) {
    if queue
        .admit_fixed_tick(fixed_time.tick(), &mut batch)
        .is_err()
    {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::GameplayLifecycle,
            "nara.gameplay.fixed-admit",
        ));
    }
}

fn gameplay_command_batch_is_current(
    fixed_time: Res<FixedTime>,
    batch: Res<GameplayCommandBatch>,
) -> bool {
    batch
        .active_tick()
        .is_some_and(|tick| tick.get() == fixed_time.tick())
}

fn acknowledge_gameplay_commands(
    fixed_time: Res<FixedTime>,
    mut queue: ResMut<GameplayCommandQueue>,
    mut batch: ResMut<GameplayCommandBatch>,
    faults: Res<RuntimeFaultReporter>,
) {
    if queue
        .acknowledge_fixed_tick(fixed_time.tick(), &mut batch)
        .is_err()
    {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::GameplayLifecycle,
            "nara.gameplay.fixed-acknowledge",
        ));
    }
}

pub mod prelude {
    pub use crate::{
        ActionCommandBinding, ActionCommandMap, ActionCommandMapError, GameplayCommandBatch,
        GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandIdError,
        GameplayCommandIngressSource, GameplayCommandKey, GameplayCommandLifecycleError,
        GameplayCommandLimitKind, GameplayCommandPayload, GameplayCommandPayloadError,
        GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandQueueSettings,
        GameplayCommandQueueStats, GameplayCommandRejection, GameplayCommandSet,
        GameplayCommandSettingsError, GameplayCommandSource, GameplayCommandSourceId,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTarget,
        GameplayCommandTargetId, GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
        MAX_ACTION_COMMAND_BINDINGS, RuntimeEntityReference,
    };
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU64, time::Duration};

    use super::*;
    use nara_app::{
        AppRunError, CoreStage, PluginError, RuntimeFaultKind, RuntimeFaultReporter,
        ScheduleCompatibilityError,
    };
    use nara_core::{ByteLimit, ItemLimit};
    use nara_ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};
    use nara_identity::{
        EntityLookup, PersistentRuntimeId, PersistentRuntimeNamespaceId,
        PersistentRuntimeReference, SceneEntityId, SceneInstanceId, SpawnedSceneInstance,
        TombstoneCause, WorldIdentityDomain, WorldIdentityDomainSettings, spawn_identity_entity,
    };
    use nara_input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionPhase, InputPlugin, KeyCode,
    };

    #[derive(Debug, Default, Resource)]
    struct CommandsByTick(Vec<(u64, Vec<GameplayCommandEnvelope>)>);

    #[derive(Debug, Default, Resource)]
    struct BatchLifecycle(Vec<(String, Option<u64>, usize)>);

    fn capture_consumed_commands(
        batch: Res<GameplayCommandBatch>,
        fixed_time: Res<FixedTime>,
        mut observed: ResMut<CommandsByTick>,
    ) {
        observed
            .0
            .push((fixed_time.tick(), batch.commands().to_vec()));
    }

    fn record_consume(batch: Res<GameplayCommandBatch>, mut lifecycle: ResMut<BatchLifecycle>) {
        lifecycle.0.push((
            "consume".to_owned(),
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
        ));
    }

    fn record_capture(batch: Res<GameplayCommandBatch>, mut lifecycle: ResMut<BatchLifecycle>) {
        lifecycle.0.push((
            "capture".to_owned(),
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
        ));
    }

    fn record_after_ack(batch: Res<GameplayCommandBatch>, mut lifecycle: ResMut<BatchLifecycle>) {
        lifecycle.0.push((
            "after_ack".to_owned(),
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
        ));
    }

    fn command(command_type: &str) -> GameplayCommandDraft {
        GameplayCommandDraft::new(GameplayCommandTypeId::new(command_type).unwrap())
    }

    fn tick(value: u64) -> GameplayCommandTick {
        GameplayCommandTick::new(value).unwrap()
    }

    fn sequence(value: u64) -> GameplayCommandSourceSequence {
        GameplayCommandSourceSequence::new(value).unwrap()
    }

    fn scene_entity_id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn persistent_reference(value: &str) -> PersistentRuntimeReference {
        PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("runtime").unwrap(),
            PersistentRuntimeId::parse_str(value).unwrap(),
        )
    }

    fn world_with_identity_domain() -> World {
        let mut world = World::new();
        let settings = WorldIdentityDomainSettings::new(
            ItemLimit::new(64).unwrap(),
            ItemLimit::new(16).unwrap(),
        )
        .unwrap();
        let domain = WorldIdentityDomain::new(&world, settings).unwrap();
        world.insert_resource(domain);
        world
    }

    fn with_identity_domain<T>(
        world: &mut World,
        mutate: impl FnOnce(&World, &mut WorldIdentityDomain) -> T,
    ) -> T {
        let mut domain = world.remove_resource::<WorldIdentityDomain>().unwrap();
        let result = mutate(world, &mut domain);
        world.insert_resource(domain);
        result
    }

    fn register_scene_entity(
        world: &mut World,
        entity_id: &SceneEntityId,
    ) -> (SpawnedSceneInstance, nara_ecs::Entity) {
        let token = spawn_identity_entity(world).unwrap();
        let instance = with_identity_domain(world, |world, domain| {
            domain
                .register_new_scene_instance(world, [(entity_id.clone(), token)])
                .unwrap()
        });
        (instance, token.entity())
    }

    fn submission(
        tick_value: u64,
        source: GameplayCommandIngressSource,
        sequence_value: u64,
        command_type: &str,
    ) -> GameplayCommandSubmission {
        GameplayCommandSubmission::new(
            tick(tick_value),
            source,
            sequence(sequence_value),
            command(command_type),
        )
    }

    fn settings(
        retained_commands: usize,
        retained_bytes: usize,
        command_bytes: usize,
        payload_items: usize,
        payload_bytes: usize,
        future_ticks: u64,
    ) -> GameplayCommandQueueSettings {
        GameplayCommandQueueSettings::new(
            ItemLimit::new(retained_commands).unwrap(),
            ByteLimit::new(retained_bytes).unwrap(),
            ByteLimit::new(command_bytes).unwrap(),
            ItemLimit::new(payload_items).unwrap(),
            ByteLimit::new(payload_bytes).unwrap(),
            NonZeroU64::new(future_ticks).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn local_action_survives_zero_tick_frame_and_is_consumed_once_across_three_ticks() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(CommandsByTick::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                capture_consumed_commands.in_set(GameplayCommandSet::Consume),
            )
            .unwrap();

        let action = ActionId::new("jump").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Space));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("player.jump").unwrap(),
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Space)
            .unwrap();

        app.run_once(Duration::ZERO).unwrap();
        assert_eq!(
            app.world()
                .resource::<GameplayCommandQueue>()
                .stats()
                .pending_commands,
            1
        );
        app.run_once(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();

        let observed = &app.world().resource::<CommandsByTick>().0;
        assert_eq!(observed.len(), 3);
        assert_eq!(observed[0].0, 1);
        assert_eq!(observed[0].1.len(), 1);
        assert_eq!(observed[0].1[0].command_type().as_str(), "player.jump");
        assert_eq!(
            observed[0].1[0].source(),
            &GameplayCommandSource::LocalAction
        );
        assert!(observed[1].1.is_empty());
        assert!(observed[2].1.is_empty());
        assert!(app.world().resource::<GameplayCommandQueue>().is_idle());
        assert_eq!(
            app.world().resource::<GameplayCommandBatch>().active_tick(),
            None
        );
    }

    #[test]
    fn multiple_zero_tick_frames_preserve_one_local_source_sequence() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(CommandsByTick::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                capture_consumed_commands.in_set(GameplayCommandSet::Consume),
            )
            .unwrap();

        let action = ActionId::new("interact").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter));
        {
            let mut command_map = app.world_mut().unwrap().resource_mut::<ActionCommandMap>();
            command_map
                .bind(ActionCommandBinding::new(
                    action.clone(),
                    ActionPhase::Started,
                    GameplayCommandTypeId::new("interact.started").unwrap(),
                ))
                .unwrap();
            command_map
                .bind(ActionCommandBinding::new(
                    action,
                    ActionPhase::Released,
                    GameplayCommandTypeId::new("interact.released").unwrap(),
                ))
                .unwrap();
        }

        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter)
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .release(KeyCode::Enter)
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter)
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let commands = &app.world().resource::<CommandsByTick>().0[0].1;
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands
                .iter()
                .map(|command| command.source_sequence().get())
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            commands
                .iter()
                .map(|command| command.command_type().as_str())
                .collect::<Vec<_>>(),
            ["interact.started", "interact.released", "interact.started"]
        );
    }

    #[test]
    fn action_bridge_preserves_target_payload_and_context() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(CommandsByTick::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                capture_consumed_commands.in_set(GameplayCommandSet::Consume),
            )
            .unwrap();

        let context = ActionContext::new("menu").unwrap();
        let action = ActionId::new("select").unwrap();
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("slot", GameplayCommandValue::I64(2))
            .unwrap();
        let target = GameplayCommandTarget::named("player").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter).with_context(context.clone()));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(
                ActionCommandBinding::new(
                    action.clone(),
                    ActionPhase::Started,
                    GameplayCommandTypeId::new("ui.select").unwrap(),
                )
                .with_context(context)
                .with_target(target.clone())
                .with_payload(payload.clone()),
            )
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("gameplay.should_not_match").unwrap(),
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter)
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let commands = &app.world().resource::<CommandsByTick>().0[0].1;
        assert_eq!(commands.len(), 1);
        let envelope = &commands[0];
        assert_eq!(envelope.command_type().as_str(), "ui.select");
        assert_eq!(envelope.target(), Some(&target));
        assert_eq!(envelope.payload(), &payload);
    }

    #[test]
    fn action_bridge_filters_by_phase() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(CommandsByTick::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                capture_consumed_commands.in_set(GameplayCommandSet::Consume),
            )
            .unwrap();

        let action = ActionId::new("cancel").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Escape));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Released,
                GameplayCommandTypeId::new("cancel.released").unwrap(),
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Escape)
            .unwrap();
        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .release(KeyCode::Escape)
            .unwrap();
        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let observed = &app.world().resource::<CommandsByTick>().0;
        assert!(observed[0].1.is_empty());
        assert_eq!(observed[1].1.len(), 1);
        assert_eq!(observed[1].1[0].command_type().as_str(), "cancel.released");
    }

    #[test]
    fn action_command_map_rejects_the_first_binding_past_its_hard_limit() {
        let binding = ActionCommandBinding::new(
            ActionId::new("bounded").unwrap(),
            ActionPhase::Started,
            GameplayCommandTypeId::new("bounded.command").unwrap(),
        );
        let mut command_map = ActionCommandMap::default();
        for _ in 0..MAX_ACTION_COMMAND_BINDINGS {
            command_map.bind(binding.clone()).unwrap();
        }
        let before = command_map.bindings().len();

        assert_eq!(
            command_map.bind(binding),
            Err(ActionCommandMapError::BindingLimit {
                requested: MAX_ACTION_COMMAND_BINDINGS + 1,
                maximum: MAX_ACTION_COMMAND_BINDINGS,
            })
        );
        assert_eq!(command_map.bindings().len(), before);
    }

    #[test]
    fn action_command_map_rejects_runtime_entity_targets_without_mutation() {
        let entity_targets = [
            RuntimeEntityReference::scene(
                SceneInstanceId::new(1).unwrap(),
                scene_entity_id("player"),
            ),
            RuntimeEntityReference::persistent(persistent_reference(
                "a1b2c3d4-e5f6-47a8-90ab-1234567890cd",
            )),
        ];
        let mut command_map = ActionCommandMap::default();

        for target in entity_targets {
            let binding = ActionCommandBinding::new(
                ActionId::new("select").unwrap(),
                ActionPhase::Started,
                GameplayCommandTypeId::new("select.entity").unwrap(),
            )
            .with_target(GameplayCommandTarget::Entity(target));

            assert_eq!(
                command_map.bind(binding),
                Err(ActionCommandMapError::RuntimeEntityTarget)
            );
            assert!(command_map.bindings().is_empty());
        }
    }

    #[test]
    fn admitted_order_is_independent_of_arrival_order() {
        let mut external_payload = GameplayCommandPayload::new();
        external_payload
            .insert("axis", GameplayCommandValue::I64(2))
            .unwrap();
        let submissions = [
            GameplayCommandSubmission::new(
                tick(1),
                GameplayCommandIngressSource::external("server-b").unwrap(),
                sequence(2),
                command("external.b")
                    .with_target(GameplayCommandTarget::named("player-b").unwrap())
                    .with_payload(external_payload),
            ),
            submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                2,
                "test.second",
            ),
            submission(
                1,
                GameplayCommandIngressSource::ai("agent-a").unwrap(),
                1,
                "ai",
            ),
            submission(
                1,
                GameplayCommandIngressSource::external("server-a").unwrap(),
                1,
                "external.a",
            ),
            submission(
                1,
                GameplayCommandIngressSource::replay("stream-a").unwrap(),
                1,
                "replay",
            ),
            submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "test.first",
            ),
        ];
        let mut forward = GameplayCommandQueue::default();
        let mut reverse = GameplayCommandQueue::default();
        forward
            .submit_local_for_next_tick(command("local"))
            .unwrap();
        for item in &submissions {
            forward.submit(item.clone()).unwrap();
        }
        for item in submissions.iter().rev() {
            reverse.submit(item.clone()).unwrap();
        }
        reverse
            .submit_local_for_next_tick(command("local"))
            .unwrap();
        let mut forward_batch = GameplayCommandBatch::new();
        let mut reverse_batch = GameplayCommandBatch::new();
        forward.admit_tick(tick(1), &mut forward_batch).unwrap();
        reverse.admit_tick(tick(1), &mut reverse_batch).unwrap();

        assert_eq!(forward_batch.commands(), reverse_batch.commands());

        let forward_keys = forward_batch
            .iter()
            .map(|envelope| envelope.key().clone())
            .collect::<Vec<_>>();
        let reverse_keys = reverse_batch
            .iter()
            .map(|envelope| envelope.key().clone())
            .collect::<Vec<_>>();
        assert_eq!(forward_keys, reverse_keys);
        assert_eq!(
            forward_keys
                .iter()
                .map(|key| (key.source().clone(), key.source_sequence().get()))
                .collect::<Vec<_>>(),
            [
                (GameplayCommandSource::LocalAction, 1),
                (GameplayCommandSource::test("driver").unwrap(), 1),
                (GameplayCommandSource::test("driver").unwrap(), 2),
                (GameplayCommandSource::replay("stream-a").unwrap(), 1),
                (GameplayCommandSource::ai("agent-a").unwrap(), 1),
                (GameplayCommandSource::external("server-a").unwrap(), 1),
                (GameplayCommandSource::external("server-b").unwrap(), 2),
            ]
        );
    }

    #[test]
    fn duplicate_key_is_rejected_without_replacing_the_first_command() {
        let mut queue = GameplayCommandQueue::default();
        let source = GameplayCommandIngressSource::external("server").unwrap();
        queue
            .submit(submission(1, source.clone(), 1, "first"))
            .unwrap();
        let before = queue.stats();

        assert_eq!(
            queue.submit(submission(1, source, 1, "replacement")),
            Err(GameplayCommandRejection::Duplicate)
        );
        let after = queue.stats();
        assert_eq!(after.retained_commands, before.retained_commands);
        assert_eq!(after.retained_bytes, before.retained_bytes);

        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();
        assert_eq!(batch.commands()[0].command_type().as_str(), "first");
    }

    #[test]
    fn source_sequence_may_be_reused_on_a_different_tick() {
        let mut queue = GameplayCommandQueue::default();
        let source = GameplayCommandIngressSource::replay("stream").unwrap();
        queue
            .submit(submission(1, source.clone(), 7, "tick.one"))
            .unwrap();
        queue.submit(submission(2, source, 7, "tick.two")).unwrap();

        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();
        assert_eq!(batch.commands()[0].source_sequence().get(), 7);
        queue.acknowledge_tick(tick(1), &mut batch).unwrap();
        queue.admit_tick(tick(2), &mut batch).unwrap();
        assert_eq!(batch.commands()[0].source_sequence().get(), 7);
        assert_eq!(batch.commands()[0].command_type().as_str(), "tick.two");
    }

    #[test]
    fn late_and_future_window_rejections_use_closed_tick_watermark() {
        let mut queue = GameplayCommandQueue::new(settings(8, 4_096, 1_024, 8, 512, 2));
        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();
        let before_late = queue.stats();

        assert!(matches!(
            queue.submit(submission(
                1,
                GameplayCommandIngressSource::test("late").unwrap(),
                1,
                "late"
            )),
            Err(GameplayCommandRejection::Late { .. })
        ));
        let after_late = queue.stats();
        assert_eq!(after_late.pending_commands, before_late.pending_commands);
        assert_eq!(after_late.active_commands, before_late.active_commands);
        assert_eq!(after_late.retained_commands, before_late.retained_commands);
        assert_eq!(after_late.pending_bytes, before_late.pending_bytes);
        assert_eq!(after_late.active_bytes, before_late.active_bytes);
        assert_eq!(after_late.retained_bytes, before_late.retained_bytes);
        assert_eq!(
            after_late.closed_through_tick,
            before_late.closed_through_tick
        );
        assert_eq!(
            after_late.acknowledged_through_tick,
            before_late.acknowledged_through_tick
        );
        assert_eq!(after_late.furthest_pending_tick, None);
        assert!(
            queue
                .submit(submission(
                    3,
                    GameplayCommandIngressSource::test("boundary").unwrap(),
                    1,
                    "boundary"
                ))
                .is_ok()
        );
        let before_future = queue.stats();
        assert!(matches!(
            queue.submit(submission(
                4,
                GameplayCommandIngressSource::test("future").unwrap(),
                1,
                "future"
            )),
            Err(GameplayCommandRejection::TooFarFuture { .. })
        ));
        let after_future = queue.stats();
        assert_eq!(
            after_future.pending_commands,
            before_future.pending_commands
        );
        assert_eq!(after_future.active_commands, before_future.active_commands);
        assert_eq!(
            after_future.retained_commands,
            before_future.retained_commands
        );
        assert_eq!(after_future.pending_bytes, before_future.pending_bytes);
        assert_eq!(after_future.active_bytes, before_future.active_bytes);
        assert_eq!(after_future.retained_bytes, before_future.retained_bytes);
        assert_eq!(
            after_future.closed_through_tick,
            before_future.closed_through_tick
        );
        assert_eq!(
            after_future.acknowledged_through_tick,
            before_future.acknowledged_through_tick
        );
        assert_eq!(after_future.furthest_pending_tick, Some(3));
    }

    #[test]
    fn active_and_pending_commands_share_the_retained_item_budget() {
        let mut queue = GameplayCommandQueue::new(settings(2, 4_096, 1_024, 8, 512, 4));
        queue
            .submit(submission(
                1,
                GameplayCommandIngressSource::test("tick-one").unwrap(),
                1,
                "one",
            ))
            .unwrap();
        queue
            .submit(submission(
                2,
                GameplayCommandIngressSource::test("tick-two").unwrap(),
                1,
                "two",
            ))
            .unwrap();
        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();

        assert_eq!(queue.stats().active_commands, 1);
        assert_eq!(queue.stats().pending_commands, 1);
        assert!(matches!(
            queue.submit(submission(
                2,
                GameplayCommandIngressSource::test("another").unwrap(),
                1,
                "full"
            )),
            Err(GameplayCommandRejection::RetainedItemLimit { .. })
        ));

        queue.acknowledge_tick(tick(1), &mut batch).unwrap();
        assert!(
            queue
                .submit(submission(
                    2,
                    GameplayCommandIngressSource::test("another").unwrap(),
                    1,
                    "after-ack"
                ))
                .is_ok()
        );
    }

    #[test]
    fn active_and_pending_commands_share_the_retained_byte_budget() {
        let first = submission(
            1,
            GameplayCommandIngressSource::test("one").unwrap(),
            1,
            "one",
        );
        let second = submission(
            2,
            GameplayCommandIngressSource::test("two").unwrap(),
            1,
            "two",
        );
        let replacement = submission(
            2,
            GameplayCommandIngressSource::test("new").unwrap(),
            1,
            "new",
        );
        let command_bytes = first.logical_bytes().unwrap();
        assert_eq!(second.logical_bytes(), Some(command_bytes));
        assert_eq!(replacement.logical_bytes(), Some(command_bytes));
        let mut queue =
            GameplayCommandQueue::new(settings(3, command_bytes * 2, command_bytes, 1, 1, 4));
        queue.submit(first).unwrap();
        queue.submit(second).unwrap();
        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();

        assert_eq!(queue.stats().active_bytes, command_bytes);
        assert_eq!(queue.stats().pending_bytes, command_bytes);
        assert!(matches!(
            queue.submit(replacement.clone()),
            Err(GameplayCommandRejection::RetainedByteLimit { .. })
        ));

        queue.acknowledge_tick(tick(1), &mut batch).unwrap();
        queue.submit(replacement).unwrap();
    }

    #[test]
    fn acknowledging_one_tick_retains_future_commands() {
        let mut queue = GameplayCommandQueue::default();
        queue
            .submit(submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "one",
            ))
            .unwrap();
        queue
            .submit(submission(
                2,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "two",
            ))
            .unwrap();
        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();
        queue.acknowledge_tick(tick(1), &mut batch).unwrap();
        assert_eq!(queue.stats().pending_commands, 1);

        queue.admit_tick(tick(2), &mut batch).unwrap();
        assert_eq!(batch.commands().len(), 1);
        assert_eq!(batch.commands()[0].command_type().as_str(), "two");
    }

    #[test]
    fn payload_nonfinite_and_replacement_overflow_are_atomic() {
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("value", GameplayCommandValue::I64(7))
            .unwrap();
        let before = payload.clone();
        for (key, value) in [
            ("nan", f64::NAN),
            ("positive_infinity", f64::INFINITY),
            ("negative_infinity", f64::NEG_INFINITY),
        ] {
            assert_eq!(
                payload.insert(key, GameplayCommandValue::F64(value)),
                Err(GameplayCommandPayloadError::NonFiniteFloat)
            );
            assert_eq!(payload, before);
        }

        let oversized = "x".repeat(MAX_GAMEPLAY_COMMAND_STRING_BYTES + 1);
        assert!(matches!(
            payload.insert("value", GameplayCommandValue::String(oversized)),
            Err(GameplayCommandPayloadError::StringTooLong { .. })
        ));
        assert_eq!(payload, before);
    }

    #[test]
    fn configured_payload_limits_accept_the_boundary_and_reject_without_retention() {
        let mut one_field = GameplayCommandPayload::new();
        one_field
            .insert("a", GameplayCommandValue::String("x".to_owned()))
            .unwrap();
        let payload_bytes = one_field.logical_bytes();
        let accepted = GameplayCommandSubmission::new(
            tick(1),
            GameplayCommandIngressSource::test("driver").unwrap(),
            sequence(1),
            command("payload").with_payload(one_field.clone()),
        );
        let mut exact = GameplayCommandQueue::new(settings(2, 4_096, 1_024, 1, payload_bytes, 2));
        exact.submit(accepted).unwrap();

        let mut two_fields = one_field;
        two_fields
            .insert("b", GameplayCommandValue::Bool(true))
            .unwrap();
        let oversized = GameplayCommandSubmission::new(
            tick(1),
            GameplayCommandIngressSource::test("other").unwrap(),
            sequence(1),
            command("payload").with_payload(two_fields.clone()),
        );
        let mut item_limited =
            GameplayCommandQueue::new(settings(2, 4_096, 1_024, 1, two_fields.logical_bytes(), 2));
        assert!(matches!(
            item_limited.submit(oversized.clone()),
            Err(GameplayCommandRejection::PayloadItemLimit { .. })
        ));
        assert_eq!(item_limited.stats().retained_commands, 0);

        let mut byte_limited =
            GameplayCommandQueue::new(settings(2, 4_096, 1_024, 4, payload_bytes, 2));
        assert!(matches!(
            byte_limited.submit(oversized),
            Err(GameplayCommandRejection::PayloadByteLimit { .. })
        ));
        assert_eq!(byte_limited.stats().retained_bytes, 0);
    }

    #[test]
    fn per_command_byte_limit_rejects_before_queue_retention() {
        let candidate = submission(
            1,
            GameplayCommandIngressSource::test("driver").unwrap(),
            1,
            "oversized",
        );
        let logical_bytes = candidate.logical_bytes().unwrap();
        let mut queue =
            GameplayCommandQueue::new(settings(1, logical_bytes, logical_bytes - 1, 1, 1, 2));

        assert_eq!(
            queue.submit(candidate),
            Err(GameplayCommandRejection::CommandByteLimit {
                requested: logical_bytes,
                maximum: logical_bytes - 1,
            })
        );
        let stats = queue.stats();
        assert_eq!(stats.pending_commands, 0);
        assert_eq!(stats.active_commands, 0);
        assert_eq!(stats.retained_commands, 0);
        assert_eq!(stats.retained_bytes, 0);
        assert_eq!(stats.rejected_invalid, 1);
    }

    #[test]
    fn queue_settings_reject_every_hard_ceiling_and_cross_limit_invariant() {
        let defaults = GameplayCommandQueueSettings::default();
        let cases = [
            (
                GameplayCommandQueueSettings::new(
                    ItemLimit::new(MAX_GAMEPLAY_COMMAND_RETAINED_COMMANDS + 1).unwrap(),
                    defaults.retained_bytes(),
                    defaults.command_bytes(),
                    defaults.payload_items(),
                    defaults.payload_bytes(),
                    defaults.future_ticks(),
                ),
                GameplayCommandLimitKind::RetainedCommands,
                MAX_GAMEPLAY_COMMAND_RETAINED_COMMANDS,
            ),
            (
                GameplayCommandQueueSettings::new(
                    defaults.retained_commands(),
                    ByteLimit::new(MAX_GAMEPLAY_COMMAND_RETAINED_BYTES + 1).unwrap(),
                    defaults.command_bytes(),
                    defaults.payload_items(),
                    defaults.payload_bytes(),
                    defaults.future_ticks(),
                ),
                GameplayCommandLimitKind::RetainedBytes,
                MAX_GAMEPLAY_COMMAND_RETAINED_BYTES,
            ),
            (
                GameplayCommandQueueSettings::new(
                    defaults.retained_commands(),
                    defaults.retained_bytes(),
                    ByteLimit::new(MAX_GAMEPLAY_COMMAND_BYTES + 1).unwrap(),
                    defaults.payload_items(),
                    defaults.payload_bytes(),
                    defaults.future_ticks(),
                ),
                GameplayCommandLimitKind::CommandBytes,
                MAX_GAMEPLAY_COMMAND_BYTES,
            ),
            (
                GameplayCommandQueueSettings::new(
                    defaults.retained_commands(),
                    defaults.retained_bytes(),
                    defaults.command_bytes(),
                    ItemLimit::new(MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS + 1).unwrap(),
                    defaults.payload_bytes(),
                    defaults.future_ticks(),
                ),
                GameplayCommandLimitKind::PayloadItems,
                MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS,
            ),
            (
                GameplayCommandQueueSettings::new(
                    defaults.retained_commands(),
                    defaults.retained_bytes(),
                    defaults.command_bytes(),
                    defaults.payload_items(),
                    ByteLimit::new(MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES + 1).unwrap(),
                    defaults.future_ticks(),
                ),
                GameplayCommandLimitKind::PayloadBytes,
                MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES,
            ),
            (
                GameplayCommandQueueSettings::new(
                    defaults.retained_commands(),
                    defaults.retained_bytes(),
                    defaults.command_bytes(),
                    defaults.payload_items(),
                    defaults.payload_bytes(),
                    NonZeroU64::new(MAX_GAMEPLAY_COMMAND_FUTURE_TICKS + 1).unwrap(),
                ),
                GameplayCommandLimitKind::FutureTicks,
                usize::try_from(MAX_GAMEPLAY_COMMAND_FUTURE_TICKS).unwrap(),
            ),
        ];

        for (result, kind, maximum) in cases {
            assert!(matches!(
                result,
                Err(GameplayCommandSettingsError::LimitTooLarge {
                    kind: actual_kind,
                    maximum: actual_maximum,
                    ..
                }) if actual_kind == kind && actual_maximum == maximum
            ));
        }

        assert_eq!(
            GameplayCommandQueueSettings::new(
                ItemLimit::new(1).unwrap(),
                ByteLimit::new(10).unwrap(),
                ByteLimit::new(11).unwrap(),
                ItemLimit::new(1).unwrap(),
                ByteLimit::new(1).unwrap(),
                NonZeroU64::new(1).unwrap(),
            ),
            Err(GameplayCommandSettingsError::CommandExceedsRetainedBytes)
        );
        assert_eq!(
            GameplayCommandQueueSettings::new(
                ItemLimit::new(1).unwrap(),
                ByteLimit::new(100).unwrap(),
                ByteLimit::new(10).unwrap(),
                ItemLimit::new(1).unwrap(),
                ByteLimit::new(11).unwrap(),
                NonZeroU64::new(1).unwrap(),
            ),
            Err(GameplayCommandSettingsError::PayloadExceedsCommandBytes)
        );
    }

    #[test]
    fn queue_byte_limit_accepts_exact_boundary_and_rejects_one_more() {
        let first = submission(
            1,
            GameplayCommandIngressSource::test("driver").unwrap(),
            1,
            "first",
        );
        let exact_bytes = first.logical_bytes().unwrap();
        let mut queue = GameplayCommandQueue::new(settings(2, exact_bytes, exact_bytes, 1, 1, 2));
        queue.submit(first).unwrap();
        let before = queue.stats();

        assert!(matches!(
            queue.submit(submission(
                2,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "x"
            )),
            Err(GameplayCommandRejection::RetainedByteLimit { .. })
        ));
        let after = queue.stats();
        assert_eq!(after.retained_commands, before.retained_commands);
        assert_eq!(after.retained_bytes, before.retained_bytes);
    }

    #[test]
    fn tick_and_local_sequence_overflow_are_typed_rejections() {
        let mut queue = GameplayCommandQueue::default();
        queue.set_watermarks_for_test(u64::MAX);
        assert_eq!(
            queue.submit_local_for_next_tick(command("overflow")),
            Err(GameplayCommandRejection::TickExhausted)
        );

        let mut queue = GameplayCommandQueue::default();
        queue.set_local_sequence_for_test(u64::MAX);
        assert_eq!(
            queue.submit_local_for_next_tick(command("overflow")),
            Err(GameplayCommandRejection::SourceSequenceExhausted)
        );
    }

    #[test]
    fn engine_owned_local_intent_loss_reaches_the_runtime_fault_reporter() {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();

        let action = ActionId::new("interact").unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(action.clone(), KeyCode::Enter));
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action,
                ActionPhase::Started,
                GameplayCommandTypeId::new("interact.started").unwrap(),
            ))
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .set_local_sequence_for_test(u64::MAX);
        app.world_mut()
            .unwrap()
            .resource_mut::<nara_input::ButtonInput<KeyCode>>()
            .press(KeyCode::Enter)
            .unwrap();

        let error = app
            .run_once(Duration::ZERO)
            .expect_err("local intent loss must cross the direct runtime boundary");
        assert_eq!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::LocalIntentLoss,
                fault_source: "nara.gameplay.local-action",
            }
        );

        let fault = app
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .expect("local intent loss should report a runtime fault");
        assert_eq!(fault.kind(), RuntimeFaultKind::LocalIntentLoss);
        assert_eq!(fault.source(), "nara.gameplay.local-action");
    }

    #[test]
    fn lifecycle_faults_fail_closed_and_hide_an_ambiguous_batch() {
        let mut queue = GameplayCommandQueue::default();
        queue
            .submit(submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "one",
            ))
            .unwrap();
        let mut batch = GameplayCommandBatch::new();
        let before = queue.stats();

        assert_eq!(
            queue.admit_fixed_tick(2, &mut batch),
            Err(GameplayCommandLifecycleError::UnexpectedTick {
                expected: 1,
                actual: 2,
            })
        );
        assert_eq!(queue.stats().retained_commands, before.retained_commands);
        assert_eq!(queue.stats().closed_through_tick, 0);
        assert_eq!(batch.active_tick(), None);
        assert_eq!(
            queue.submit(submission(
                1,
                GameplayCommandIngressSource::test("another").unwrap(),
                1,
                "blocked",
            )),
            Err(GameplayCommandRejection::LifecycleFaulted)
        );
        assert_eq!(queue.stats().rejected_lifecycle, 1);
        assert_eq!(queue.stats().rejected_invalid, 0);
        assert_eq!(
            queue.admit_fixed_tick(1, &mut batch),
            Err(GameplayCommandLifecycleError::Poisoned)
        );
        assert_eq!(
            queue.last_lifecycle_error(),
            Some(&GameplayCommandLifecycleError::UnexpectedTick {
                expected: 1,
                actual: 2,
            })
        );

        let mut active_queue = GameplayCommandQueue::default();
        active_queue
            .submit(submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "one",
            ))
            .unwrap();
        let mut active_batch = GameplayCommandBatch::new();
        active_queue.admit_fixed_tick(1, &mut active_batch).unwrap();
        let active = active_queue.stats();
        assert_eq!(active_batch.len(), 1);
        assert_eq!(
            active_queue.admit_fixed_tick(2, &mut active_batch),
            Err(GameplayCommandLifecycleError::BatchAlreadyActive)
        );
        assert_eq!(
            active_queue.stats().retained_commands,
            active.retained_commands
        );
        assert_eq!(active_queue.stats().active_commands, 0);
        assert_eq!(
            active_queue.stats().quarantined_commands,
            active.active_commands
        );
        assert_eq!(active_queue.quarantined_commands().len(), 1);
        assert_eq!(
            active_queue.quarantined_commands()[0]
                .command_type()
                .as_str(),
            "one"
        );
        assert_eq!(active_batch.active_tick(), None);
        assert!(active_batch.commands().is_empty());
        assert_eq!(
            active_queue.acknowledge_fixed_tick(2, &mut active_batch),
            Err(GameplayCommandLifecycleError::Poisoned)
        );
        assert_eq!(active_batch.active_tick(), None);
        assert!(active_batch.commands().is_empty());
        assert_eq!(active_queue.stats().acknowledged_through_tick, 0);
        assert_eq!(
            active_queue.last_lifecycle_error(),
            Some(&GameplayCommandLifecycleError::BatchAlreadyActive)
        );
    }

    #[test]
    fn seal_rejects_moving_consume_across_ordered_fixed_phases() {
        fn moved_consumer() {}

        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.configure_sets(
            CoreStage::FixedUpdate,
            GameplayCommandSet::Consume.in_set(FixedUpdateSet::Finalize),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            moved_consumer.in_set(GameplayCommandSet::Consume),
        )
        .unwrap();

        let error = app.seal().unwrap_err();

        assert!(matches!(
            error,
            PluginError::ScheduleCompatibility(ScheduleCompatibilityError::BuildFailed {
                schedule: CoreStage::FixedUpdate,
                ..
            })
        ));
    }

    #[test]
    fn fixed_schedule_hides_a_stale_active_batch_from_consumers() {
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(CommandsByTick::default())
            .unwrap()
            .insert_resource(BatchLifecycle::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                capture_consumed_commands.in_set(GameplayCommandSet::Consume),
            )
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                record_capture.in_set(GameplayCommandSet::Capture),
            )
            .unwrap();

        let mut stale_batch = app
            .world_mut()
            .unwrap()
            .remove_resource::<GameplayCommandBatch>()
            .unwrap();
        {
            let mut queue = app
                .world_mut()
                .unwrap()
                .resource_mut::<GameplayCommandQueue>();
            queue
                .submit(submission(
                    1,
                    GameplayCommandIngressSource::test("driver").unwrap(),
                    1,
                    "stale",
                ))
                .unwrap();
            queue.admit_tick(tick(1), &mut stale_batch).unwrap();
        }
        assert_eq!(stale_batch.len(), 1);
        app.world_mut().unwrap().insert_resource(stale_batch);

        let error = app
            .run_once(FixedTime::DEFAULT_TIMESTEP)
            .expect_err("the stale batch must cross the direct runtime boundary");
        assert_eq!(
            error,
            AppRunError::DirectRuntime {
                kind: RuntimeFaultKind::GameplayLifecycle,
                fault_source: "nara.gameplay.fixed-admit",
            }
        );

        assert!(app.world().resource::<CommandsByTick>().0.is_empty());
        assert!(app.world().resource::<BatchLifecycle>().0.is_empty());
        assert_eq!(
            app.world().resource::<GameplayCommandBatch>().active_tick(),
            None
        );
        let stats = app.world().resource::<GameplayCommandQueue>().stats();
        assert_eq!(stats.active_commands, 0);
        assert_eq!(stats.quarantined_commands, 1);
        assert_eq!(stats.retained_commands, 1);
        assert_eq!(stats.lifecycle_faults, 2);
        assert_eq!(
            app.world()
                .resource::<GameplayCommandQueue>()
                .quarantined_commands()[0]
                .command_type()
                .as_str(),
            "stale"
        );
        assert_eq!(
            app.world()
                .resource::<GameplayCommandQueue>()
                .last_lifecycle_error(),
            Some(&GameplayCommandLifecycleError::BatchAlreadyActive)
        );
        let fault = app
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .expect("a lifecycle invariant should report a runtime fault");
        assert_eq!(fault.kind(), RuntimeFaultKind::GameplayLifecycle);
        assert_eq!(fault.source(), "nara.gameplay.fixed-admit");
    }

    #[test]
    fn capture_runs_before_engine_acknowledgement() {
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(BatchLifecycle::default())
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                record_consume.in_set(GameplayCommandSet::Consume),
            )
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                record_capture.in_set(GameplayCommandSet::Capture),
            )
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                record_after_ack.after(GameplayCommandSet::Acknowledge),
            )
            .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(submission(
                1,
                GameplayCommandIngressSource::test("driver").unwrap(),
                1,
                "once",
            ))
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<BatchLifecycle>().0,
            [
                ("consume".to_owned(), Some(1), 1),
                ("capture".to_owned(), Some(1), 1),
                ("after_ack".to_owned(), None, 0),
            ]
        );
    }

    #[test]
    fn entity_command_target_logical_bytes_cover_the_nested_identity_shape() {
        assert!(PersistentRuntimeId::parse_str("not-a-uuid").is_err());
        let named = GameplayCommandTarget::named("player").unwrap();
        let scene = GameplayCommandTarget::Entity(RuntimeEntityReference::scene(
            SceneInstanceId::new(7).unwrap(),
            scene_entity_id("scene/player"),
        ));
        let persistent = GameplayCommandTarget::Entity(RuntimeEntityReference::persistent(
            persistent_reference("A1B2C3D4-E5F6-47A8-90AB-1234567890CD"),
        ));

        assert_eq!(named.logical_bytes(), Some(1 + "player".len()));
        assert_eq!(
            scene.logical_bytes(),
            Some(1 + 1 + size_of::<u64>() + "scene/player".len())
        );
        assert_eq!(
            persistent.logical_bytes(),
            Some(1 + 1 + "runtime".len() + 36)
        );
    }

    #[test]
    fn entity_command_target_preserves_current_world_lookup_outcomes() {
        let player = scene_entity_id("player");
        let mut world = world_with_identity_domain();
        let (resolved_instance, resolved_entity) = register_scene_entity(&mut world, &player);
        let resolved_target =
            GameplayCommandTarget::Entity(resolved_instance.runtime_reference(&player).unwrap());
        assert_eq!(
            resolved_target.resolve_entity(&world),
            Some(EntityLookup::Resolved(resolved_entity))
        );

        let missing_target = GameplayCommandTarget::Entity(RuntimeEntityReference::scene(
            SceneInstanceId::new(99).unwrap(),
            player.clone(),
        ));
        assert_eq!(
            missing_target.resolve_entity(&world),
            Some(EntityLookup::Missing)
        );

        with_identity_domain(&mut world, |world, domain| {
            domain
                .retire_scene_instance(world, &resolved_instance, TombstoneCause::Unloaded)
                .unwrap();
        });
        assert!(matches!(
            resolved_target.resolve_entity(&world),
            Some(EntityLookup::Tombstoned(Some(tombstone)))
                if tombstone.cause() == TombstoneCause::Unloaded
        ));

        let (stale_instance, stale_entity) = register_scene_entity(&mut world, &player);
        let stale_target =
            GameplayCommandTarget::Entity(stale_instance.runtime_reference(&player).unwrap());
        assert!(world.despawn(stale_entity));
        assert_eq!(
            stale_target.resolve_entity(&world),
            Some(EntityLookup::StaleRegistration)
        );

        assert_eq!(
            stale_target.resolve_entity(&World::new()),
            Some(EntityLookup::DomainUnavailable)
        );
        assert_eq!(
            GameplayCommandTarget::named("player")
                .unwrap()
                .resolve_entity(&world),
            None
        );
    }

    #[test]
    fn entity_command_target_resolves_after_same_timeline_restore() {
        let player = scene_entity_id("player");
        let mut source_world = world_with_identity_domain();
        let (source_instance, source_entity) = register_scene_entity(&mut source_world, &player);
        let target =
            GameplayCommandTarget::Entity(source_instance.runtime_reference(&player).unwrap());
        let snapshot = source_world
            .resource::<WorldIdentityDomain>()
            .scene_identity_snapshot(&source_world, &source_instance)
            .unwrap();

        let mut restored_world = world_with_identity_domain();
        let _occupied = restored_world.spawn_empty().id();
        let restored_token = spawn_identity_entity(&mut restored_world).unwrap();
        assert_ne!(source_entity.to_bits(), restored_token.entity().to_bits());
        with_identity_domain(&mut restored_world, |world, domain| {
            domain
                .register_restored_scene_instance(
                    world,
                    &snapshot,
                    [(player, restored_token, None)],
                )
                .unwrap();
        });

        assert_eq!(
            target.resolve_entity(&restored_world),
            Some(EntityLookup::Resolved(restored_token.entity()))
        );
    }

    #[test]
    fn parallel_fork_remaps_submission_targets_without_mutating_the_source() {
        let player = scene_entity_id("player");
        let mut source_world = world_with_identity_domain();
        let (source_instance, _) = register_scene_entity(&mut source_world, &player);
        let source_reference = source_instance.runtime_reference(&player).unwrap();
        let source_snapshot = source_world
            .resource::<WorldIdentityDomain>()
            .scene_identity_snapshot(&source_world, &source_instance)
            .unwrap();
        let submission = GameplayCommandSubmission::new(
            tick(7),
            GameplayCommandIngressSource::replay("fork").unwrap(),
            sequence(1),
            command("player.move")
                .with_target(GameplayCommandTarget::Entity(source_reference.clone())),
        );

        let mut fork_world = world_with_identity_domain();
        let _occupied = register_scene_entity(&mut fork_world, &scene_entity_id("occupied"));
        let fork_token = spawn_identity_entity(&mut fork_world).unwrap();
        let (fork_instance, remap) = with_identity_domain(&mut fork_world, |world, domain| {
            domain
                .register_parallel_scene_fork(
                    world,
                    &source_snapshot,
                    [(player.clone(), fork_token, None)],
                )
                .unwrap()
        });

        let remapped = submission.remap_entity_target(remap.references()).unwrap();
        assert_eq!(
            submission.command().target(),
            Some(&GameplayCommandTarget::Entity(source_reference))
        );
        let expected =
            GameplayCommandTarget::Entity(fork_instance.runtime_reference(&player).unwrap());
        assert_eq!(remapped.command().target(), Some(&expected));
        assert!(matches!(
            expected.resolve_entity(&fork_world),
            Some(EntityLookup::Resolved(entity)) if entity == fork_token.entity()
        ));
    }

    #[test]
    fn zero_and_invalid_identity_values_are_rejected_before_retention() {
        assert_eq!(GameplayCommandTick::new(0), None);
        assert_eq!(GameplayCommandSourceSequence::new(0), None);
        assert!(GameplayCommandSource::external("").is_err());
        assert!(GameplayCommandTarget::named("bad\nname").is_err());
        assert!(GameplayCommandTypeId::new("x".repeat(MAX_GAMEPLAY_COMMAND_ID_BYTES + 1)).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn submission_roundtrips_and_reserved_or_obsolete_shapes_are_rejected() {
        let submission = submission(
            1,
            GameplayCommandIngressSource::replay("stream").unwrap(),
            9,
            "player.jump",
        );
        let encoded = serde_json::to_string(&submission).unwrap();
        assert_eq!(
            serde_json::to_value(&submission).unwrap(),
            serde_json::json!({
                "tick": 1,
                "source": { "Replay": { "stream": "stream" } },
                "source_sequence": 9,
                "command": {
                    "command_type": "player.jump",
                    "target": null,
                    "payload": {}
                }
            })
        );
        assert_eq!(
            serde_json::from_str::<GameplayCommandSubmission>(&encoded).unwrap(),
            submission
        );

        let obsolete = r#"{
            "sequence": 1,
            "command_type": "player.jump",
            "source": "LocalAction",
            "time": { "frame": 1, "fixed_tick": null },
            "target": null,
            "payload": {}
        }"#;
        assert!(serde_json::from_str::<GameplayCommandSubmission>(obsolete).is_err());

        let forged_local = r#"{
            "tick": 1,
            "source": "LocalAction",
            "source_sequence": 1,
            "command": { "command_type": "player.jump" }
        }"#;
        assert!(serde_json::from_str::<GameplayCommandSubmission>(forged_local).is_err());

        let unknown_source_field = r#"{
            "tick": 1,
            "source": { "Replay": { "stream": "stream", "unexpected": true } },
            "source_sequence": 1,
            "command": { "command_type": "player.jump" }
        }"#;
        assert!(serde_json::from_str::<GameplayCommandSubmission>(unknown_source_field).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn invalid_submission_identity_and_payload_shapes_are_rejected() {
        let invalid_values = [
            serde_json::json!({
                "tick": 0,
                "source": { "Test": { "driver": "driver" } },
                "source_sequence": 1,
                "command": { "command_type": "command" }
            }),
            serde_json::json!({
                "tick": 1,
                "source": { "External": { "producer": "" } },
                "source_sequence": 1,
                "command": { "command_type": "command" }
            }),
            serde_json::json!({
                "tick": 1,
                "source": { "Ai": { "agent": "agent" } },
                "source_sequence": 0,
                "command": { "command_type": "command" }
            }),
            serde_json::json!({
                "tick": 1,
                "source": { "Replay": { "stream": "stream" } },
                "source_sequence": 1,
                "command": { "command_type": "bad\ncommand" }
            }),
        ];
        for value in invalid_values {
            assert!(serde_json::from_value::<GameplayCommandSubmission>(value).is_err());
        }

        let oversized_source = serde_json::json!({
            "tick": 1,
            "source": {
                "External": { "producer": "x".repeat(MAX_GAMEPLAY_COMMAND_ID_BYTES + 1) }
            },
            "source_sequence": 1,
            "command": { "command_type": "command" }
        });
        assert!(serde_json::from_value::<GameplayCommandSubmission>(oversized_source).is_err());

        let oversized_string = serde_json::json!({
            "tick": 1,
            "source": { "Test": { "driver": "driver" } },
            "source_sequence": 1,
            "command": {
                "command_type": "command",
                "payload": {
                    "value": { "String": "x".repeat(MAX_GAMEPLAY_COMMAND_STRING_BYTES + 1) }
                }
            }
        });
        assert!(serde_json::from_value::<GameplayCommandSubmission>(oversized_string).is_err());

        let mut oversized_key_payload = serde_json::Map::new();
        oversized_key_payload.insert(
            "x".repeat(MAX_GAMEPLAY_COMMAND_PAYLOAD_KEY_BYTES + 1),
            serde_json::json!({ "Bool": true }),
        );
        let oversized_key = serde_json::json!({
            "tick": 1,
            "source": { "Test": { "driver": "driver" } },
            "source_sequence": 1,
            "command": {
                "command_type": "command",
                "payload": oversized_key_payload
            }
        });
        assert!(serde_json::from_value::<GameplayCommandSubmission>(oversized_key).is_err());

        let duplicate_payload_key = r#"{
            "tick": 1,
            "source": { "Test": { "driver": "driver" } },
            "source_sequence": 1,
            "command": {
                "command_type": "command",
                "payload": {
                    "duplicate": { "I64": 1 },
                    "duplicate": { "I64": 2 }
                }
            }
        }"#;
        assert!(serde_json::from_str::<GameplayCommandSubmission>(duplicate_payload_key).is_err());

        let invalid_payload_key = r#"{
            "tick": 1,
            "source": { "Test": { "driver": "driver" } },
            "source_sequence": 1,
            "command": {
                "command_type": "command",
                "payload": { "\n": { "Bool": true } }
            }
        }"#;
        assert!(serde_json::from_str::<GameplayCommandSubmission>(invalid_payload_key).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn admitted_envelope_serializes_the_canonical_replay_shape() {
        let mut queue = GameplayCommandQueue::default();
        queue
            .submit(submission(
                1,
                GameplayCommandIngressSource::replay("stream").unwrap(),
                9,
                "player.jump",
            ))
            .unwrap();
        let mut batch = GameplayCommandBatch::new();
        queue.admit_tick(tick(1), &mut batch).unwrap();

        assert_eq!(
            serde_json::to_value(&batch.commands()[0]).unwrap(),
            serde_json::json!({
                "tick": 1,
                "source": { "Replay": { "stream": "stream" } },
                "source_sequence": 9,
                "command": {
                    "command_type": "player.jump",
                    "target": null,
                    "payload": {}
                }
            })
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn entity_command_targets_use_the_canonical_identity_shape() {
        let named = GameplayCommandTarget::named("player").unwrap();
        let scene = GameplayCommandTarget::Entity(RuntimeEntityReference::scene(
            SceneInstanceId::new(7).unwrap(),
            scene_entity_id("scene/player"),
        ));
        let persistent = GameplayCommandTarget::Entity(RuntimeEntityReference::persistent(
            persistent_reference("A1B2C3D4-E5F6-47A8-90AB-1234567890CD"),
        ));

        assert_eq!(
            serde_json::to_value(&named).unwrap(),
            serde_json::json!({ "Named": "player" })
        );
        assert_eq!(
            serde_json::to_value(&scene).unwrap(),
            serde_json::json!({
                "Entity": {
                    "kind": "scene",
                    "instance": 7,
                    "entity": "scene/player"
                }
            })
        );
        assert_eq!(
            serde_json::to_value(&persistent).unwrap(),
            serde_json::json!({
                "Entity": {
                    "kind": "persistent",
                    "entity": {
                        "namespace": "runtime",
                        "entity": "a1b2c3d4-e5f6-47a8-90ab-1234567890cd"
                    }
                }
            })
        );

        for target in [&named, &scene, &persistent] {
            let encoded = serde_json::to_value(target).unwrap();
            assert_eq!(
                serde_json::from_value::<GameplayCommandTarget>(encoded).unwrap(),
                *target
            );
        }
        assert!(
            serde_json::from_value::<GameplayCommandTarget>(
                serde_json::json!({ "Scene": "scene/player" })
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<GameplayCommandTarget>(serde_json::json!({
                "Entity": {
                    "kind": "scene",
                    "instance": 7,
                    "entity": "scene/player",
                    "domain": 3
                }
            }))
            .is_err()
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn action_command_binding_defaults_context_to_gameplay() {
        let json = r#"{
            "action": "jump",
            "phase": "Started",
            "command": {
                "command_type": "player.jump",
                "target": null,
                "payload": {}
            }
        }"#;

        let binding: ActionCommandBinding = serde_json::from_str(json).unwrap();
        assert_eq!(binding.context(), &ActionContext::gameplay());

        assert!(
            serde_json::from_str::<ActionCommandMap>(r#"{ "bindings": [], "unexpected": true }"#)
                .is_err()
        );

        let oversized = serde_json::json!({
            "bindings": vec![binding; MAX_ACTION_COMMAND_BINDINGS + 1],
        });
        assert!(serde_json::from_value::<ActionCommandMap>(oversized).is_err());

        let runtime_target = serde_json::json!({
            "bindings": [{
                "action": "jump",
                "phase": "Started",
                "command": {
                    "command_type": "player.jump",
                    "target": {
                        "Entity": {
                            "kind": "scene",
                            "instance": 1,
                            "entity": "player"
                        }
                    },
                    "payload": {}
                }
            }]
        });
        assert!(serde_json::from_value::<ActionCommandMap>(runtime_target).is_err());
    }
}
