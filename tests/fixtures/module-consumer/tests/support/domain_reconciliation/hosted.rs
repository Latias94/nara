//! Two independent native plugins consume the public managed Project Scene travel protocol.

use nara_engine::{
    ProductRecipe,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId, PluginShutdownContext, StartupStage,
        VirtualTime,
    },
    core::ItemLimit,
    ecs::{
        Component, Entity, Resource, World,
        bevy_ecs::lifecycle::{Despawn, HookContext},
        bevy_ecs::observer::On,
        bevy_ecs::system::SystemState,
        bevy_ecs::world::DeferredWorld,
    },
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    identity::{EntityLookup, SpawnedSceneInstance, spawn_identity_entity},
    project::ProjectPath,
    project_host::{
        HeadlessRunIntent, HeadlessRunOutcome, PreparedProjectSceneView, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectScenes,
        advanced::{
            ManagedSceneDomain, ManagedSceneDomainAttempt, ManagedSceneDomainError,
            ManagedSceneDomainId, ManagedSceneDomains, RuntimeProbe,
            begin_prepared_project_scene_travel, headless_run_with_probe,
        },
    },
    scene::{
        SceneDocument, SceneEntityId, SceneEntityRecord,
        advanced::{SceneNativeRetirementClaims, SceneProductTransactionLimits},
    },
};
use nara_engine::{
    diagnostic::DiagnosticCode,
    project_host::{
        ProjectSceneRequestError,
        advanced::{ManagedSceneDomainPhase, ManagedSceneDomainPreparation},
    },
};
use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Recover,
    StopDuringRetirement,
    StopDuringReconcile,
    InvalidPreparation,
    Unclaimed,
    CancelBeforeRetirement,
    BaselineChangedBeforeRetirement,
    AdditionalRetirementClaimDrift,
    MissingCandidateBeforeRetirement,
    CleanupRetryDiagnostics,
    DeferredAssetMutation,
    GlobalDespawnObserver,
    CancelWithGlobalDespawnObserver,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    pub preparations: [usize; 2],
    pub retirement_calls: [usize; 2],
    pub reconciliation_calls: [usize; 2],
    pub live: [usize; 2],
    pub released: [usize; 2],
    pub hooks: [usize; 2],
    pub created: [usize; 2],
    pub attempt_drops: [usize; 2],
    pub shutdowns: [usize; 2],
    pub configured: usize,
    pub gameplay_frames: usize,
    pub paused_frames: usize,
}

#[derive(Clone, Resource)]
struct Control {
    scenario: Scenario,
    effects: Arc<Mutex<Effects>>,
}

struct Lease {
    domain: usize,
    effects: Arc<Mutex<Effects>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut effects = self.effects.lock().unwrap();
        effects.live[self.domain] -= 1;
        effects.released[self.domain] += 1;
    }
}

#[derive(Component)]
#[component(on_remove = removed_a)]
struct NativeA(Lease);
#[derive(Component)]
#[component(on_remove = removed_b)]
struct NativeB(Lease);
#[derive(Component)]
#[component(on_remove = unclaimed_removed)]
struct Unclaimed;
fn removed_a(world: DeferredWorld<'_>, context: HookContext) {
    world
        .get::<NativeA>(context.entity)
        .unwrap()
        .0
        .effects
        .lock()
        .unwrap()
        .hooks[0] += 1;
}
fn removed_b(world: DeferredWorld<'_>, context: HookContext) {
    world
        .get::<NativeB>(context.entity)
        .unwrap()
        .0
        .effects
        .lock()
        .unwrap()
        .hooks[1] += 1;
}
fn unclaimed_removed(_: DeferredWorld<'_>, _: HookContext) {}

#[derive(Default)]
struct PluginA;
#[derive(Default)]
struct PluginB;
impl Plugin for PluginA {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("consumer.travel.a"), PluginCategory::Runtime);
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        install(app, 0)
    }
    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        shutdown(context.world_mut(), 0);
        Ok(())
    }
}
impl Plugin for PluginB {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("consumer.travel.b"), PluginCategory::Runtime);
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        install(app, 1)
    }
    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        shutdown(context.world_mut(), 1);
        Ok(())
    }
}
fn install(app: &mut App, domain: usize) -> Result<(), PluginError> {
    app.init_resource::<ManagedSceneDomains>()?;
    let world = app.world_mut()?;
    if domain == 0 {
        world.register_component::<NativeA>();
    } else {
        world.register_component::<NativeB>();
    }
    world
        .resource_mut::<ManagedSceneDomains>()
        .register(Box::new(Domain(domain)))
        .unwrap();
    app.add_systems(StartupStage::Tooling, move |world: &mut World| {
        let entities = world
            .iter_entities()
            .filter(|entity| entity.contains::<nara_engine::scene::SceneEntitySource>())
            .map(|entity| entity.id())
            .collect::<Vec<_>>();
        for entity in entities {
            create(world, entity, domain);
        }
    })?;
    Ok(())
}
fn create(world: &mut World, entity: Entity, domain: usize) {
    let control = world.resource::<Control>().clone();
    {
        let mut effects = control.effects.lock().unwrap();
        effects.live[domain] += 1;
        effects.created[domain] += 1;
    }
    let lease = Lease {
        domain,
        effects: control.effects,
    };
    if domain == 0 {
        world.entity_mut(entity).insert(NativeA(lease));
    } else {
        world.entity_mut(entity).insert(NativeB(lease));
    }
}
fn remove(world: &mut World, entity: Entity, domain: usize) {
    if world.get_entity(entity).is_err() {
        return;
    }
    if domain == 0 {
        world.entity_mut(entity).remove::<NativeA>();
    } else {
        world.entity_mut(entity).remove::<NativeB>();
    }
}
fn shutdown(world: &mut World, domain: usize) {
    let control = world.resource::<Control>().clone();
    let entities = world
        .iter_entities()
        .filter(|entity| {
            if domain == 0 {
                entity.contains::<NativeA>()
            } else {
                entity.contains::<NativeB>()
            }
        })
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    for entity in entities {
        remove(world, entity, domain);
    }
    control.effects.lock().unwrap().shutdowns[domain] += 1;
}

struct Domain(usize);
impl ManagedSceneDomain for Domain {
    fn id(&self) -> ManagedSceneDomainId {
        ManagedSceneDomainId::new(if self.0 == 0 {
            "consumer.travel.a"
        } else {
            "consumer.travel.b"
        })
    }
    fn prepare(
        &self,
        world: &World,
        target: &PreparedProjectSceneView<'_>,
    ) -> Result<ManagedSceneDomainPreparation, ManagedSceneDomainError> {
        let control = world.resource::<Control>().clone();
        control.effects.lock().unwrap().preparations[self.0] += 1;
        let old = target
            .expected_instance()
            .entity_ids()
            .iter()
            .map(|id| match target.expected_instance().resolve(world, id) {
                EntityLookup::Resolved(entity) => entity,
                other => panic!("old entity must resolve: {other:?}"),
            })
            .collect();
        let claims = SceneNativeRetirementClaims::from_component_ids([if self.0 == 0 {
            world.component_id::<NativeA>().unwrap()
        } else {
            world.component_id::<NativeB>().unwrap()
        }]);
        Ok(ManagedSceneDomainPreparation::new(
            claims,
            Attempt {
                domain: self.0,
                control,
                old,
                cursor: 0,
                reconcile_cursor: 0,
                failed_retirement: false,
                failed_reconcile: false,
            },
        ))
    }
}
struct Attempt {
    domain: usize,
    control: Control,
    old: Vec<Entity>,
    cursor: usize,
    reconcile_cursor: usize,
    failed_retirement: bool,
    failed_reconcile: bool,
}
impl Drop for Attempt {
    fn drop(&mut self) {
        self.control.effects.lock().unwrap().attempt_drops[self.domain] += 1;
    }
}
fn domain_error(code: &'static str) -> ManagedSceneDomainError {
    ManagedSceneDomainError::new(DiagnosticCode::new(code).unwrap())
}
impl ManagedSceneDomainAttempt for Attempt {
    fn retire(
        &mut self,
        world: &mut World,
        _: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError> {
        self.control.effects.lock().unwrap().retirement_calls[self.domain] += 1;
        if self.domain == 0 && self.control.scenario == Scenario::DeferredAssetMutation {
            world.commands().queue(|world: &mut World| {
                let mut state = nara_ecs::bevy_ecs::system::SystemState::<(
                    nara_engine::ecs::ResMut<nara_engine::asset::ProjectAssetDatabase>,
                )>::new(world);
                state
                    .get_mut(world)
                    .unwrap()
                    .0
                    .insert(nara_engine::asset::AssetRecord::new(
                        nara_engine::asset::StableAssetId::parse_str(
                            "77777777-7777-4777-8777-777777777777",
                        )
                        .unwrap(),
                        nara_engine::asset::AssetPath::new("deferred.png").unwrap(),
                        nara_engine::asset::AssetSourceKind::Image,
                    ))
                    .unwrap();
            });
        }
        while let Some(entity) = self.old.get(self.cursor).copied() {
            remove(world, entity, self.domain);
            self.cursor += 1;
            if self.domain == 1
                && !self.failed_retirement
                && matches!(
                    self.control.scenario,
                    Scenario::Recover | Scenario::StopDuringRetirement
                )
            {
                self.failed_retirement = true;
                return Err(domain_error("consumer.travel.retirement-failed"));
            }
        }
        Ok(())
    }
    fn reconcile(
        &mut self,
        world: &mut World,
        published: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError> {
        self.control.effects.lock().unwrap().reconciliation_calls[self.domain] += 1;
        while let Some(id) = published.entity_ids().get(self.reconcile_cursor) {
            let EntityLookup::Resolved(entity) = published.resolve(world, id) else {
                panic!("published entity must resolve")
            };
            create(world, entity, self.domain);
            self.reconcile_cursor += 1;
            if self.domain == 1
                && !self.failed_reconcile
                && matches!(
                    self.control.scenario,
                    Scenario::Recover | Scenario::StopDuringReconcile
                )
            {
                self.failed_reconcile = true;
                return Err(domain_error("consumer.travel.reconcile-failed"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default, Resource)]
struct Outcome {
    done: bool,
}
#[derive(Resource, Default)]
struct Progress {
    request: Option<ProjectSceneRequestId>,
    begun: bool,
    degradation: u8,
    waited: bool,
    gameplay: usize,
    elapsed: Duration,
    entities_before_begin: u32,
    cleanup_observer: Option<Entity>,
}
struct Controller(Control);
impl Plugin for Controller {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.travel.controller"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .init_resource::<Outcome>()?
            .init_resource::<Progress>()?
            .add_systems(CoreStage::Update, gameplay)?
            .add_systems(CoreStage::Last, maintenance)?;
        Ok(())
    }
}
fn gameplay(world: &mut World) {
    world
        .resource::<Control>()
        .effects
        .lock()
        .unwrap()
        .gameplay_frames += 1;
}

fn maintenance(world: &mut World) {
    let control = world.resource::<Control>().clone();
    if world.resource::<Outcome>().done {
        return;
    }
    if world.resource::<Progress>().request.is_none() {
        let request = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            access
                .get_mut(world)
                .unwrap()
                .request(ProjectPath::new("next.scene.json").unwrap())
                .unwrap()
        };
        world.resource_mut::<Progress>().request = Some(request);
        return;
    }
    let request = world.resource::<Progress>().request.unwrap();
    let state = {
        let mut access = SystemState::<ProjectScenes>::new(world);
        access.get_mut(world).unwrap().state(request).unwrap()
    };
    if !world.resource::<Progress>().begun {
        if state != ProjectSceneRequestState::Ready {
            std::thread::yield_now();
            return;
        }
        if control.scenario == Scenario::Unclaimed {
            let entity = world
                .iter_entities()
                .find(|entity| entity.contains::<NativeA>())
                .unwrap()
                .id();
            world.entity_mut(entity).insert(Unclaimed);
        }
        let limit = if control.scenario == Scenario::InvalidPreparation {
            SceneProductTransactionLimits::MAX_OVERLAY_WRITES + 1
        } else {
            1
        };
        let additional_retirements = if control.scenario == Scenario::AdditionalRetirementClaimDrift
        {
            vec![spawn_identity_entity(world).unwrap()]
        } else {
            Vec::new()
        };
        let before_begin = world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>();
        let entities_before_begin = u32::try_from(before_begin.len()).unwrap();
        let effects = Arc::clone(&control.effects);
        let result = begin_prepared_project_scene_travel(
            world,
            request,
            SceneProductTransactionLimits::new(ItemLimit::new(limit).unwrap(), ItemLimit::ONE),
            &additional_retirements,
            move |_| {
                effects.lock().unwrap().configured += 1;
            },
        );
        if matches!(
            control.scenario,
            Scenario::InvalidPreparation | Scenario::Unclaimed
        ) {
            assert!(result.is_err());
            let effects = control.effects.lock().unwrap();
            assert_eq!(effects.live, [2, 2]);
            assert_eq!(effects.retirement_calls, [0, 0]);
            assert_eq!(effects.hooks, [0, 0]);
            drop(effects);
            world.resource_mut::<Outcome>().done = true;
            return;
        }
        result.unwrap_or_else(|report| {
            panic!("managed preparation rejected: {:?}", report.diagnostics)
        });
        if control.scenario == Scenario::AdditionalRetirementClaimDrift {
            create(world, additional_retirements[0].entity(), 0);
        } else if matches!(
            control.scenario,
            Scenario::MissingCandidateBeforeRetirement | Scenario::CleanupRetryDiagnostics
        ) {
            let scratch = world
                .iter_entities()
                .find(|entity| !before_begin.contains(&entity.id()))
                .unwrap()
                .id();
            if control.scenario == Scenario::MissingCandidateBeforeRetirement {
                world.commands().queue(move |world: &mut World| {
                    world.entity_mut(scratch).despawn();
                });
            } else {
                let observer = world
                    .spawn(
                        nara_engine::ecs::bevy_ecs::observer::Observer::new(
                            |_: On<Despawn<()>>| {},
                        )
                        .with_entity(scratch),
                    )
                    .id();
                world.flush();
                world.resource_mut::<Progress>().cleanup_observer = Some(observer);
            }
        }
        if matches!(
            control.scenario,
            Scenario::GlobalDespawnObserver | Scenario::CancelWithGlobalDespawnObserver
        ) {
            world.commands().queue(|world: &mut World| {
                world.spawn(nara_engine::ecs::bevy_ecs::observer::Observer::new(
                    |_: On<Despawn<()>>| {},
                ));
            });
            world.flush();
        }
        let gameplay = control.effects.lock().unwrap().gameplay_frames;
        let elapsed = world.resource::<VirtualTime>().elapsed;
        let mut progress = world.resource_mut::<Progress>();
        progress.begun = true;
        progress.gameplay = gameplay;
        progress.elapsed = elapsed;
        progress.entities_before_begin = entities_before_begin;
        drop(progress);
        if control.scenario == Scenario::CancelBeforeRetirement {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let mut scenes = access.get_mut(world).unwrap();
            assert!(scenes.cancel(request));
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Cancelling)
            );
        } else if control.scenario == Scenario::CancelWithGlobalDespawnObserver {
            let mut access = SystemState::<ProjectScenes>::new(world);
            assert!(access.get_mut(world).unwrap().cancel(request));
        } else if control.scenario == Scenario::BaselineChangedBeforeRetirement {
            let entity = world
                .iter_entities()
                .find(|entity| entity.contains::<NativeA>())
                .unwrap()
                .id();
            world.entity_mut(entity).insert(Unclaimed);
        }
        return;
    }
    if control.scenario == Scenario::CleanupRetryDiagnostics
        && state == ProjectSceneRequestState::Degraded
    {
        {
            let effects = control.effects.lock().unwrap();
            assert_eq!(effects.retirement_calls, [0, 0]);
            assert_eq!(effects.live, [2, 2]);
        }
        let observer = world
            .resource_mut::<Progress>()
            .cleanup_observer
            .take()
            .expect("the first cleanup failure retains its observer");
        {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            for code in [
                "scene.prepared-baseline-changed",
                "project.scene.candidate-cleanup-incomplete",
            ] {
                assert!(
                    scenes
                        .diagnostics(request)
                        .unwrap()
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str() == code)
                );
            }
        }
        world.entity_mut(observer).despawn();
        world.flush();
        let mut access = SystemState::<ProjectScenes>::new(world);
        assert!(access.get_mut(world).unwrap().retry(request));
        return;
    }
    if matches!(
        control.scenario,
        Scenario::CancelBeforeRetirement
            | Scenario::BaselineChangedBeforeRetirement
            | Scenario::AdditionalRetirementClaimDrift
            | Scenario::MissingCandidateBeforeRetirement
            | Scenario::CleanupRetryDiagnostics
    ) {
        let expected = if control.scenario == Scenario::CancelBeforeRetirement {
            ProjectSceneRequestState::Cancelled
        } else {
            ProjectSceneRequestState::Rejected(ProjectSceneRequestError::CommitRejected)
        };
        assert_eq!(
            state, expected,
            "intact rejection is terminal without requiring a retry or cancellation"
        );
        let effects = control.effects.lock().unwrap();
        assert_eq!(effects.retirement_calls, [0, 0]);
        assert_eq!(effects.reconciliation_calls, [0, 0]);
        assert_eq!(
            effects.live,
            if control.scenario == Scenario::AdditionalRetirementClaimDrift {
                [3, 2]
            } else {
                [2, 2]
            }
        );
        assert_eq!(effects.hooks, [0, 0]);
        assert_eq!(effects.attempt_drops, [1, 1]);
        let current_entities = u32::try_from(world.iter_entities().count()).unwrap();
        assert_eq!(
            current_entities,
            world.resource::<Progress>().entities_before_begin,
            "discard removes the unpublished candidate"
        );
        if effects.gameplay_frames == world.resource::<Progress>().gameplay {
            assert_eq!(
                world.resource::<VirtualTime>().elapsed,
                world.resource::<Progress>().elapsed
            );
            return;
        }
        assert!(
            world.resource::<VirtualTime>().elapsed > world.resource::<Progress>().elapsed,
            "intact terminal results release the frame-entry simulation gate on the following frame"
        );
        drop(effects);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        if control.scenario != Scenario::CancelBeforeRetirement {
            assert!(
                scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "scene.prepared-baseline-changed")
            );
        }
        if control.scenario == Scenario::CleanupRetryDiagnostics {
            assert!(
                !scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code().as_str() == "project.scene.candidate-cleanup-incomplete"
                    }),
                "successful cleanup removes transient cleanup errors but retains its original rejection"
            );
        }
        assert!(scenes.acknowledge(request));
        assert_eq!(scenes.state(request), None);
        world.resource_mut::<Outcome>().done = true;
        return;
    }
    let mut effects = control.effects.lock().unwrap();
    assert_eq!(
        effects.gameplay_frames,
        world.resource::<Progress>().gameplay,
        "gameplay stays suspended throughout travel"
    );
    assert_eq!(
        world.resource::<VirtualTime>().elapsed,
        world.resource::<Progress>().elapsed
    );
    effects.paused_frames += 1;
    if state == ProjectSceneRequestState::Degraded {
        if matches!(
            control.scenario,
            Scenario::GlobalDespawnObserver | Scenario::CancelWithGlobalDespawnObserver
        ) {
            assert!(
                world.resource::<Progress>().entities_before_begin
                    < u32::try_from(world.iter_entities().count()).unwrap(),
                "unpublished candidate entities remain owned by the request after cleanup refusal"
            );
            assert_eq!(effects.retirement_calls, [0, 0]);
            assert_eq!(effects.reconciliation_calls, [0, 0]);
            assert_eq!(effects.live, [2, 2]);
            drop(effects);
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            let expected_code = if control.scenario == Scenario::CancelWithGlobalDespawnObserver {
                "project.scene.candidate-cleanup-incomplete"
            } else {
                "project.scene.request-lifecycle-unsupported"
            };
            assert!(
                scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == expected_code),
                "cleanup refusal should be reported: {:#?}",
                scenes.diagnostics(request)
            );
            world.resource_mut::<Outcome>().done = true;
            return;
        }
        if control.scenario == Scenario::DeferredAssetMutation {
            assert_eq!(effects.retirement_calls, [1, 1]);
            assert_eq!(effects.hooks, [2, 2]);
            drop(effects);
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            assert!(
                scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "project.scene.asset-catalog-changed")
            );
            world.resource_mut::<Outcome>().done = true;
            return;
        }
        if matches!(
            control.scenario,
            Scenario::StopDuringRetirement | Scenario::StopDuringReconcile
        ) {
            assert_eq!(
                effects.attempt_drops,
                [0, 0],
                "Stop retains every domain attempt until physical close"
            );
            if control.scenario == Scenario::StopDuringRetirement {
                assert_eq!(effects.live, [0, 1]);
            } else {
                assert_eq!(effects.live, [2, 1]);
            }
            drop(effects);
            world.resource_mut::<Outcome>().done = true;
            return;
        }
        let mut progress = world.resource_mut::<Progress>();
        if !progress.waited {
            progress.waited = true;
            return;
        }
        progress.waited = false;
        progress.degradation += 1;
        let phases = if progress.degradation == 1 {
            assert_eq!(effects.retirement_calls, [1, 1]);
            [
                ManagedSceneDomainPhase::Retired,
                ManagedSceneDomainPhase::Retiring,
            ]
        } else {
            assert_eq!(effects.retirement_calls, [1, 2]);
            assert_eq!(effects.reconciliation_calls, [1, 1]);
            assert_eq!(effects.created, [4, 3]);
            assert_eq!(effects.live, [2, 1]);
            [
                ManagedSceneDomainPhase::Reconciled,
                ManagedSceneDomainPhase::Reconciling,
            ]
        };
        drop(progress);
        drop(effects);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let mut scenes = access.get_mut(world).unwrap();
        for (domain, phase) in ["consumer.travel.a", "consumer.travel.b"]
            .into_iter()
            .zip(phases)
        {
            assert_eq!(
                scenes.native_domain_phase(request, ManagedSceneDomainId::new(domain)),
                Some(phase)
            );
        }
        assert!(scenes.diagnostics(request).unwrap().has_errors());
        assert!(
            !scenes.cancel(request),
            "native retirement custody rejects cancellation"
        );
        assert!(scenes.retry(request));
    } else if state == ProjectSceneRequestState::Committed {
        assert_eq!(effects.retirement_calls, [1, 2]);
        assert_eq!(effects.reconciliation_calls, [1, 2]);
        assert_eq!(effects.live, [2, 2]);
        assert_eq!(effects.created, [4, 4]);
        assert_eq!(effects.configured, 1);
        assert_eq!(world.resource::<Progress>().degradation, 2);
        drop(effects);
        let mut access = SystemState::<ProjectScenes>::new(world);
        let scenes = access.get_mut(world).unwrap();
        assert!(
            !scenes.diagnostics(request).unwrap().has_errors(),
            "successful retry must not retain the resolved native failure as a final error"
        );
        world.resource_mut::<Outcome>().done = true;
    }
}

pub fn run(scenario: Scenario) -> Effects {
    let project = Project::new();
    let control = Control {
        scenario,
        effects: Arc::new(Mutex::new(Effects::default())),
    };
    let capture = control.clone();
    let probe = RuntimeProbe::new(PluginDefinition::infallible::<Controller, _>(
        PluginDefinitionId::new("consumer.travel.controller", 1),
        b"native-travel",
        move || Controller(capture.clone()),
    ));
    let recipe = ProductRecipe::new()
        .add_plugin::<PluginA>()
        .unwrap()
        .add_plugin::<PluginB>()
        .unwrap();
    let intent =
        HeadlessRunIntent::frames(NonZeroU32::new(4000).unwrap(), Duration::from_millis(1))
            .with_recipe(recipe)
            .stop_when(|outcome: &Outcome| outcome.done);
    let mut run = headless_run_with_probe(project.capability(), intent, probe, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                assert!(outcome.done);
                break;
            }
            HeadlessRunOutcome::Failed => {
                panic!("hosted native travel failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
    }
    drop(run);
    let effects = control.effects.lock().unwrap().clone();
    assert_eq!(effects.live, [0, 0]);
    assert_eq!(effects.shutdowns, [1, 1]);
    assert_eq!(effects.attempt_drops, [1, 1]);
    assert_eq!(effects.hooks, effects.released);
    effects
}

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_managed_native_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = 'c40c55e1-345b-5c21-936f-42ba01306c28'\nnative_document_identity = 'project_location'\nname = \"Native travel\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[content]\nscenes = [\"next.scene.json\"]\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        for path in ["startup.scene.json", "next.scene.json"] {
            let scene = SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(SceneEntityId::new("one").unwrap()),
                SceneEntityRecord::new(SceneEntityId::new("two").unwrap()),
            ]);
            fs::write(
                root.join("scenes").join(path),
                scene.to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("assets/deferred.png.meta"),
            nara_engine::asset::AssetMeta::new(
                nara_engine::asset::AssetMetaIdentity::new(
                    nara_engine::asset::StableAssetId::parse_str(
                        "77777777-7777-4777-8777-777777777777",
                    )
                    .unwrap(),
                    nara_engine::asset::AssetPath::new("deferred.png").unwrap(),
                    nara_engine::asset::AssetSourceKind::Image,
                ),
                nara_engine::image::ImageImportSettings::default(),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("assets/deferred.png"),
            include_bytes!("../../../../../fixtures/images/valid-rgba-1x1.png"),
        )
        .unwrap();
        let project = Self(root);
        let capability = project.capability();
        let manifest = capability
            .open_file(&nara_engine::fs::RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate =
            nara_engine::project_host::ingest_project_manifest(&manifest, None).unwrap();
        let recipe = ProductRecipe::new()
            .add_plugin::<PluginA>()
            .unwrap()
            .add_plugin::<PluginB>()
            .unwrap();
        let plan = nara_engine::project_host::resolve_product_recipe(&candidate, recipe).unwrap();
        fs::write(
            project.0.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }
    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.0)
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(&self.0).unwrap();
        DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
