#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fmt::{self, Display, Formatter},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use engine::{
        app::{App, CoreStage, FixedTime, FixedUpdateSet, RuntimeFaultKind, RuntimeFaultReporter},
        ecs::{
            Commands, Component, Query, Res, ResMut, Resource, ScheduleLabel, SystemSet,
            error::{BevyError, ErrorContext, FallbackErrorHandler},
            schedule::IntoScheduleConfigs,
        },
        gameplay::{
            GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
            GameplayCommandLifecycleError, GameplayCommandPlugin, GameplayCommandQueue,
            GameplayCommandSet, GameplayCommandSourceSequence, GameplayCommandSubmission,
            GameplayCommandTick, GameplayCommandTypeId,
        },
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    enum ExtensionSet {
        SimulateMember,
        BeforeConsume,
        ConsumeMember,
        CaptureMember,
    }

    #[derive(Debug, Default, Resource)]
    struct AnchorTrace {
        simulate_runs: usize,
        consume: Vec<(Option<u64>, usize, usize)>,
        capture: Vec<(Option<u64>, usize, usize)>,
    }

    #[derive(Component)]
    struct BeforeConsumeDeferred;

    #[derive(Component)]
    struct ConsumeDeferred;

    #[derive(Component)]
    struct CaptureDeferred;

    fn record_simulate_membership(mut trace: ResMut<AnchorTrace>) {
        trace.simulate_runs += 1;
    }

    fn emit_before_consume(mut commands: Commands) {
        commands.spawn(BeforeConsumeDeferred);
    }

    fn observe_consume(
        batch: Res<GameplayCommandBatch>,
        deferred: Query<&BeforeConsumeDeferred>,
        mut trace: ResMut<AnchorTrace>,
        mut commands: Commands,
    ) {
        trace.consume.push((
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
            deferred.iter().count(),
        ));
        commands.spawn(ConsumeDeferred);
    }

    fn observe_capture(
        batch: Res<GameplayCommandBatch>,
        deferred: Query<&ConsumeDeferred>,
        mut trace: ResMut<AnchorTrace>,
        mut commands: Commands,
    ) {
        trace.capture.push((
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
            deferred.iter().count(),
        ));
        commands.spawn(CaptureDeferred);
    }

    fn configure_extension_anchors(app: &mut App) {
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                ExtensionSet::SimulateMember.in_set(FixedUpdateSet::Simulate),
                ExtensionSet::BeforeConsume.before(GameplayCommandSet::Consume),
                ExtensionSet::ConsumeMember.in_set(GameplayCommandSet::Consume),
                ExtensionSet::CaptureMember.in_set(GameplayCommandSet::Capture),
            ),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            record_simulate_membership.in_set(ExtensionSet::SimulateMember),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            emit_before_consume.in_set(ExtensionSet::BeforeConsume),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            observe_consume.in_set(ExtensionSet::ConsumeMember),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            observe_capture.in_set(ExtensionSet::CaptureMember),
        )
        .unwrap();
    }

    fn submission(tick: u64) -> GameplayCommandSubmission {
        GameplayCommandSubmission::new(
            GameplayCommandTick::new(tick).unwrap(),
            GameplayCommandIngressSource::test("renamed-root").unwrap(),
            GameplayCommandSourceSequence::new(1).unwrap(),
            GameplayCommandDraft::new(GameplayCommandTypeId::new("fixture.command").unwrap()),
        )
    }

    #[test]
    fn documented_anchors_expose_deferred_and_cleanup_semantics() {
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.init_resource::<AnchorTrace>().unwrap();
        configure_extension_anchors(&mut app);
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(submission(1))
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let trace = app.world().resource::<AnchorTrace>();
        assert_eq!(trace.simulate_runs, 1);
        assert_eq!(trace.consume, [(Some(1), 1, 1)]);
        assert_eq!(trace.capture, [(Some(1), 1, 1)]);
        assert!(
            app.world()
                .iter_entities()
                .any(|entity| entity.contains::<CaptureDeferred>())
        );
        let batch = app.world().resource::<GameplayCommandBatch>();
        assert_eq!(batch.active_tick(), None);
        assert!(batch.is_empty());
        let stats = app.world().resource::<GameplayCommandQueue>().stats();
        assert_eq!(stats.active_commands, 0);
        assert_eq!(stats.retained_commands, 0);
        assert_eq!(stats.acknowledged_through_tick, 1);
        assert!(app.get_schedule_mut(CoreStage::FixedUpdate).is_err());
    }

    #[derive(Debug, Default, Resource)]
    struct ConditionalRuns(usize);

    #[derive(Debug, Resource)]
    struct ConditionalEnabled(bool);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    struct ConditionalConsume;

    fn conditional_enabled(enabled: Res<ConditionalEnabled>) -> bool {
        enabled.0
    }

    fn record_conditional_consume(mut runs: ResMut<ConditionalRuns>) {
        runs.0 += 1;
    }

    #[test]
    fn public_anchor_member_run_condition_may_skip_without_blocking_cleanup() {
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.init_resource::<ConditionalRuns>().unwrap();
        app.insert_resource(ConditionalEnabled(false)).unwrap();
        app.configure_sets(
            CoreStage::FixedUpdate,
            ConditionalConsume
                .in_set(GameplayCommandSet::Consume)
                .run_if(conditional_enabled),
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            record_conditional_consume.in_set(ConditionalConsume),
        )
        .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(submission(1))
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(app.world().resource::<ConditionalRuns>().0, 0);
        let stats = app.world().resource::<GameplayCommandQueue>().stats();
        assert_eq!(stats.retained_commands, 0);
        assert_eq!(stats.acknowledged_through_tick, 1);
    }

    static APP_POLICY_ERRORS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug)]
    struct FixtureSystemError;

    impl Display for FixtureSystemError {
        fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
            formatter.write_str("injected schedule extension failure")
        }
    }

    impl Error for FixtureSystemError {}

    fn record_app_policy_error(_error: BevyError, _context: ErrorContext) {
        APP_POLICY_ERRORS.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_under_app_policy() -> Result<(), BevyError> {
        Err(BevyError::error(FixtureSystemError))
    }

    #[test]
    fn public_anchor_member_errors_follow_the_configured_app_policy() {
        APP_POLICY_ERRORS.store(0, Ordering::SeqCst);
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.insert_resource(FallbackErrorHandler(record_app_policy_error))
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, fail_under_app_policy)
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                fail_under_app_policy.in_set(FixedUpdateSet::Simulate),
            )
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                fail_under_app_policy.in_set(GameplayCommandSet::Consume),
            )
            .unwrap()
            .add_systems(
                CoreStage::FixedUpdate,
                fail_under_app_policy.in_set(GameplayCommandSet::Capture),
            )
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(APP_POLICY_ERRORS.load(Ordering::SeqCst), 4);
        assert_eq!(
            app.world()
                .resource::<GameplayCommandQueue>()
                .stats()
                .acknowledged_through_tick,
            1
        );
        assert_eq!(app.world().resource::<RuntimeFaultReporter>().fault(), None);
    }

    #[derive(Debug, Default, Resource)]
    struct DomainFaultTrace {
        consume: Vec<(Option<u64>, usize)>,
        capture_runs: usize,
    }

    fn record_consume_before_fault(
        batch: Res<GameplayCommandBatch>,
        mut trace: ResMut<DomainFaultTrace>,
    ) {
        trace.consume.push((
            batch.active_tick().map(GameplayCommandTick::get),
            batch.len(),
        ));
    }

    fn replace_queue_during_capture(mut trace: ResMut<DomainFaultTrace>, mut commands: Commands) {
        trace.capture_runs += 1;
        commands.insert_resource(GameplayCommandQueue::default());
    }

    #[test]
    fn gameplay_lifecycle_faults_are_observable_and_gate_later_anchor_members() {
        let mut app = App::new();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        app.init_resource::<DomainFaultTrace>().unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            record_consume_before_fault.in_set(GameplayCommandSet::Consume),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            replace_queue_during_capture.in_set(GameplayCommandSet::Capture),
        )
        .unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(submission(1))
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let trace = app.world().resource::<DomainFaultTrace>();
        assert_eq!(trace.consume, [(Some(1), 1)]);
        assert_eq!(trace.capture_runs, 1);
        assert_eq!(
            app.world()
                .resource::<GameplayCommandQueue>()
                .last_lifecycle_error(),
            Some(&GameplayCommandLifecycleError::BatchTickMismatch)
        );
        let fault = app
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .expect("gameplay acknowledgement failure reports a domain fault");
        assert_eq!(fault.kind(), RuntimeFaultKind::GameplayLifecycle);
        assert_eq!(fault.source(), "nara.gameplay.fixed-acknowledge");

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let trace = app.world().resource::<DomainFaultTrace>();
        assert_eq!(trace.consume, [(Some(1), 1)]);
        assert_eq!(trace.capture_runs, 1);
        let batch = app.world().resource::<GameplayCommandBatch>();
        assert_eq!(batch.active_tick(), None);
        assert!(batch.is_empty());
    }

    #[derive(Component)]
    struct IgnoredDeferred;

    #[derive(Debug, Default, Resource)]
    struct VisibilityTrace(bool);

    fn emit_ignored_deferred(mut commands: Commands) {
        commands.spawn(IgnoredDeferred);
    }

    fn observe_ignored_deferred(
        deferred: Query<&IgnoredDeferred>,
        mut trace: ResMut<VisibilityTrace>,
    ) {
        trace.0 = !deferred.is_empty();
    }

    #[test]
    fn ignore_deferred_edge_seals_but_fails_the_visibility_oracle() {
        let mut app = App::new();
        app.init_resource::<VisibilityTrace>().unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            emit_ignored_deferred.before_ignore_deferred(FixedUpdateSet::Simulate),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            observe_ignored_deferred.in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert!(!app.world().resource::<VisibilityTrace>().0);
        assert!(
            app.world()
                .iter_entities()
                .any(|entity| entity.contains::<IgnoredDeferred>())
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct ForeignSchedule;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    struct CrossScheduleTarget;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    struct AbsentTarget;

    #[derive(Debug, Default, Resource)]
    struct ExecutionTrace(Vec<&'static str>);

    fn record_fixed(mut trace: ResMut<ExecutionTrace>) {
        trace.0.push("fixed");
    }

    fn record_foreign(mut trace: ResMut<ExecutionTrace>) {
        trace.0.push("foreign");
    }

    #[test]
    fn absent_and_cross_schedule_targets_do_not_order_execution() {
        let mut app = App::new();
        app.init_resource::<ExecutionTrace>().unwrap();
        app.init_schedule(ForeignSchedule).unwrap();
        app.add_systems(ForeignSchedule, record_foreign.in_set(CrossScheduleTarget))
            .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            record_fixed.after(CrossScheduleTarget).after(AbsentTarget),
        )
        .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();
        assert_eq!(app.world().resource::<ExecutionTrace>().0, ["fixed"]);

        app.run_schedule(ForeignSchedule).unwrap();
        assert_eq!(
            app.world().resource::<ExecutionTrace>().0,
            ["fixed", "foreign"]
        );
    }

    #[derive(Debug, Default, Resource)]
    struct CommutativeResult(u32);

    fn add_one(mut result: ResMut<CommutativeResult>) {
        result.0 += 1;
    }

    fn add_ten(mut result: ResMut<CommutativeResult>) {
        result.0 += 10;
    }

    fn run_unordered_peers(reverse_registration: bool) -> u32 {
        let mut app = App::new();
        app.init_resource::<CommutativeResult>().unwrap();
        if reverse_registration {
            app.add_systems(
                CoreStage::FixedUpdate,
                (
                    add_ten.in_set(FixedUpdateSet::Simulate),
                    add_one.in_set(FixedUpdateSet::Simulate),
                ),
            )
            .unwrap();
        } else {
            app.add_systems(
                CoreStage::FixedUpdate,
                (
                    add_one.in_set(FixedUpdateSet::Simulate),
                    add_ten.in_set(FixedUpdateSet::Simulate),
                ),
            )
            .unwrap();
        }
        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();
        app.world().resource::<CommutativeResult>().0
    }

    #[test]
    fn unrelated_registration_permutations_preserve_only_semantic_results() {
        assert_eq!(run_unordered_peers(false), 11);
        assert_eq!(run_unordered_peers(true), 11);
    }
}
