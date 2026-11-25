//! Retained current-Scene ownership and one-shot startup activation.
//!
//! Project Hosts use the same materialization operation as direct managed-App integrations. The
//! operation owns the retained source, creates the scene receipt itself, and publishes the pair
//! only after scene spawning succeeds. Later data-only replacements keep that same owner current,
//! so callers cannot combine unrelated documents and receipts.

mod replacement;
#[cfg(feature = "serde")]
pub(crate) use replacement::current_scene_bindings;
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
use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticReport, ReportDiagnostic as Diagnostic, SafeSummary,
};
use nara_ecs::identity::SpawnedSceneInstance;
use nara_ecs::{
    Res, ResMut, Resource, SystemSet, World,
    bevy_ecs::{error::BevyError, system::ScheduleSystem},
    schedule::IntoScheduleConfigs,
};
#[cfg(feature = "serde")]
use nara_scene::RuntimeSceneCandidate;
use nara_scene::{
    CompiledSceneCandidate, CompiledSceneDocument, SceneComponentBindings, SceneOccurrenceKey,
    scene_component_bindings, validate_scene_component_bindings_authority,
};

#[cfg(feature = "project-source")]
use crate::project_content::{
    ProjectContentGenerationLease, ProjectContentLease, ProjectSceneContentStamp,
    ProjectSourceContent,
};
/// Stable plugin identity for the provisional advanced startup-scene activation seam.
pub const STARTUP_SCENE_ACTIVATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.startup-scene-activation");
const STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STARTUP_SCENE_ACTIVATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[
            nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
            nara_scene::SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
        ])
        .shutdown_obligations(&[
            #[cfg(feature = "serde")]
            crate::project_host::PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION,
        ]);

enum SceneRetention {
    #[cfg(feature = "project-source")]
    Project {
        _lease: ProjectContentLease,
        _generation: ProjectContentGenerationLease,
        sources: Arc<ProjectSourceContent>,
        content_stamp: Option<ProjectSceneContentStamp>,
        schema_authority: Arc<nara_scene::SceneComponentBindingsWitness>,
    },
    #[cfg(feature = "serde")]
    RuntimeContent {
        content: crate::runtime_content::RuntimeContentScene,
        custody: RuntimeContentCustody,
    },
    Direct,
}

#[cfg(feature = "serde")]
enum RuntimeContentCustody {
    Independent,
    HostManaged,
}

enum RetainedSceneCandidate {
    Compiled(Arc<CompiledSceneCandidate>),
    #[cfg(feature = "serde")]
    Runtime(Arc<RuntimeSceneCandidate>),
}

/// Move-only retained source accepted by startup materialization or current-Scene replacement.
///
/// File-backed callers receive this value from Project Content. Direct managed-App callers use
/// [`SceneSource::direct`] for compiled sources or `SceneSource::runtime` for cooked Scenes.
/// Each materialization consumes the source owner. Cooked candidates require the `serde` feature.
pub struct SceneSource {
    candidate: RetainedSceneCandidate,
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
}

impl SceneSource {
    /// Retains an already compiled source Scene for a direct managed-App integration.
    #[must_use]
    pub fn direct(candidate: CompiledSceneCandidate) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Compiled(Arc::new(candidate)),
            retention: SceneRetention::Direct,
        }
    }

    /// Binds an admitted processed Scene for a direct managed-App integration.
    ///
    /// The candidate carries the component definitions that admitted its data.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn runtime(candidate: RuntimeSceneCandidate) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Runtime(Arc::new(candidate)),
            retention: SceneRetention::Direct,
        }
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn from_project_content(
        candidate: Arc<CompiledSceneCandidate>,
        sources: Arc<ProjectSourceContent>,
        content_stamp: Option<ProjectSceneContentStamp>,
        lease: ProjectContentLease,
        generation: ProjectContentGenerationLease,
        schema_authority: Arc<nara_scene::SceneComponentBindingsWitness>,
    ) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Compiled(candidate),
            retention: SceneRetention::Project {
                _lease: lease,
                _generation: generation,
                sources,
                content_stamp,
                schema_authority,
            },
        }
    }

    #[cfg(feature = "serde")]
    pub(crate) fn from_runtime_content(
        content: crate::runtime_content::RuntimeContentScene,
    ) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Runtime(content.candidate_arc()),
            retention: SceneRetention::RuntimeContent {
                content,
                custody: RuntimeContentCustody::Independent,
            },
        }
    }

    /// Marks first-party processed content whose resource collections require Host residency.
    #[cfg(feature = "serde")]
    pub(crate) fn from_hosted_runtime_content(
        content: crate::runtime_content::RuntimeContentScene,
    ) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Runtime(content.candidate_arc()),
            retention: SceneRetention::RuntimeContent {
                content,
                custody: RuntimeContentCustody::HostManaged,
            },
        }
    }

    #[must_use]
    pub(crate) fn document(&self) -> &CompiledSceneDocument {
        match &self.candidate {
            RetainedSceneCandidate::Compiled(candidate) => candidate.document(),
            #[cfg(feature = "serde")]
            RetainedSceneCandidate::Runtime(candidate) => candidate.document(),
        }
    }

    fn compiled_scene(&self) -> Option<&CompiledSceneCandidate> {
        match &self.candidate {
            RetainedSceneCandidate::Compiled(candidate) => Some(candidate),
            #[cfg(feature = "serde")]
            RetainedSceneCandidate::Runtime(_) => None,
        }
    }

    pub(crate) fn asset_resolution(&self) -> Option<nara_asset::AssetReferenceResolver<'_>> {
        match &self.retention {
            #[cfg(feature = "project-source")]
            SceneRetention::Project { sources, .. } => Some(sources.resolution()),
            #[cfg(feature = "serde")]
            SceneRetention::RuntimeContent { .. } => None,
            SceneRetention::Direct => None,
        }
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn is_project_backed(&self) -> bool {
        matches!(self.retention, SceneRetention::Project { .. })
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn source_content_stamp(&self) -> Option<ProjectSceneContentStamp> {
        match &self.retention {
            SceneRetention::Project { content_stamp, .. } => *content_stamp,
            SceneRetention::RuntimeContent { .. } | SceneRetention::Direct => None,
        }
    }

    pub(crate) fn is_content_managed(&self) -> bool {
        match self.retention {
            #[cfg(feature = "project-source")]
            SceneRetention::Project { .. } => true,
            #[cfg(feature = "serde")]
            SceneRetention::RuntimeContent { ref custody, .. } => {
                matches!(custody, RuntimeContentCustody::HostManaged)
            }
            SceneRetention::Direct => false,
        }
    }

    fn matches_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        #[cfg(feature = "serde")]
        if let RetainedSceneCandidate::Runtime(candidate) = &self.candidate {
            return candidate.definitions().matches(bindings);
        }
        match &self.retention {
            #[cfg(feature = "project-source")]
            SceneRetention::Project {
                schema_authority, ..
            } => schema_authority.matches(bindings),
            #[cfg(feature = "serde")]
            SceneRetention::RuntimeContent { .. } => unreachable!(
                "Runtime content always retains a processed Scene with its own definitions"
            ),
            SceneRetention::Direct => {
                let _ = bindings;
                true
            }
        }
    }
}

impl fmt::Debug for SceneSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneSource")
            .field("entity_count", &self.document().entities().len())
            .field(
                "retention",
                &match self.retention {
                    #[cfg(feature = "project-source")]
                    SceneRetention::Project { .. } => "project-content",
                    #[cfg(feature = "serde")]
                    SceneRetention::RuntimeContent { .. } => "runtime-content",
                    SceneRetention::Direct => "direct",
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
    pub fn with_document<R>(&self, inspect: impl FnOnce(&CompiledSceneDocument) -> R) -> Option<R> {
        self.source
            .upgrade()
            .map(|source| inspect(source.document()))
    }

    /// Borrows the exact compiled candidate and scoped source resolution together.
    ///
    /// Both values come from the same retained source owner. Direct compiled sources have no
    /// project database. Returns `None` for a processed Runtime Scene or after the root activation
    /// owner retires, even if another caller still retains the underlying document.
    pub fn with_compiled_scene<R>(
        &self,
        inspect: impl FnOnce(
            &CompiledSceneCandidate,
            Option<nara_asset::AssetReferenceResolver<'_>>,
        ) -> R,
    ) -> Option<R> {
        let source = self.source.upgrade()?;
        Some(inspect(source.compiled_scene()?, source.asset_resolution()))
    }

    /// Borrows the exact admitted processed candidate while the current owner retains it.
    ///
    /// Returns `None` for a source compilation or after owner retirement. The bounded borrow does
    /// not retain the candidate, its definitions or the Runtime content base beyond this call.
    #[cfg(feature = "serde")]
    pub fn with_runtime_scene<R>(
        &self,
        inspect: impl FnOnce(&RuntimeSceneCandidate) -> R,
    ) -> Option<R> {
        let source = self.source.upgrade()?;
        match &source.candidate {
            RetainedSceneCandidate::Compiled(_) => None,
            RetainedSceneCandidate::Runtime(candidate) => Some(inspect(candidate)),
        }
    }

    /// Returns the retained Host-managed Runtime Scene's Product and original catalog digest.
    ///
    /// Returns `None` for other Scene sources or after the owner retires.
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn hosted_runtime_scene_identity(
        &self,
    ) -> Option<(nara_asset::AssetProductRef, crate::fs::ContentDigest)> {
        let source = self.source.upgrade()?;
        match &source.retention {
            SceneRetention::RuntimeContent {
                content,
                custody: RuntimeContentCustody::HostManaged,
            } => Some((content.product(), content.base().catalog_digest())),
            _ => None,
        }
    }
}

impl fmt::Debug for SceneSourceView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneSourceView")
            .field("available", &(self.source.strong_count() > 0))
            .finish_non_exhaustive()
    }
}

/// The active Scene's retained source and successful instance receipt.
///
/// Read this resource with `Res<CurrentScene>` in a system or borrow it through
/// [`CurrentScene::get`] in an exclusive system. The Scene activation and travel operations
/// update the source and receipt together.
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
#[derive(Resource)]
pub struct CurrentScene {
    source: Arc<SceneSource>,
    receipt: SpawnedSceneInstance,
}

impl CurrentScene {
    /// Returns the active Scene after startup activation or a successful replacement.
    #[must_use]
    pub fn get(world: &World) -> Option<&Self> {
        world.get_resource()
    }

    /// Returns the admitted source-content stamp, when the Scene came from project files.
    #[cfg(feature = "project-source")]
    #[must_use]
    pub fn source_content_stamp(&self) -> Option<ProjectSceneContentStamp> {
        self.source.source_content_stamp()
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn is_project_backed(&self) -> bool {
        self.source.is_project_backed()
    }

    #[cfg(feature = "serde")]
    pub(crate) fn is_content_managed(&self) -> bool {
        self.source.is_content_managed()
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn project_sources(&self) -> Option<&Arc<ProjectSourceContent>> {
        match &self.source.retention {
            SceneRetention::Project { sources, .. } => Some(sources),
            _ => None,
        }
    }

    #[cfg(feature = "project-source")]
    pub(crate) fn deferred_source_scene(
        &self,
        product: nara_asset::AssetProductRef,
    ) -> Option<&nara_asset::AssetSourcePath> {
        self.source.source_content_stamp()?;
        self.project_sources()?.deferred_scene(product)
    }

    #[cfg(feature = "serde")]
    pub(crate) fn matches_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        self.source.matches_component_bindings(bindings)
    }

    #[cfg(feature = "serde")]
    pub(crate) fn source_key(&self) -> &nara_scene::SourceDocumentKey {
        match &self.source.candidate {
            RetainedSceneCandidate::Compiled(candidate) => candidate.source_key(),
            RetainedSceneCandidate::Runtime(candidate) => candidate.source_key(),
        }
    }

    /// Identifies only first-party hosted processed content, retaining the original base identity.
    #[cfg(feature = "serde")]
    pub(crate) fn hosted_runtime_base(
        &self,
    ) -> Option<&crate::runtime_content::RuntimeContentBase> {
        self.hosted_runtime_scene().map(|scene| scene.base())
    }

    #[cfg(feature = "serde")]
    pub(crate) fn hosted_runtime_scene(
        &self,
    ) -> Option<&crate::runtime_content::RuntimeContentScene> {
        match &self.source.retention {
            SceneRetention::RuntimeContent {
                content,
                custody: RuntimeContentCustody::HostManaged,
            } => Some(content),
            _ => None,
        }
    }

    #[cfg(feature = "serde")]
    pub(crate) fn commit_source(&mut self, source: SceneSource, receipt: SpawnedSceneInstance) {
        self.source = Arc::new(source);
        self.receipt = receipt;
    }

    /// Returns the immutable source admitted for the active Scene.
    #[must_use]
    pub fn source(&self) -> &CompiledSceneDocument {
        self.source.document()
    }

    /// Returns the successful instance receipt paired with the active source.
    #[must_use]
    pub const fn receipt(&self) -> &SpawnedSceneInstance {
        &self.receipt
    }

    /// Observes this exact source until its Scene is replaced or retired.
    #[must_use]
    pub fn source_view(&self) -> SceneSourceView {
        SceneSourceView {
            source: Arc::downgrade(&self.source),
        }
    }
}

impl fmt::Debug for CurrentScene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CurrentScene")
            .field("source", &self.source)
            .field("instance_id", &self.receipt.instance_id())
            .field("entity_count", &self.receipt.len())
            .finish_non_exhaustive()
    }
}

#[derive(Resource)]
struct StartupSceneActivationInput(CurrentScene);

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

#[cfg(feature = "serde")]
pub(crate) fn current_scene_inspection_bindings(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<SceneComponentBindings, nara_scene::SceneSpawnReport> {
    if !world
        .get_resource::<StartupSceneActivationAuthority>()
        .is_some_and(|authority| authority.phase == StartupSceneActivationPhase::Materialized)
        || world.contains_resource::<StartupSceneActivationInput>()
    {
        return current_scene_bindings(world, expected);
    }
    let rejected = |code: &'static str, summary: &'static str| {
        let mut report = nara_scene::SceneSpawnReport::default();
        report.diagnostics.push(Diagnostic::error(
            DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
            SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
        ));
        report
    };
    let owner = world.get_resource::<CurrentScene>().ok_or_else(|| {
        rejected(
            "scene.current.owner-missing",
            "Current Scene ownership is unavailable",
        )
    })?;
    if owner.receipt != *expected {
        return Err(rejected(
            "scene.current.expected-instance-mismatch",
            "Current Scene no longer matches the expected instance",
        ));
    }
    validate_scene_component_bindings_authority(world).map_err(|_| {
        rejected(
            "scene.current.component-bindings-authority-invalid",
            "Current Scene component bindings authority is invalid",
        )
    })?;
    scene_component_bindings(world).cloned().ok_or_else(|| {
        rejected(
            "scene.current.component-bindings-unavailable",
            "Current Scene component bindings are unavailable",
        )
    })
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
        #[cfg(feature = "serde")]
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
/// neither pending input nor activation. The complete component selection is always derived from
/// and validated against the candidate World; callers cannot substitute another selection.
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
        || world.contains_resource::<CurrentScene>()
    {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.activation-state-conflict",
            "Startup scene activation state conflicts with its open authority",
        ));
    }

    validate_scene_component_bindings_authority(world).map_err(|_| {
        StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-authority-invalid",
            "Startup scene component bindings authority is invalid",
        )
    })?;
    let bindings = scene_component_bindings(world).cloned().ok_or_else(|| {
        StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-unavailable",
            "Startup scene component bindings are unavailable",
        )
    })?;
    if !source.matches_component_bindings(&bindings) {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-mismatch",
            "Startup scene source does not match the installed component definitions",
        ));
    }

    // The product owns one current-Scene slot for the complete managed runtime lifetime.
    let occurrence = SceneOccurrenceKey::new("nara.product", "current-scene")
        .expect("the product declares a valid startup occurrence");
    let prepared = match &source.candidate {
        RetainedSceneCandidate::Compiled(candidate) => {
            bindings.prepare_scene(world, candidate, &occurrence, source.asset_resolution())
        }
        #[cfg(feature = "serde")]
        RetainedSceneCandidate::Runtime(candidate) => {
            bindings.prepare_runtime_scene(world, candidate, &occurrence)
        }
    }
    .map_err(|report| StartupSceneMaterializeError::new(report.diagnostics))?;
    let mut report = prepared.commit(world);
    if report.diagnostics.has_errors() {
        return Err(StartupSceneMaterializeError::new(report.diagnostics));
    }
    let receipt = report
        .instance
        .take()
        .expect("successful scene spawning always publishes an instance receipt");
    world.insert_resource(StartupSceneActivationInput(CurrentScene {
        source: Arc::new(source),
        receipt,
    }));
    world
        .resource_mut::<StartupSceneActivationAuthority>()
        .phase = StartupSceneActivationPhase::Materialized;
    Ok(report.diagnostics)
}

#[cfg(feature = "project-source")]
pub(crate) fn materialized_startup_scene_receipt(world: &World) -> Option<&SpawnedSceneInstance> {
    world
        .get_resource::<StartupSceneActivationInput>()
        .map(|pending| &pending.0.receipt)
}

fn consume_startup_scene_activation(world: &mut World) -> Result<(), BevyError> {
    if !world.contains_resource::<StartupSceneActivationInput>() {
        return Ok(());
    }
    if world.contains_resource::<CurrentScene>() {
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
    active: Option<Res<CurrentScene>>,
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
pub(crate) fn compile_test_scene(
    document: &nara_scene::SceneDocument,
    bindings: &SceneComponentBindings,
    source: nara_scene::SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(document, &nara_scene::InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .candidate
        .expect("the fixture compiles")
        .bind_source(source)
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use nara_ecs::bevy_ecs::system::SystemState;
    use nara_reflect::ComponentRegistryBuilder;
    use nara_scene::{SceneDocument, spawn_scene};

    use super::*;

    fn scene_candidate(document: Arc<SceneDocument>) -> CompiledSceneCandidate {
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        compile_test_scene(
            &document,
            &SceneComponentBindings::new(registry, []).unwrap(),
            nara_scene::SourceDocumentKey::named(
                "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
                "startup-scene-fixture",
                nara_scene::SourceDocumentKind::Scene,
            )
            .unwrap(),
        )
    }

    #[test]
    fn borrowed_current_scene_is_absent_without_activation() {
        let world = World::new();
        assert!(CurrentScene::get(&world).is_none());
    }

    #[test]
    fn borrowed_and_scheduled_current_scene_share_owner_without_retaining_source() {
        let mut world = World::new();
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(SceneSource::direct(scene_candidate(Arc::clone(&document))));
        let RetainedSceneCandidate::Compiled(compiled) = &source.candidate else {
            panic!("the fixture retains a source compilation");
        };
        let compiled_weak = Arc::downgrade(compiled);
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let report = spawn_scene(
            &mut world,
            &registry,
            source.compiled_scene().unwrap(),
            &SceneOccurrenceKey::new("startup-fixture", "current").unwrap(),
            None,
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        world.insert_resource(CurrentScene {
            source,
            receipt: report.instance.unwrap(),
        });
        let mut access = SystemState::<Res<CurrentScene>>::new(&mut world);
        let (source_view, scheduled_view) = {
            let current = CurrentScene::get(&world).unwrap();
            let scheduled = access.get(&world).unwrap();
            assert!(std::ptr::eq(
                current.source(),
                current.source.compiled_scene().unwrap().document()
            ));
            assert!(std::ptr::eq(current.source(), scheduled.source()));
            assert!(std::ptr::eq(current.receipt(), scheduled.receipt()));
            #[cfg(feature = "project-source")]
            {
                assert_eq!(current.source_content_stamp(), None);
                assert_eq!(scheduled.source_content_stamp(), None);
            }
            let source_view = current.source_view();
            let scheduled_view = scheduled.source_view();
            assert!(Weak::ptr_eq(&source_view.source, &scheduled_view.source));
            assert_eq!(
                source_view.with_document(|source| source as *const CompiledSceneDocument),
                Some(current.source() as *const CompiledSceneDocument)
            );
            assert_eq!(Arc::strong_count(&current.source), 1);
            (source_view, scheduled_view)
        };

        drop(world);
        assert_eq!(source_view.with_document(|_| ()), None);
        assert_eq!(scheduled_view.with_document(|_| ()), None);
        assert!(compiled_weak.upgrade().is_none());
        assert_eq!(Arc::strong_count(&document), 1);
    }

    #[test]
    fn source_view_cannot_extend_the_root_document_lifetime() {
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(SceneSource::direct(scene_candidate(Arc::clone(&document))));
        let view = SceneSourceView {
            source: Arc::downgrade(&source),
        };
        let clone = view.clone();
        let RetainedSceneCandidate::Compiled(compiled) = &source.candidate else {
            panic!("the fixture retains a source compilation");
        };
        let compiled_weak = Arc::downgrade(compiled);

        assert_eq!(Arc::strong_count(&source), 1);
        assert_eq!(
            view.with_document(|source| source.entities().len()),
            Some(0)
        );
        assert_eq!(
            view.with_compiled_scene(|candidate, database| {
                assert!(candidate.canonical_provenance().is_empty());
                assert!(database.is_none());
                candidate.document().entities().len()
            }),
            Some(0)
        );

        drop(source);
        assert!(compiled_weak.upgrade().is_none());
        assert_eq!(Arc::strong_count(&document), 1);
        assert_eq!(view.with_document(|source| source.entities().len()), None);
        assert_eq!(clone.with_document(|source| source.entities().len()), None);
        assert_eq!(view.with_compiled_scene(|_, _| ()), None);
    }

    #[test]
    fn direct_materialization_uses_candidate_bindings_and_promotes_once() {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_scene::SceneComponentsPlugin,
            nara_scene::SceneComponentBindingsPlugin::new([]),
            StartupSceneActivationPlugin,
        ))
        .unwrap();
        let sealed = app.seal().unwrap();
        let mut candidate = nara_app::RuntimeCandidate::admit(
            sealed,
            nara_app::RuntimeObligationLedger::new(),
            nara_app::RuntimeClosePolicy::default(),
        )
        .unwrap();
        let source = SceneSource::direct(scene_candidate(Arc::new(
            SceneDocument::from_ordered_roots([]),
        )));
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
        assert!(!candidate.world().contains_resource::<CurrentScene>());
        let runtime = candidate.complete_startup().unwrap().promote();
        assert!(
            !runtime
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(runtime.world().contains_resource::<CurrentScene>());
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
        let source = SceneSource::direct(scene_candidate(Arc::new(
            SceneDocument::from_ordered_roots([]),
        )));
        let error = materialize_startup_scene(&mut world, source).unwrap_err();

        assert_eq!(
            error.diagnostics().iter().next().unwrap().code().as_str(),
            "startup.scene.activation-window-closed"
        );
        assert_eq!(world.entities().len(), entity_count);
        assert!(!world.contains_resource::<StartupSceneActivationInput>());
        assert!(!world.contains_resource::<CurrentScene>());
        assert_eq!(
            world.resource::<StartupSceneActivationAuthority>().phase,
            StartupSceneActivationPhase::Finalized
        );
    }
}
