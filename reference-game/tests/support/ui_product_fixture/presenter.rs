//! Component-only presentation glue for the focused product fixture.
//!
//! This module exercises planner, acceptance and exact-receipt contracts without WGPU, a native
//! window or OS accessibility authority. It must not be cited as desktop/backend or platform
//! accessibility evidence.

use std::{
    mem,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use nara::{
    app::{
        __RuntimeDriverPort, CoreStage, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginId, PluginShutdownContext, PluginShutdownObligationId, RuntimeDriverScope,
        RuntimeGeneration, RuntimeWorldAccessError,
    },
    ecs::{
        Entity, Mut, Resource, With, schedule::IntoScheduleConfigs, upstream::lifecycle::Add,
        upstream::observer::On,
    },
    gameplay::{GameplayCommandBatch, GameplayCommandSet, add_local_gameplay_command_systems},
    identity::{EntityLookup, WorldIdentityDomain},
    prelude::{App, Commands, FixedTime, Plugin, PluginError, Query, Res, ResMut, World},
    scene::SceneEntitySource,
    ui::latest_ui_frame,
    window::WindowId,
};
use nara_reference_game::{
    PlayerRole, REFERENCE_RUNTIME_UI_PLUGIN_ID, WaveRetryRejection, WaveRetryStatus, WaveSnapshot,
};
use nara_render::RenderTarget;
use nara_ui::{
    UiAccessibilityInput, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionJournal,
    UiPresentationReceipt, UiRouteDisposition, UiSurfaceContentTransform, active_ui_surface_slots,
    dispatch_ui_accessibility_input, update_ui_surface_slot,
};
use nara_ui_render::{
    UiBackendSubmissionIdentity, UiPaintPacket, UiPreparedImages, UiPresentationAttemptId,
    UiPresentationBackendGeneration, UiPresentationCapabilityGeneration, UiPresentationEpoch,
    UiPresentationOutcome, UiPresentationPlanner, UiPresentationRuntimeGeneration,
};

use super::reference_surface_id;

const COMPONENT_PRESENTER_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.component-presenter");
const RETRY_REPLACEMENT_REJECTION_FIXTURE_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.retry-replacement-rejection-fixture");
const COMPONENT_PRESENTER_REQUIREMENTS: &[PluginId] = &[REFERENCE_RUNTIME_UI_PLUGIN_ID];
const COMPONENT_PRESENTER_SHUTDOWN_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("reference-game.test.component-presenter.ui-action-audit");
const COMPONENT_PRESENTER_SHUTDOWN_OBLIGATIONS: &[PluginShutdownObligationId] =
    &[COMPONENT_PRESENTER_SHUTDOWN_OBLIGATION];
const COMPONENT_PRESENTER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMPONENT_PRESENTER_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(COMPONENT_PRESENTER_REQUIREMENTS)
        .shutdown_obligations(COMPONENT_PRESENTER_SHUTDOWN_OBLIGATIONS);
const RETRY_REPLACEMENT_REJECTION_FIXTURE_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    RETRY_REPLACEMENT_REJECTION_FIXTURE_PLUGIN_ID,
    PluginCategory::Runtime,
)
.requires_plugins(COMPONENT_PRESENTER_REQUIREMENTS)
.shutdown_obligations(COMPONENT_PRESENTER_SHUTDOWN_OBLIGATIONS);
static NEXT_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PRESENTATION_ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);
const RETRY_COMMAND_TYPE: &str = "reference-game.retry-v1";
const MAX_RECORDED_RETRY_BATCHES: usize = 8;
const MAX_RECORDED_UI_ACTIONS: usize = 16;

#[derive(Debug, Default)]
struct ComponentPresenterPlugin;

impl Plugin for ComponentPresenterPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMPONENT_PRESENTER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        install_component_presenter(app)
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        shutdown_component_presenter(context)
    }
}

#[derive(Debug, Default)]
struct RetryReplacementRejectionFixturePlugin;

impl Plugin for RetryReplacementRejectionFixturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETRY_REPLACEMENT_REJECTION_FIXTURE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        install_component_presenter(app)?;
        app.init_resource::<RetryCandidateObserver>()?.add_systems(
            CoreStage::FixedUpdate,
            (
                install_retry_candidate_observer_after_terminal,
                repair_retry_candidate_observer_after_rejection,
            )
                .chain()
                .after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        shutdown_component_presenter(context)
    }
}

#[derive(Clone, Debug)]
struct PresentedComponentPacket {
    receipt: UiPresentationReceipt,
    packet: Arc<UiPaintPacket>,
    acceptance: UiBackendSubmissionIdentity,
}

#[derive(Resource)]
pub(super) struct ComponentPresentationState {
    last: Option<PresentedComponentPacket>,
    planner: UiPresentationPlanner,
    backend_generation: UiPresentationBackendGeneration,
}

impl Default for ComponentPresentationState {
    fn default() -> Self {
        Self {
            last: None,
            planner: UiPresentationPlanner::default(),
            backend_generation: UiPresentationBackendGeneration::new(
                NEXT_BACKEND_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
                1,
            )
            .expect("the component fixture uses non-zero backend generations"),
        }
    }
}

impl ComponentPresentationState {
    pub(super) fn latest_receipt(&self) -> Option<UiPresentationReceipt> {
        self.last.as_ref().map(|presented| presented.receipt)
    }

    pub(super) fn latest(&self) -> Option<(UiPresentationReceipt, Arc<UiPaintPacket>)> {
        self.last
            .as_ref()
            .map(|presented| (presented.receipt, Arc::clone(&presented.packet)))
    }

    pub(super) fn latest_acceptance_identity(&self) -> Option<UiBackendSubmissionIdentity> {
        self.last.as_ref().map(|presented| presented.acceptance)
    }

    fn next_epoch(&self) -> u64 {
        self.last.as_ref().map_or(1, |presented| {
            presented
                .receipt
                .presentation_epoch()
                .checked_add(1)
                .expect("the bounded component presenter epoch should not exhaust")
        })
    }
}

#[derive(Debug, Default, Resource)]
struct ComponentAccessibilityInputs(Vec<UiAccessibilityInput>);

impl __RuntimeDriverPort for ComponentAccessibilityInputs {
    type Input = UiAccessibilityInput;
    type Output = ();

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.0.push(input);
    }
}

#[derive(Debug, Default, Resource)]
struct ComponentSurfaceUpdates(Vec<UiSurfaceContentTransform>);

impl __RuntimeDriverPort for ComponentSurfaceUpdates {
    type Input = UiSurfaceContentTransform;
    type Output = ();

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.0.push(input);
    }
}

#[derive(Debug, Default, Resource)]
struct RetryCandidateObserver {
    installed: bool,
    triggered: bool,
    observer_entity: Option<Entity>,
    repair_at_tick: Option<u64>,
}

#[derive(Debug, Default, Resource)]
pub(super) struct ComponentRetryCommandTrace {
    batches: Vec<usize>,
    overflowed: bool,
}

#[derive(Debug, Resource)]
pub(super) struct ComponentUiActionTrace {
    cursor: UiActionCursor,
    events: Vec<UiActionEvent>,
    overflowed: bool,
}

impl ComponentUiActionTrace {
    pub(super) fn events(&self) -> &[UiActionEvent] {
        &self.events
    }

    pub(super) const fn overflowed(&self) -> bool {
        self.overflowed
    }
}

impl ComponentRetryCommandTrace {
    pub(super) fn batches(&self) -> &[usize] {
        &self.batches
    }

    pub(super) const fn overflowed(&self) -> bool {
        self.overflowed
    }

    fn record(&mut self, retry_count: usize) {
        if retry_count == 0 {
            return;
        }
        if self.batches.len() == MAX_RECORDED_RETRY_BATCHES {
            self.overflowed = true;
            return;
        }
        self.batches.push(retry_count);
    }
}

fn install_component_presenter(app: &mut App) -> Result<(), PluginError> {
    let cursor = app
        .world_mut()?
        .get_resource_mut::<UiActionJournal>()
        .ok_or_else(|| fixture_error("UI action journal is not installed"))?
        .register_consumer(
            UiActionConsumerId::new("reference-game.test.ui-action-audit")
                .map_err(|error| fixture_error(error.to_string()))?,
        )
        .map_err(|error| fixture_error(error.to_string()))?;
    app.register_plugin_shutdown_obligation(COMPONENT_PRESENTER_SHUTDOWN_OBLIGATION)?;
    app.insert_resource(ComponentUiActionTrace {
        cursor,
        events: Vec::new(),
        overflowed: false,
    })?;
    add_local_gameplay_command_systems(app, record_component_ui_actions)?;
    app.init_resource::<ComponentPresentationState>()?
        .init_resource::<ComponentAccessibilityInputs>()?
        .init_resource::<ComponentSurfaceUpdates>()?
        .init_resource::<ComponentRetryCommandTrace>()?
        .add_systems(
            CoreStage::First,
            (
                apply_component_surface_updates,
                dispatch_component_accessibility_inputs,
            )
                .chain(),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            record_component_retry_commands.in_set(GameplayCommandSet::Capture),
        )?
        .add_systems(CoreStage::Cleanup, present_latest_reference_ui_component)?;
    Ok(())
}

fn shutdown_component_presenter(
    context: &mut PluginShutdownContext<'_>,
) -> Result<(), PluginError> {
    if !context
        .world()
        .contains_resource::<ComponentUiActionTrace>()
    {
        return Ok(());
    }
    if !context.world().contains_resource::<UiActionJournal>() {
        return Err(fixture_error("UI action journal is not installed"));
    }
    context.world_mut().resource_scope(
        |world, mut journal: Mut<UiActionJournal>| -> Result<(), PluginError> {
            let trace = world
                .get_resource::<ComponentUiActionTrace>()
                .expect("validated component UI action trace remains installed");
            journal
                .unregister_consumer(&trace.cursor)
                .map_err(|error| fixture_error(error.to_string()))
        },
    )?;
    context
        .world_mut()
        .remove_resource::<ComponentUiActionTrace>();
    Ok(())
}

fn record_component_ui_actions(
    mut journal: ResMut<UiActionJournal>,
    mut trace: ResMut<ComponentUiActionTrace>,
) {
    let Ok(pending) = journal.pending(&trace.cursor) else {
        trace.overflowed = true;
        return;
    };
    let last_sequence = pending.last().map(UiActionEvent::sequence);
    let remaining = MAX_RECORDED_UI_ACTIONS.saturating_sub(trace.events.len());
    trace.events.extend(pending.iter().take(remaining).cloned());
    trace.overflowed |= pending.len() > remaining;
    if let Some(sequence) = last_sequence
        && journal.acknowledge(&trace.cursor, sequence).is_err()
    {
        trace.overflowed = true;
    }
}

fn fixture_error(message: impl Into<String>) -> PluginError {
    PluginError::SetupFailed {
        plugin: COMPONENT_PRESENTER_PLUGIN_ID,
        message: message.into(),
    }
}

fn record_component_retry_commands(
    batch: Res<GameplayCommandBatch>,
    mut trace: ResMut<ComponentRetryCommandTrace>,
) {
    let retry_count = batch
        .iter()
        .filter(|command| command.command_type().as_str() == RETRY_COMMAND_TYPE)
        .count();
    trace.record(retry_count);
}

pub(super) fn queue_component_accessibility_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: UiAccessibilityInput,
) -> Result<(), RuntimeWorldAccessError> {
    scope.__apply_port::<ComponentAccessibilityInputs>(input)
}

pub(super) fn queue_component_surface_update(
    scope: &mut RuntimeDriverScope<'_>,
    transform: UiSurfaceContentTransform,
) -> Result<(), RuntimeWorldAccessError> {
    scope.__apply_port::<ComponentSurfaceUpdates>(transform)
}

fn apply_component_surface_updates(world: &mut World) {
    let pending = mem::take(&mut world.resource_mut::<ComponentSurfaceUpdates>().0);
    for transform in pending {
        let slot = active_ui_surface_slots(world)
            .expect("the component fixture should retain active UI surfaces")
            .into_iter()
            .find(|slot| slot.surface_id() == reference_surface_id())
            .expect("the reference UI surface should remain installed");
        update_ui_surface_slot(world, slot.surface_id(), slot.endpoint(), transform)
            .expect("the component Host fixture should publish the exact surface transform");
    }
}

fn dispatch_component_accessibility_inputs(world: &mut World) {
    let pending = mem::take(&mut world.resource_mut::<ComponentAccessibilityInputs>().0);
    for input in pending {
        assert!(matches!(
            dispatch_ui_accessibility_input(world, input),
            UiRouteDisposition::Consumed(_)
        ));
    }
}

fn present_latest_reference_ui_component(world: &mut World) {
    let Ok(Some(frame)) = latest_ui_frame(world, reference_surface_id()) else {
        return;
    };
    let (packet, epoch, backend_generation) = {
        let mut state = world.resource_mut::<ComponentPresentationState>();
        let epoch = state.next_epoch();
        let packet = state
            .planner
            .plan(
                Arc::clone(&frame),
                RenderTarget::Window(WindowId::PRIMARY),
                &UiPreparedImages::default(),
            )
            .expect("the bounded component presenter should plan the exact latest frame");
        (packet, epoch, state.backend_generation)
    };
    let presentation_epoch = UiPresentationEpoch::new(epoch)
        .expect("the bounded component presenter uses non-zero presentation epochs");
    let runtime_generation =
        UiPresentationRuntimeGeneration::new(world.resource::<RuntimeGeneration>().get())
            .expect("managed runtime generations are non-zero");
    let authority = nara_ui::__private::prepare_ui_presentation(world, packet.frame(), epoch)
        .expect("the Host should reserve exact presentation authority");
    let prepared = nara_ui_render::__private::prepare_backend_submission(
        UiPresentationAttemptId::new(NEXT_PRESENTATION_ATTEMPT_ID.fetch_add(1, Ordering::Relaxed))
            .expect("the bounded component presenter uses non-zero attempt identities"),
        Arc::clone(&packet),
        presentation_epoch,
        runtime_generation,
        UiPresentationCapabilityGeneration::new(1)
            .expect("the bounded component presenter uses a non-zero capability generation"),
    );
    let prepared_identity = prepared.identity();
    let mut outcomes = nara_ui_render::__private::UiPresentationOutcomes::default();
    let permit = outcomes
        .reserve_backend(std::slice::from_ref(&prepared))
        .expect("the bounded component presenter should reserve one backend outcome")
        .pop()
        .expect("one prepared attempt reserves one backend outcome");
    let envelope = nara_ui_render::__private::UiBackendSubmission::pair(prepared, permit)
        .expect("the prepared attempt and backend permit should match");
    let submitted = nara_ui_render::__private::bind_backend_submission_batch(
        vec![envelope],
        backend_generation,
    )
    .expect("the bounded component presenter should bind its exact backend generation")
    .pop()
    .expect("one prepared attempt binds one backend submission");
    outcomes.commit_backend(submitted.presented());
    let mut backend_world = World::new();
    backend_world.insert_resource(outcomes);
    let commit = match nara_ui_render::drain_ui_presentation_outcomes(&mut backend_world)
        .expect("the bounded component presenter should publish one backend outcome")
        .pop()
        .expect("one backend submission publishes one outcome")
    {
        UiPresentationOutcome::Presented(commit) => commit,
        outcome => panic!("the bounded component presenter expected Presented, got {outcome:?}"),
    };
    let accepted = nara_ui_render::__private::discard_ui_backend_acceptance(commit);
    assert_eq!(accepted.identity(), prepared_identity);
    assert_eq!(accepted.identity().runtime_generation(), runtime_generation);
    if !Arc::ptr_eq(accepted.packet(), &packet) {
        nara_ui::__private::supersede_prepared_ui_presentation(world, authority);
        panic!("backend acceptance did not retain the exact product packet");
    }
    let acceptance = accepted.identity();
    let identity = accepted.packet().identity();
    let receipt = nara_ui::__private::commit_prepared_ui_presentation(world, authority);
    assert_eq!(receipt.surface_id(), identity.surface_id());
    assert_eq!(receipt.frame_revision(), identity.frame_revision());
    assert_eq!(receipt.route_revision(), identity.route_revision());
    assert_eq!(
        receipt.presentation_revision(),
        identity.presentation_revision()
    );
    assert_eq!(
        receipt.presentation_epoch(),
        acceptance.presentation_epoch().get()
    );
    assert!(acceptance.matches_receipt(receipt));
    world.resource_mut::<ComponentPresentationState>().last = Some(PresentedComponentPacket {
        receipt,
        packet,
        acceptance,
    });
}

fn install_retry_candidate_observer_after_terminal(world: &mut World) {
    if world.resource::<RetryCandidateObserver>().installed
        || !world.resource::<WaveSnapshot>().is_terminal()
    {
        return;
    }
    let old_player = world
        .iter_entities()
        .find_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            (entity.contains::<PlayerRole>()
                && world
                    .resource::<WorldIdentityDomain>()
                    .resolve_entity_reference(
                        world,
                        &nara::identity::EntityReference::SceneLocal {
                            entity: source.entity_id.clone(),
                        },
                        Some(source.instance_id),
                    )
                    == EntityLookup::Resolved(entity.id()))
            .then_some(entity.id())
        })
        .expect("the terminal run must retain its exact registered player");
    let observer_entity = world
        .add_observer(
            move |event: On<Add<SceneEntitySource>>,
                  snapshot: Res<WaveSnapshot>,
                  mut observer: ResMut<RetryCandidateObserver>,
                  players: Query<(Entity, &SceneEntitySource), With<PlayerRole>>,
                  mut commands: Commands| {
                if !snapshot.is_terminal() || observer.triggered {
                    return;
                }
                if old_player != event.entity && players.get(old_player).is_ok() {
                    observer.triggered = true;
                    commands.entity(old_player).despawn();
                }
            },
        )
        .id();
    world.flush();
    let mut observer = world.resource_mut::<RetryCandidateObserver>();
    observer.installed = true;
    observer.observer_entity = Some(observer_entity);
}

fn repair_retry_candidate_observer_after_rejection(world: &mut World) {
    let fixed_tick = world.resource::<FixedTime>().tick();
    let rejected = world.resource::<WaveRetryStatus>().last_rejection()
        == Some(WaveRetryRejection::ReplacementRejected);
    let repair_at_tick = world.resource::<RetryCandidateObserver>().repair_at_tick;
    if rejected && repair_at_tick.is_none() {
        world
            .resource_mut::<RetryCandidateObserver>()
            .repair_at_tick = fixed_tick.checked_add(1);
        return;
    }
    if repair_at_tick.is_none_or(|repair_at_tick| fixed_tick < repair_at_tick) {
        return;
    }
    let observer_entity = world
        .resource_mut::<RetryCandidateObserver>()
        .observer_entity
        .take()
        .expect("the installed Retry rejection observer should remain repairable");
    assert!(world.despawn(observer_entity));
    world
        .resource_mut::<RetryCandidateObserver>()
        .repair_at_tick = None;
}

pub(super) fn component_presenter_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ComponentPresenterPlugin>()
}

pub(super) fn retry_replacement_rejection_fixture_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<RetryReplacementRejectionFixturePlugin>()
}
