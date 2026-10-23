//! Retained current-Scene ownership and one-shot startup activation.
//!
//! Project Hosts use the same materialization operation as direct managed-App integrations. The
//! operation owns the retained source, creates the scene receipt itself, and publishes the pair
//! only after scene spawning succeeds. Later data-only replacements keep that same owner current,
//! so callers cannot combine unrelated documents and receipts.

mod replacement;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) use replacement::current_scene_registry;
pub use replacement::{replace_current_scene_with_product, restart_current_scene_with_product};

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
    schedule::IntoScheduleConfigs,
    system::SystemParam,
    upstream::change_detection::Tick,
    upstream::component::ComponentId,
    upstream::error::BevyError,
    upstream::system::{
        ReadOnlySystemParam, ScheduleSystem, SystemAccess, SystemMeta, SystemParamValidationError,
    },
    upstream::world::unsafe_world_cell::UnsafeWorldCell,
};
use nara_identity::SpawnedSceneInstance;
use nara_reflect::{ComponentRegistry, component_registry, validate_component_registry_authority};
use nara_scene::{ExpandedSceneCandidate, SceneDocument, spawn_expanded_scene};

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use crate::project_content::{
    ProjectContentAssets, ProjectContentGenerationLease, ProjectContentLease,
};
use crate::scene_retention::direct_startup_scene_retained_bytes;

/// Stable plugin identity for the provisional advanced startup-scene activation seam.
pub const STARTUP_SCENE_ACTIVATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.startup-scene-activation");
const STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STARTUP_SCENE_ACTIVATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[nara_scene::SCENE_COMPONENTS_PLUGIN_ID])
        .shutdown_obligations(&[
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            crate::project_host::PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION,
        ]);

/// Validation failure while binding a direct managed-App Scene source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneSourceError {
    /// Logical retained-byte accounting overflowed `usize`.
    RetainedBytesOverflow,
    /// The exact logical retained-byte charge exceeds the caller's declared limit.
    RetainedBytesExceeded { required: usize, limit: usize },
}

impl fmt::Display for SceneSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RetainedBytesOverflow => "scene retained-byte accounting overflowed",
            Self::RetainedBytesExceeded { .. } => "scene exceeds the direct retained-byte limit",
        })
    }
}

impl Error for SceneSourceError {}

enum SceneRetention {
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    Project {
        _lease: ProjectContentLease,
        _generation: ProjectContentGenerationLease,
        assets: Arc<ProjectContentAssets>,
        retained_bytes: usize,
    },
    Direct {
        retained_bytes: usize,
        _limit: usize,
    },
}

/// Move-only retained source accepted by startup materialization or current-Scene replacement.
///
/// File-backed callers receive this value from Project Content. Direct managed-App callers use
/// [`SceneSource::direct`] and must declare a deterministic logical retained-byte limit.
/// That direct limit is neither allocator-capacity nor RSS accounting and does not claim a Project
/// Content reservation. The value deliberately does not implement `Clone`: exactly one
/// materialization consumes exactly one source owner.
pub struct SceneSource {
    candidate: Arc<ExpandedSceneCandidate>,
    retention: SceneRetention,
}

/// Non-owning view of the source retained by the current-Scene authority.
///
/// The view deliberately holds neither a strong document reference nor the Project Content lease.
/// Clones therefore cannot extend retained memory beyond candidate or runtime retirement. Product
/// code may inspect the source only through a bounded borrow while the private root owner remains
/// alive. The move-only [`SceneSource`] remains the only materialization input.
#[derive(Clone)]
pub struct SceneSourceView {
    source: Weak<SceneSource>,
    retained_bytes: usize,
}

impl SceneSource {
    /// Binds an owned source for a direct managed-App integration.
    ///
    /// This path enforces its explicit limit but does not claim or imply a
    /// `ProjectContentBudgetHost` reservation.
    pub fn direct(
        document: Arc<SceneDocument>,
        retained_byte_limit: ByteLimit,
    ) -> Result<Self, SceneSourceError> {
        Self::direct_expanded(ExpandedSceneCandidate::plain(document), retained_byte_limit)
    }

    /// Retains an already-expanded candidate without separating its document from its sources.
    pub fn direct_expanded(
        candidate: ExpandedSceneCandidate,
        retained_byte_limit: ByteLimit,
    ) -> Result<Self, SceneSourceError> {
        let retained_bytes =
            direct_startup_scene_retained_bytes(candidate.document(), candidate.provenance())
                .map_err(|_| SceneSourceError::RetainedBytesOverflow)?;
        let limit = retained_byte_limit.get();
        if retained_bytes > limit {
            return Err(SceneSourceError::RetainedBytesExceeded {
                required: retained_bytes,
                limit,
            });
        }
        Ok(Self {
            candidate: Arc::new(candidate),
            retention: SceneRetention::Direct {
                retained_bytes,
                _limit: limit,
            },
        })
    }

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn from_project_content(
        candidate: Arc<ExpandedSceneCandidate>,
        assets: Arc<ProjectContentAssets>,
        lease: ProjectContentLease,
        generation: ProjectContentGenerationLease,
        retained_bytes: usize,
    ) -> Self {
        Self {
            candidate,
            retention: SceneRetention::Project {
                _lease: lease,
                _generation: generation,
                assets,
                retained_bytes,
            },
        }
    }

    #[must_use]
    pub(crate) fn document(&self) -> &SceneDocument {
        self.candidate.document()
    }

    pub(crate) fn asset_database(&self) -> Option<&nara_asset::ProjectAssetDatabase> {
        match &self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            SceneRetention::Project { assets, .. } => Some(assets.database()),
            SceneRetention::Direct { .. } => None,
        }
    }

    pub(crate) fn is_project_backed(&self) -> bool {
        match self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            SceneRetention::Project { .. } => true,
            SceneRetention::Direct { .. } => false,
        }
    }

    #[must_use]
    pub(crate) const fn retained_bytes(&self) -> usize {
        match &self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            SceneRetention::Project { retained_bytes, .. } => *retained_bytes,
            SceneRetention::Direct { retained_bytes, .. } => *retained_bytes,
        }
    }

    #[cfg(test)]
    pub(crate) fn direct_limit(&self) -> Option<usize> {
        match self.retention {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            SceneRetention::Project { .. } => None,
            SceneRetention::Direct { _limit, .. } => Some(_limit),
        }
    }
}

impl fmt::Debug for SceneSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneSource")
            .field("entity_count", &self.document().entities.len())
            .field("retained_bytes", &self.retained_bytes())
            .field(
                "retention",
                &match self.retention {
                    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                    SceneRetention::Project { .. } => "project-content",
                    SceneRetention::Direct { .. } => "direct-limit",
                },
            )
            .finish_non_exhaustive()
    }
}

impl SceneSourceView {
    /// Borrows the exact immutable document while the root activation authority retains it.
    ///
    /// Returns `None` after that authority has retired. The temporary strong reference is kept
    /// inside this method so the view cannot accidentally extend the retained allocation.
    pub fn with_document<R>(&self, inspect: impl FnOnce(&SceneDocument) -> R) -> Option<R> {
        self.source
            .upgrade()
            .map(|source| inspect(source.document()))
    }

    /// Borrows the exact expanded candidate and admitted asset database together.
    ///
    /// Both values come from the same retained source owner. Direct sources have no project
    /// database. Returns `None` after the root activation owner retires, even if another caller
    /// still retains the underlying document.
    pub fn with_expanded_scene<R>(
        &self,
        inspect: impl FnOnce(&ExpandedSceneCandidate, Option<&nara_asset::ProjectAssetDatabase>) -> R,
    ) -> Option<R> {
        let source = self.source.upgrade()?;
        Some(inspect(&source.candidate, source.asset_database()))
    }

    /// Returns the logical retained-byte charge owned by the root activation authority.
    #[must_use]
    pub(crate) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

impl fmt::Debug for SceneSourceView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneSourceView")
            .field("available", &(self.source.strong_count() > 0))
            .field("retained_bytes", &self.retained_bytes())
            .finish_non_exhaustive()
    }
}

/// Read-only product view of the committed Scene source and its matching instance receipt.
///
/// The activation plugin promotes a private one-shot materialization input into a private owner
/// before product-dependent Startup systems run. That root resource, which downstream code
/// cannot name or remove, owns source retention. Replacement and Restart update that same pair
/// before their exclusive caller returns. A cloned receipt is an observation and an expected-state
/// guard, not independently mutable current-Scene authority.
///
/// Scheduled systems receive this parameter directly. Exclusive callers use [`CurrentScene::get`]
/// to borrow the same source and receipt from a shared [`World`] reference.
pub struct CurrentScene<'w> {
    owner: Res<'w, CurrentSceneOwner>,
}

/// Borrowed read-only access to the committed Scene source and its matching instance receipt.
///
/// Obtain this view with [`CurrentScene::get`]. Its private owner stays in the World, and the view
/// retains no strong source reference. References returned by its methods borrow that same owner;
/// finish inspecting them before mutating the World or committing a replacement.
///
/// The World cannot be mutated while a current-Scene borrow is still in use:
///
/// ```compile_fail,E0502
/// use nara::{advanced_prelude::CurrentScene, ecs::World};
///
/// fn mutate_while_borrowed(world: &mut World) {
///     let current = CurrentScene::get(world).unwrap();
///     world.spawn_empty();
///     let _ = current.receipt();
/// }
/// ```
pub struct CurrentSceneView<'w> {
    owner: &'w CurrentSceneOwner,
}

#[derive(Resource)]
pub(crate) struct CurrentSceneOwner {
    source: Arc<SceneSource>,
    receipt: SpawnedSceneInstance,
}

impl CurrentScene<'_> {
    /// Borrows the committed Scene source and its matching receipt without constructing a system.
    ///
    /// Returns `None` before startup activation or when no current Scene is installed. This only
    /// reads the private owner; it does not activate, advance, flush or commit any work.
    #[must_use]
    pub fn get(world: &World) -> Option<CurrentSceneView<'_>> {
        world
            .get_resource::<CurrentSceneOwner>()
            .map(|owner| CurrentSceneView { owner })
    }

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

    /// Returns a non-owning view of the exact committed source.
    ///
    /// Cloning or leaking this view cannot keep an old source alive after replacement or runtime
    /// retirement. Restart keeps this source and publishes a new matching instance receipt.
    #[must_use]
    pub fn source_view(&self) -> SceneSourceView {
        self.owner.source_view()
    }
}

impl CurrentSceneView<'_> {
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

    /// Returns a non-owning view of the exact committed source.
    ///
    /// This weak view may outlive the World borrow, but cannot retain an old source after
    /// replacement or runtime retirement. Restart keeps the source with a new matching receipt.
    #[must_use]
    pub fn source_view(&self) -> SceneSourceView {
        self.owner.source_view()
    }
}

impl CurrentSceneOwner {
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn is_project_backed(&self) -> bool {
        self.source.is_project_backed()
    }

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn commit_source(&mut self, source: SceneSource, receipt: SpawnedSceneInstance) {
        self.source = Arc::new(source);
        self.receipt = receipt;
    }

    pub(crate) fn source(&self) -> &SceneDocument {
        self.source.document()
    }

    pub(crate) const fn receipt(&self) -> &SpawnedSceneInstance {
        &self.receipt
    }

    fn source_view(&self) -> SceneSourceView {
        SceneSourceView {
            source: Arc::downgrade(&self.source),
            retained_bytes: self.source.retained_bytes(),
        }
    }
}

impl fmt::Debug for CurrentScene<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CurrentScene")
            .field("source", &self.owner.source)
            .field("instance_id", &self.owner.receipt.instance_id())
            .field("entity_count", &self.owner.receipt.len())
            .finish_non_exhaustive()
    }
}

// SAFETY: Every operation delegates to `Res<CurrentSceneOwner>` and therefore registers
// and performs only the same read access to the private owner resource.
unsafe impl SystemParam for CurrentScene<'_> {
    type State = ComponentId;
    type Item<'world, 'state> = CurrentScene<'world>;

    fn init_state(world: &mut World) -> Self::State {
        <Res<'_, CurrentSceneOwner> as SystemParam>::init_state(world)
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        system_access: &mut SystemAccess,
        world: &mut World,
    ) {
        <Res<'_, CurrentSceneOwner> as SystemParam>::init_access(
            state,
            system_meta,
            system_access,
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
            <Res<'_, CurrentSceneOwner> as SystemParam>::get_param(
                state,
                system_meta,
                world,
                change_tick,
            )?
        };
        Ok(CurrentScene { owner })
    }
}

// SAFETY: the delegated resource parameter is read-only.
unsafe impl ReadOnlySystemParam for CurrentScene<'_> {}

#[derive(Resource)]
struct StartupSceneActivationInput(CurrentSceneOwner);

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

/// Registers product initialization that consumes the one-shot startup-scene activation window.
///
/// The startup-scene module owns the concrete startup schedule and ordering set. Product plugins
/// use this semantic entry point instead of depending on [`StartupStage::Runtime`] or the internal
/// consume/finalize phases.
pub fn add_startup_scene_activation_systems<'a, M>(
    app: &'a mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&'a mut App, PluginError> {
    app.add_systems(
        StartupStage::Runtime,
        systems.in_set(StartupSceneActivationSet),
    )
}

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
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        {
            crate::project_host::register_project_scene_close_participant(app)?;
            app.add_systems(
                nara_app::CoreStage::TaskUpdate,
                crate::project_host::poll_project_scene_requests,
            )?;
        }
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
    source: SceneSource,
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
        || world.contains_resource::<CurrentSceneOwner>()
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

    let mut report =
        spawn_expanded_scene(world, &registry, &source.candidate, source.asset_database());
    if report.diagnostics.has_errors() {
        return Err(StartupSceneMaterializeError::new(report.diagnostics));
    }
    let receipt = report
        .instance
        .take()
        .expect("successful scene spawning always publishes an instance receipt");
    world.insert_resource(StartupSceneActivationInput(CurrentSceneOwner {
        source: Arc::new(source),
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
    if world.contains_resource::<CurrentSceneOwner>() {
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
    active: Option<Res<CurrentSceneOwner>>,
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
mod replacement_tests;

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use nara_ecs::upstream::system::SystemState;

    use super::*;

    #[test]
    fn borrowed_current_scene_is_absent_without_activation() {
        let world = World::new();
        assert!(CurrentScene::get(&world).is_none());
        assert!(!world.contains_resource::<CurrentSceneOwner>());
    }

    #[test]
    fn borrowed_and_scheduled_current_scene_share_owner_without_retaining_source() {
        let mut world = World::new();
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(
            SceneSource::direct(Arc::clone(&document), ByteLimit::new(1024).unwrap()).unwrap(),
        );
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let report = spawn_expanded_scene(&mut world, &registry, &source.candidate, None);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        world.insert_resource(CurrentSceneOwner {
            source,
            receipt: report.instance.unwrap(),
        });
        let mut access = SystemState::<CurrentScene>::new(&mut world);
        let (source_view, scheduled_view) = {
            let current = CurrentScene::get(&world).unwrap();
            let scheduled = access.get(&world).unwrap();
            assert!(std::ptr::eq(current.source(), document.as_ref()));
            assert!(std::ptr::eq(current.source(), scheduled.source()));
            assert!(std::ptr::eq(current.receipt(), scheduled.receipt()));
            let source_view = current.source_view();
            let scheduled_view = scheduled.source_view();
            assert!(Weak::ptr_eq(&source_view.source, &scheduled_view.source));
            assert_eq!(
                source_view.with_document(|source| source as *const SceneDocument),
                Some(current.source() as *const SceneDocument)
            );
            assert_eq!(Arc::strong_count(&current.owner.source), 1);
            (source_view, scheduled_view)
        };

        drop(world);
        assert_eq!(source_view.with_document(|_| ()), None);
        assert_eq!(scheduled_view.with_document(|_| ()), None);
        assert_eq!(Arc::strong_count(&document), 1);
    }

    #[test]
    fn direct_source_enforces_its_exact_retained_byte_limit() {
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let required = direct_startup_scene_retained_bytes(
            &document,
            &nara_scene::SceneEntityProvenanceMap::new(),
        )
        .unwrap();
        assert!(required > 1);

        let source =
            SceneSource::direct(Arc::clone(&document), ByteLimit::new(required).unwrap()).unwrap();

        assert!(std::ptr::eq(source.document(), document.as_ref()));
        assert_eq!(source.retained_bytes(), required);
        assert_eq!(source.direct_limit(), Some(required));
        assert_eq!(
            SceneSource::direct(
                document,
                ByteLimit::new(required.checked_sub(1).unwrap()).unwrap(),
            )
            .unwrap_err(),
            SceneSourceError::RetainedBytesExceeded {
                required,
                limit: required - 1,
            }
        );
    }

    #[test]
    fn source_view_cannot_extend_the_root_document_lifetime() {
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(
            SceneSource::direct(Arc::clone(&document), ByteLimit::new(1024).unwrap()).unwrap(),
        );
        let view = SceneSourceView {
            source: Arc::downgrade(&source),
            retained_bytes: source.retained_bytes(),
        };
        let clone = view.clone();

        assert_eq!(Arc::strong_count(&source), 1);
        assert_eq!(view.with_document(|source| source.entities.len()), Some(0));
        assert_eq!(clone.retained_bytes(), source.retained_bytes());
        assert_eq!(
            view.with_expanded_scene(|candidate, database| {
                assert!(candidate.provenance().is_empty());
                assert!(database.is_none());
                candidate.document().entities.len()
            }),
            Some(0)
        );

        drop(source);
        assert_eq!(Arc::strong_count(&document), 1);
        assert_eq!(view.with_document(|source| source.entities.len()), None);
        assert_eq!(clone.with_document(|source| source.entities.len()), None);
        assert_eq!(view.with_expanded_scene(|_, _| ()), None);
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
        let source = SceneSource::direct(
            Arc::new(SceneDocument::from_ordered_roots([])),
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
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
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
        assert!(!candidate.world().contains_resource::<CurrentSceneOwner>());
        let runtime = candidate.complete_startup().unwrap().promote();
        assert!(
            !runtime
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(runtime.world().contains_resource::<CurrentSceneOwner>());
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
        let mut world = World::new();
        world.insert_resource(StartupSceneActivationAuthority {
            phase: StartupSceneActivationPhase::Finalized,
        });
        let entity_count = world.entities().len();
        let source = SceneSource::direct(
            Arc::new(SceneDocument::from_ordered_roots([])),
            ByteLimit::new(1024).unwrap(),
        )
        .unwrap();
        let error = materialize_startup_scene(&mut world, source).unwrap_err();

        assert_eq!(
            error.diagnostics().iter().next().unwrap().code().as_str(),
            "startup.scene.activation-window-closed"
        );
        assert_eq!(world.entities().len(), entity_count);
        assert!(!world.contains_resource::<StartupSceneActivationInput>());
        assert!(!world.contains_resource::<CurrentSceneOwner>());
        assert_eq!(
            world.resource::<StartupSceneActivationAuthority>().phase,
            StartupSceneActivationPhase::Finalized
        );
    }
}
