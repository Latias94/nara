//! Provisional startup-scene activation boundary for advanced product integrations.
//!
//! Project Hosts use the same materialization operation as direct managed-App integrations. The
//! operation owns the retained source, creates the scene receipt itself, and publishes the pair
//! only after scene spawning succeeds, so callers cannot combine unrelated documents and receipts.

use std::{
    error::Error,
    fmt,
    sync::{Arc, Weak},
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, RuntimeFaultDetail,
    StartupStage,
};
use nara_core::ByteLimit;
use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};
use nara_ecs::{
    Res, ResMut, Resource, SystemSet, World,
    change_detection::Tick,
    component::ComponentId,
    error::BevyError,
    query::FilteredAccessSet,
    schedule::IntoScheduleConfigs,
    system::{ReadOnlySystemParam, SystemMeta, SystemParam, SystemParamValidationError},
    world::unsafe_world_cell::UnsafeWorldCell,
};
use nara_identity::SpawnedSceneInstance;
use nara_reflect::{ComponentRegistry, component_registry, validate_component_registry_authority};
use nara_scene::{SceneDocument, spawn_scene};

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use crate::project_content::ProjectContentLease;
use crate::scene_retention::direct_startup_scene_retained_bytes;

/// Stable plugin identity for the provisional advanced startup-scene activation seam.
pub const STARTUP_SCENE_ACTIVATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.startup-scene-activation");
const STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STARTUP_SCENE_ACTIVATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara_scene::SCENE_COMPONENTS_PLUGIN_ID]);

/// Validation failure while binding a direct managed-App startup source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupSceneSourceError {
    /// Logical retained-byte accounting overflowed `usize`.
    RetainedBytesOverflow,
    /// The exact logical retained-byte charge exceeds the caller's declared limit.
    RetainedBytesExceeded { required: usize, limit: usize },
}

impl fmt::Display for StartupSceneSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RetainedBytesOverflow => "startup scene retained-byte accounting overflowed",
            Self::RetainedBytesExceeded { .. } => {
                "startup scene exceeds the direct retained-byte limit"
            }
        })
    }
}

impl Error for StartupSceneSourceError {}

enum StartupSceneRetention {
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    Project {
        _lease: ProjectContentLease,
        retained_bytes: usize,
    },
    Direct {
        retained_bytes: usize,
        _limit: usize,
    },
}

/// Move-only retained source accepted by [`materialize_startup_scene`].
///
/// File-backed callers receive this value from Project Content. Direct managed-App callers use
/// [`StartupSceneSource::direct`] and must declare a deterministic logical retained-byte limit.
/// That direct limit is neither allocator-capacity nor RSS accounting and does not claim a Project
/// Content reservation. The value deliberately does not implement `Clone`: exactly one
/// materialization consumes exactly one source owner.
pub struct StartupSceneSource {
    document: Arc<SceneDocument>,
    retention: StartupSceneRetention,
}

/// Non-owning view of the startup source retained by the root activation authority.
///
/// The view deliberately holds neither a strong document reference nor the Project Content lease.
/// Clones therefore cannot extend retained memory beyond candidate or runtime retirement. Product
/// code may inspect the source only through a bounded borrow while the private root owner remains
/// alive. The move-only [`StartupSceneSource`] remains the only materialization input.
#[derive(Clone)]
pub struct StartupSceneSourceView {
    document: Weak<SceneDocument>,
    retained_bytes: usize,
}

impl StartupSceneSource {
    /// Binds an owned source for a direct managed-App integration.
    ///
    /// This path enforces its explicit limit but does not claim or imply a
    /// `ProjectContentBudgetHost` reservation.
    pub fn direct(
        document: Arc<SceneDocument>,
        retained_byte_limit: ByteLimit,
    ) -> Result<Self, StartupSceneSourceError> {
        let retained_bytes = direct_startup_scene_retained_bytes(&document)
            .map_err(|_| StartupSceneSourceError::RetainedBytesOverflow)?;
        let limit = retained_byte_limit.get();
        if retained_bytes > limit {
            return Err(StartupSceneSourceError::RetainedBytesExceeded {
                required: retained_bytes,
                limit,
            });
        }
        Ok(Self {
            document,
            retention: StartupSceneRetention::Direct {
                retained_bytes,
                _limit: limit,
            },
        })
    }

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn from_project_content(
        document: Arc<SceneDocument>,
        lease: ProjectContentLease,
        retained_bytes: usize,
    ) -> Self {
        Self {
            document,
            retention: StartupSceneRetention::Project {
                _lease: lease,
                retained_bytes,
            },
        }
    }

    #[must_use]
    pub(crate) fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub(crate) const fn retained_bytes(&self) -> usize {
        match &self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            StartupSceneRetention::Project { retained_bytes, .. } => *retained_bytes,
            StartupSceneRetention::Direct { retained_bytes, .. } => *retained_bytes,
        }
    }

    #[cfg(test)]
    pub(crate) fn direct_limit(&self) -> Option<usize> {
        match self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            StartupSceneRetention::Project { .. } => None,
            StartupSceneRetention::Direct { _limit, .. } => Some(_limit),
        }
    }
}

impl fmt::Debug for StartupSceneSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupSceneSource")
            .field("entity_count", &self.document.entities.len())
            .field("retained_bytes", &self.retained_bytes())
            .field(
                "retention",
                &match self.retention {
                    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                    StartupSceneRetention::Project { .. } => "project-content",
                    StartupSceneRetention::Direct { .. } => "direct-limit",
                },
            )
            .finish_non_exhaustive()
    }
}

impl StartupSceneSourceView {
    /// Borrows the exact immutable document while the root activation authority retains it.
    ///
    /// Returns `None` after that authority has retired. The temporary strong reference is kept
    /// inside this method so the view cannot accidentally extend the retained allocation.
    pub fn with_document<R>(&self, inspect: impl FnOnce(&SceneDocument) -> R) -> Option<R> {
        self.document
            .upgrade()
            .map(|document| inspect(document.as_ref()))
    }

    /// Returns the logical retained-byte charge owned by the root activation authority.
    #[must_use]
    pub(crate) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

impl fmt::Debug for StartupSceneSourceView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupSceneSourceView")
            .field("available", &(self.document.strong_count() > 0))
            .field("retained_bytes", &self.retained_bytes())
            .finish_non_exhaustive()
    }
}

/// Read-only product view of one exact startup source and its matching successful spawn receipt.
///
/// The activation plugin promotes a private one-shot materialization input into a private owner
/// before product-dependent Startup systems run. That root resource, which downstream code
/// cannot name or remove, owns the retention guard for the candidate and runtime lifetime. Product
/// systems receive this read-only system parameter; replacement ownership remains outside this
/// provisional surface.
pub struct StartupSceneActivation<'w> {
    owner: Res<'w, StartupSceneActivationOwner>,
}

#[derive(Resource)]
pub(crate) struct StartupSceneActivationOwner {
    source: StartupSceneSource,
    receipt: SpawnedSceneInstance,
}

impl StartupSceneActivation<'_> {
    /// Returns the exact immutable source admitted for the active runtime.
    #[must_use]
    pub fn source(&self) -> &SceneDocument {
        self.owner.source()
    }

    /// Returns the successful receipt paired with the admitted source.
    #[must_use]
    pub fn receipt(&self) -> &SpawnedSceneInstance {
        self.owner.receipt()
    }

    /// Returns a non-owning view of the exact startup source for product-local Retry.
    ///
    /// The private root activation owner remains the sole retention authority. Cloning or leaking
    /// this view cannot keep its document or Project Content lease alive after runtime retirement.
    #[must_use]
    pub fn source_view(&self) -> StartupSceneSourceView {
        StartupSceneSourceView {
            document: Arc::downgrade(&self.owner.source.document),
            retained_bytes: self.owner.source.retained_bytes(),
        }
    }
}

impl StartupSceneActivationOwner {
    pub(crate) fn source(&self) -> &SceneDocument {
        self.source.document()
    }

    pub(crate) const fn receipt(&self) -> &SpawnedSceneInstance {
        &self.receipt
    }
}

impl fmt::Debug for StartupSceneActivation<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupSceneActivation")
            .field("source", &self.owner.source)
            .field("instance_id", &self.owner.receipt.instance_id())
            .field("entity_count", &self.owner.receipt.len())
            .finish_non_exhaustive()
    }
}

// SAFETY: Every operation delegates to `Res<StartupSceneActivationOwner>` and therefore registers
// and performs only the same read access to the private owner resource.
unsafe impl SystemParam for StartupSceneActivation<'_> {
    type State = ComponentId;
    type Item<'world, 'state> = StartupSceneActivation<'world>;

    fn init_state(world: &mut World) -> Self::State {
        <Res<'_, StartupSceneActivationOwner> as SystemParam>::init_state(world)
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        component_access_set: &mut FilteredAccessSet,
        world: &mut World,
    ) {
        <Res<'_, StartupSceneActivationOwner> as SystemParam>::init_access(
            state,
            system_meta,
            component_access_set,
            world,
        );
    }

    unsafe fn get_param<'world, 'state>(
        state: &'state mut Self::State,
        system_meta: &SystemMeta,
        world: UnsafeWorldCell<'world>,
        change_tick: Tick,
    ) -> Result<Self::Item<'world, 'state>, SystemParamValidationError> {
        // SAFETY: this method has exactly the access registered by the delegated `Res` parameter.
        let owner = unsafe {
            <Res<'_, StartupSceneActivationOwner> as SystemParam>::get_param(
                state,
                system_meta,
                world,
                change_tick,
            )?
        };
        Ok(StartupSceneActivation { owner })
    }
}

// SAFETY: the delegated resource parameter is read-only.
unsafe impl ReadOnlySystemParam for StartupSceneActivation<'_> {}

#[derive(Resource)]
struct StartupSceneActivationInput(StartupSceneActivationOwner);

#[derive(Resource)]
struct StartupSceneActivationAuthority {
    phase: StartupSceneActivationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupSceneActivationPhase {
    Open,
    Materialized,
    Finalized,
}

/// Provisional ordering anchor for product systems that consume startup-scene activation.
///
/// The engine promotes the one-shot input before this set and finalizes the activation window
/// afterward. Those internal phases are deliberately not public schedule anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct StartupSceneActivationSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum StartupSceneActivationInternalSet {
    Consume,
    Finalize,
}

/// Installs the provisional startup-scene activation phases.
#[derive(Debug, Default, Clone, Copy)]
pub struct StartupSceneActivationPlugin;

impl Plugin for StartupSceneActivationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(StartupSceneActivationAuthority {
            phase: StartupSceneActivationPhase::Open,
        })?
        .configure_sets(
            StartupStage::Runtime,
            (
                StartupSceneActivationInternalSet::Consume,
                StartupSceneActivationSet,
                StartupSceneActivationInternalSet::Finalize,
            )
                .chain(),
        )?
        .add_systems(
            StartupStage::Runtime,
            consume_startup_scene_activation.in_set(StartupSceneActivationInternalSet::Consume),
        )?
        .add_systems(
            StartupStage::Runtime,
            finalize_startup_scene_activation.in_set(StartupSceneActivationInternalSet::Finalize),
        )?;
        Ok(())
    }
}

/// Spawns one retained source and publishes only its matching successful receipt.
///
/// The operation must run while the caller has exclusive access to an unpublished managed-App
/// candidate. Every predictable rejection occurs before scene spawning. A scene failure publishes
/// neither pending input nor activation. The executable component registry is always derived from
/// and validated against the candidate World; callers cannot substitute another registry.
pub fn materialize_startup_scene(
    world: &mut World,
    source: StartupSceneSource,
) -> Result<DiagnosticReport, StartupSceneMaterializeError> {
    if !world.contains_resource::<StartupSceneActivationAuthority>() {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.activation-authority-missing",
            "Startup scene activation authority is unavailable",
        ));
    }
    match world.resource::<StartupSceneActivationAuthority>().phase {
        StartupSceneActivationPhase::Open => {}
        StartupSceneActivationPhase::Materialized => {
            return Err(StartupSceneMaterializeError::single(
                "startup.scene.activation-already-present",
                "Startup scene activation is already present",
            ));
        }
        StartupSceneActivationPhase::Finalized => {
            return Err(StartupSceneMaterializeError::single(
                "startup.scene.activation-window-closed",
                "Startup scene activation window is closed",
            ));
        }
    }
    if world.contains_resource::<StartupSceneActivationInput>()
        || world.contains_resource::<StartupSceneActivationOwner>()
    {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.activation-state-conflict",
            "Startup scene activation state conflicts with its open authority",
        ));
    }

    validate_component_registry_authority(world).map_err(|_| {
        StartupSceneMaterializeError::single(
            "startup.scene.component-registry-authority-invalid",
            "Startup scene component registry authority is invalid",
        )
    })?;
    let registry = component_registry(world)
        .and_then(|registry| registry.snapshot().ok())
        .map(ComponentRegistry::from_snapshot)
        .ok_or_else(|| {
            StartupSceneMaterializeError::single(
                "startup.scene.component-registry-unavailable",
                "Startup scene component registry is unavailable",
            )
        })?;

    let mut report = spawn_scene(world, &registry, source.document());
    if report.diagnostics.has_errors() {
        return Err(StartupSceneMaterializeError::new(report.diagnostics));
    }
    let receipt = report
        .instance
        .take()
        .expect("successful scene spawning always publishes an instance receipt");
    world.insert_resource(StartupSceneActivationInput(StartupSceneActivationOwner {
        source,
        receipt,
    }));
    world
        .resource_mut::<StartupSceneActivationAuthority>()
        .phase = StartupSceneActivationPhase::Materialized;
    Ok(report.diagnostics)
}

fn consume_startup_scene_activation(world: &mut World) -> Result<(), BevyError> {
    if !world.contains_resource::<StartupSceneActivationInput>() {
        return Ok(());
    }
    if world.contains_resource::<StartupSceneActivationOwner>() {
        return Err(classified_startup_error(
            "startup.scene.activation-conflict",
            "Startup scene activation authority conflicts with pending input",
        ));
    }
    let pending = world
        .remove_resource::<StartupSceneActivationInput>()
        .expect("the exclusive consume system observed one pending input");
    world.insert_resource(pending.0);
    Ok(())
}

fn finalize_startup_scene_activation(
    mut authority: ResMut<StartupSceneActivationAuthority>,
    pending: Option<Res<StartupSceneActivationInput>>,
    active: Option<Res<StartupSceneActivationOwner>>,
) -> Result<(), BevyError> {
    let phase = authority.phase;
    authority.phase = StartupSceneActivationPhase::Finalized;
    if pending.is_some() {
        return Err(classified_startup_error(
            "startup.scene.activation-unconsumed",
            "Startup scene activation input was not promoted",
        ));
    }
    if phase == StartupSceneActivationPhase::Materialized && active.is_none() {
        return Err(classified_startup_error(
            "startup.scene.activation-owner-missing",
            "Startup scene activation owner is unavailable",
        ));
    }
    if phase == StartupSceneActivationPhase::Open && active.is_some() {
        return Err(classified_startup_error(
            "startup.scene.activation-owner-unexpected",
            "Startup scene activation owner exists without materialization authority",
        ));
    }
    Ok(())
}

fn classified_startup_error(code: &'static str, summary: &'static str) -> BevyError {
    let detail = RuntimeFaultDetail::new(code, summary, "nara.startup-scene")
        .expect("engine-owned startup diagnostic metadata is valid");
    detail.into_bevy_error()
}

/// Failure from the sealed startup-scene materialization operation.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupSceneMaterializeError {
    diagnostics: Box<DiagnosticReport>,
}

impl StartupSceneMaterializeError {
    fn new(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
        }
    }

    fn single(code: &'static str, summary: &'static str) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
            SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
        ));
        Self::new(diagnostics)
    }

    /// Returns the structured diagnostics that rejected materialization.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}

impl fmt::Display for StartupSceneMaterializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("startup scene materialization failed")
    }
}

impl Error for StartupSceneMaterializeError {}

#[cfg(test)]
mod tests {
    use std::{sync::OnceLock, time::Duration};

    use super::*;

    #[test]
    fn direct_source_enforces_its_exact_retained_byte_limit() {
        let document = Arc::new(SceneDocument::new([]));
        let required = direct_startup_scene_retained_bytes(&document).unwrap();
        assert!(required > 1);

        let source =
            StartupSceneSource::direct(Arc::clone(&document), ByteLimit::new(required).unwrap())
                .unwrap();

        assert!(std::ptr::eq(source.document(), document.as_ref()));
        assert_eq!(source.retained_bytes(), required);
        assert_eq!(source.direct_limit(), Some(required));
        assert_eq!(
            StartupSceneSource::direct(
                document,
                ByteLimit::new(required.checked_sub(1).unwrap()).unwrap(),
            )
            .unwrap_err(),
            StartupSceneSourceError::RetainedBytesExceeded {
                required,
                limit: required - 1,
            }
        );
    }

    #[test]
    fn source_view_cannot_extend_the_root_document_lifetime() {
        let document = Arc::new(SceneDocument::new([]));
        let view = StartupSceneSourceView {
            document: Arc::downgrade(&document),
            retained_bytes: 17,
        };
        let clone = view.clone();

        assert_eq!(Arc::strong_count(&document), 1);
        assert_eq!(view.with_document(|source| source.entities.len()), Some(0));
        assert_eq!(clone.retained_bytes(), 17);

        drop(document);
        assert_eq!(view.with_document(|source| source.entities.len()), None);
        assert_eq!(clone.with_document(|source| source.entities.len()), None);
    }

    #[test]
    fn direct_materialization_uses_candidate_registry_and_promotes_once() {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_scene::SceneComponentsPlugin,
            StartupSceneActivationPlugin,
        ))
        .unwrap();
        let sealed = app.seal().unwrap();
        let mut candidate = nara_app::RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                sealed,
                nara_app::RuntimeObligationLedger::new(),
                nara_app::RuntimeClosePolicy::default(),
            )
            .unwrap();
        let source = StartupSceneSource::direct(
            Arc::new(SceneDocument::new([])),
            ByteLimit::new(1024).unwrap(),
        )
        .unwrap();
        let materialization = Arc::new(OnceLock::new());
        let command_result = Arc::clone(&materialization);

        candidate
            .with_admission_scope(move |scope| {
                scope.apply_command(move |world: &mut World| {
                    assert!(
                        command_result
                            .set(materialize_startup_scene(world, source))
                            .is_ok(),
                        "one direct admission command publishes one result"
                    );
                });
            })
            .unwrap();
        let diagnostics = Arc::try_unwrap(materialization)
            .ok()
            .and_then(OnceLock::into_inner)
            .expect("the direct admission command returned a result")
            .unwrap();

        assert!(!diagnostics.has_errors());
        assert!(
            candidate
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(
            !candidate
                .world()
                .contains_resource::<StartupSceneActivationOwner>()
        );
        let runtime = candidate.complete_startup().unwrap().promote();
        assert!(
            !runtime
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(
            runtime
                .world()
                .contains_resource::<StartupSceneActivationOwner>()
        );
        assert_eq!(
            runtime
                .world()
                .resource::<StartupSceneActivationAuthority>()
                .phase,
            StartupSceneActivationPhase::Finalized
        );
        let mut retirement = runtime.begin_retirement();
        assert_eq!(
            retirement.drive_retirement(),
            nara_app::RuntimeCandidateRetirementState::Retired
        );
    }

    #[test]
    fn direct_materialization_is_rejected_after_startup_finalizes() {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_scene::SceneComponentsPlugin,
            StartupSceneActivationPlugin,
        ))
        .unwrap();
        app.run_once(Duration::ZERO).unwrap();

        let entity_count = app.world().entities().len();
        let source = StartupSceneSource::direct(
            Arc::new(SceneDocument::new([])),
            ByteLimit::new(1024).unwrap(),
        )
        .unwrap();
        let error = materialize_startup_scene(app.world_mut().unwrap(), source).unwrap_err();

        assert_eq!(
            error.diagnostics().iter().next().unwrap().code().as_str(),
            "startup.scene.activation-window-closed"
        );
        assert_eq!(app.world().entities().len(), entity_count);
        assert!(
            !app.world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(
            !app.world()
                .contains_resource::<StartupSceneActivationOwner>()
        );
        assert_eq!(
            app.world()
                .resource::<StartupSceneActivationAuthority>()
                .phase,
            StartupSceneActivationPhase::Finalized
        );
    }
}
