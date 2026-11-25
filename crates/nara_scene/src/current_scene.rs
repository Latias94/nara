//! Startup, inspection, and replacement of the primary Scene in a World.

use std::{
    fmt,
    sync::{Arc, Weak},
};

use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
#[cfg(feature = "runtime-catalog")]
use nara_asset::AssetProductRef;
use nara_asset::AssetReferenceResolver;
use nara_core::diagnostic::{DiagnosticCode, ReportDiagnostic as Diagnostic, SafeSummary};
#[cfg(feature = "runtime-catalog")]
use nara_core::vfs::capabilities::ContentDigest;
use nara_ecs::{Resource, World};
#[cfg(feature = "runtime-catalog")]
use nara_reflect::SchemaCompositionFingerprint;

#[cfg(feature = "serde")]
use crate::RuntimeSceneCandidate;
use crate::advanced::{
    SceneAssetReservations, SceneProductOverlayWriter, SceneProductRetirement,
    SceneProductTransactionLimits, StagedSceneReplacement,
};
use crate::{
    CompiledSceneCandidate, CompiledSceneDocument, SceneComponentBindings, SceneSpawnReport,
    SourceDocumentKey, SpawnedSceneInstance, candidate::SceneMaterializationInput,
};
#[cfg(feature = "runtime-catalog")]
use crate::{SceneComponentBindingsWitness, runtime_catalog::RuntimeContentCatalog};

mod startup;

pub use startup::{
    StartupSceneActivationSet, StartupSceneMaterializeError, add_startup_scene_activation_systems,
    install_startup_scene_activation, materialize_startup_scene,
    materialized_startup_scene_receipt,
};

pub const STARTUP_SCENE_ACTIVATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.startup-scene-activation");
pub const STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STARTUP_SCENE_ACTIVATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[
            crate::SCENE_COMPONENTS_PLUGIN_ID,
            crate::SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
        ]);

/// Installs startup materialization for Apps that supply their own Scenes.
#[derive(Debug, Default, Clone, Copy)]
pub struct StartupSceneActivationPlugin;

impl Plugin for StartupSceneActivationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STARTUP_SCENE_ACTIVATION_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        install_startup_scene_activation(app)
    }
}

/// A retained asset reference scope for a Scene compiled from source files.
#[cfg(feature = "runtime-catalog")]
pub trait SceneSourceAssets: Send + Sync + 'static {
    fn resolver(&self) -> AssetReferenceResolver<'_>;
}

/// A cooked Scene and its original retained pack catalog.
#[cfg(feature = "runtime-catalog")]
pub trait PackedSceneSource: Send + Sync + 'static {
    fn candidate(&self) -> &RuntimeSceneCandidate;
    fn catalog(&self) -> &RuntimeContentCatalog;
    fn catalog_digest(&self) -> ContentDigest;
    fn product(&self) -> AssetProductRef;
}

/// Digests copied from an admitted source Scene closure.
#[cfg(feature = "runtime-catalog")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneSourceStamp {
    schema_fingerprint: SchemaCompositionFingerprint,
    manifest_digest: ContentDigest,
    content_digest: ContentDigest,
    closure_plan_digest: ContentDigest,
}

#[cfg(feature = "runtime-catalog")]
impl SceneSourceStamp {
    #[must_use]
    pub const fn new(
        schema_fingerprint: SchemaCompositionFingerprint,
        manifest_digest: ContentDigest,
        content_digest: ContentDigest,
        closure_plan_digest: ContentDigest,
    ) -> Self {
        Self {
            schema_fingerprint,
            manifest_digest,
            content_digest,
            closure_plan_digest,
        }
    }

    #[must_use]
    pub const fn schema_fingerprint(self) -> SchemaCompositionFingerprint {
        self.schema_fingerprint
    }

    #[must_use]
    pub const fn manifest_digest(self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub const fn content_digest(self) -> ContentDigest {
        self.content_digest
    }

    #[must_use]
    pub const fn closure_plan_digest(self) -> ContentDigest {
        self.closure_plan_digest
    }
}

enum RetainedSceneCandidate {
    Compiled(Arc<CompiledSceneCandidate>),
    #[cfg(feature = "serde")]
    Runtime(Arc<RuntimeSceneCandidate>),
    #[cfg(feature = "runtime-catalog")]
    Cooked {
        owner: Box<dyn PackedSceneSource>,
        hosted: bool,
    },
}

enum SceneRetention {
    Direct,
    #[cfg(feature = "runtime-catalog")]
    Authored {
        assets: Box<dyn SceneSourceAssets>,
        bindings: Arc<SceneComponentBindingsWitness>,
        stamp: SceneSourceStamp,
    },
}

/// One retained Scene candidate, with any input scope needed to interpret it.
pub struct SceneSource {
    candidate: RetainedSceneCandidate,
    retention: SceneRetention,
}

impl SceneSource {
    #[must_use]
    pub fn direct(candidate: CompiledSceneCandidate) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Compiled(Arc::new(candidate)),
            retention: SceneRetention::Direct,
        }
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn runtime(candidate: RuntimeSceneCandidate) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Runtime(Arc::new(candidate)),
            retention: SceneRetention::Direct,
        }
    }

    /// Retains the source asset scope and the component definitions used to compile this Scene.
    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn authored(
        candidate: Arc<CompiledSceneCandidate>,
        assets: impl SceneSourceAssets,
        bindings: Arc<SceneComponentBindingsWitness>,
        stamp: SceneSourceStamp,
    ) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Compiled(candidate),
            retention: SceneRetention::Authored {
                assets: Box::new(assets),
                bindings,
                stamp,
            },
        }
    }

    /// Retains the admitted Scene and its pack catalog as one source.
    ///
    /// `hosted` identifies Scenes whose asset residency is managed by the product integration.
    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn cooked(owner: impl PackedSceneSource, hosted: bool) -> Self {
        Self {
            candidate: RetainedSceneCandidate::Cooked {
                owner: Box::new(owner),
                hosted,
            },
            retention: SceneRetention::Direct,
        }
    }

    fn materialization_input(&self) -> SceneMaterializationInput<'_> {
        match &self.candidate {
            RetainedSceneCandidate::Compiled(candidate) => {
                SceneMaterializationInput::Compiled(candidate)
            }
            #[cfg(feature = "serde")]
            RetainedSceneCandidate::Runtime(candidate) => {
                SceneMaterializationInput::Runtime(candidate)
            }
            #[cfg(feature = "runtime-catalog")]
            RetainedSceneCandidate::Cooked { owner, .. } => {
                SceneMaterializationInput::Runtime(owner.candidate())
            }
        }
    }

    #[must_use]
    pub fn document(&self) -> &CompiledSceneDocument {
        self.materialization_input().document()
    }

    #[must_use]
    pub fn source_key(&self) -> &SourceDocumentKey {
        self.materialization_input().source_key()
    }

    #[must_use]
    pub fn compiled_scene(&self) -> Option<&CompiledSceneCandidate> {
        match self.materialization_input() {
            SceneMaterializationInput::Compiled(candidate) => Some(candidate),
            #[cfg(feature = "serde")]
            SceneMaterializationInput::Runtime(_) => None,
        }
    }

    #[cfg(feature = "serde")]
    #[must_use]
    pub fn runtime_scene(&self) -> Option<&RuntimeSceneCandidate> {
        match self.materialization_input() {
            SceneMaterializationInput::Compiled(_) => None,
            SceneMaterializationInput::Runtime(candidate) => Some(candidate),
        }
    }

    #[must_use]
    pub fn asset_resolution(&self) -> Option<AssetReferenceResolver<'_>> {
        match &self.retention {
            #[cfg(feature = "runtime-catalog")]
            SceneRetention::Authored { assets, .. } => Some(assets.resolver()),
            _ => None,
        }
    }

    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn source_stamp(&self) -> Option<SceneSourceStamp> {
        match self.retention {
            SceneRetention::Authored { stamp, .. } => Some(stamp),
            _ => None,
        }
    }

    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn hosted_runtime_scene_identity(&self) -> Option<(AssetProductRef, ContentDigest)> {
        let owner = self.hosted_pack_source()?;
        Some((owner.product(), owner.catalog_digest()))
    }

    /// Returns whether this source comes from the same hosted pack catalog instance.
    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn uses_hosted_catalog(&self, catalog: &RuntimeContentCatalog) -> bool {
        self.hosted_pack_source()
            .is_some_and(|owner| std::ptr::eq(owner.catalog(), catalog))
    }

    #[cfg(feature = "runtime-catalog")]
    fn hosted_pack_source(&self) -> Option<&dyn PackedSceneSource> {
        match &self.candidate {
            RetainedSceneCandidate::Cooked {
                owner,
                hosted: true,
            } => Some(owner.as_ref()),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_content_managed(&self) -> bool {
        #[cfg(feature = "runtime-catalog")]
        {
            self.source_stamp().is_some() || self.hosted_pack_source().is_some()
        }
        #[cfg(not(feature = "runtime-catalog"))]
        {
            false
        }
    }

    pub fn matches_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        #[cfg(feature = "serde")]
        if let Some(candidate) = self.runtime_scene() {
            return candidate.definitions().matches(bindings);
        }
        #[cfg(feature = "runtime-catalog")]
        if let SceneRetention::Authored {
            bindings: witness, ..
        } = &self.retention
        {
            return witness.matches(bindings);
        }
        let _ = bindings;
        true
    }
}

impl fmt::Debug for SceneSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneSource")
            .field("source", self.source_key())
            .field("entities", &self.document().entities().len())
            .finish_non_exhaustive()
    }
}

/// Weak inspection of the exact source retained by a primary Scene.
#[derive(Clone, Debug)]
pub struct SceneSourceView(Weak<SceneSource>);

impl SceneSourceView {
    pub fn with_document<R>(&self, inspect: impl FnOnce(&CompiledSceneDocument) -> R) -> Option<R> {
        self.0.upgrade().map(|source| inspect(source.document()))
    }

    pub fn with_compiled_scene<R>(
        &self,
        inspect: impl FnOnce(&CompiledSceneCandidate, Option<AssetReferenceResolver<'_>>) -> R,
    ) -> Option<R> {
        let source = self.0.upgrade()?;
        Some(inspect(source.compiled_scene()?, source.asset_resolution()))
    }

    #[cfg(feature = "serde")]
    pub fn with_runtime_scene<R>(
        &self,
        inspect: impl FnOnce(&RuntimeSceneCandidate) -> R,
    ) -> Option<R> {
        let source = self.0.upgrade()?;
        source.runtime_scene().map(inspect)
    }

    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn hosted_runtime_scene_identity(&self) -> Option<(AssetProductRef, ContentDigest)> {
        self.0.upgrade()?.hosted_runtime_scene_identity()
    }
}

/// The primary Scene's retained source and successful instance receipt.
///
/// Read with `Res<CurrentScene>` or borrow through [`Self::get`] in an exclusive system.
///
/// ```compile_fail,E0502
/// use nara_scene::current_scene::CurrentScene;
/// use nara_ecs::World;
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
    #[must_use]
    pub fn get(world: &World) -> Option<&Self> {
        world.get_resource()
    }

    #[must_use]
    pub fn source(&self) -> &CompiledSceneDocument {
        self.source.document()
    }

    #[must_use]
    pub fn retained_source(&self) -> &SceneSource {
        &self.source
    }

    #[must_use]
    pub const fn receipt(&self) -> &SpawnedSceneInstance {
        &self.receipt
    }

    #[must_use]
    pub fn source_view(&self) -> SceneSourceView {
        SceneSourceView(Arc::downgrade(&self.source))
    }

    #[cfg(feature = "runtime-catalog")]
    #[must_use]
    pub fn source_content_stamp(&self) -> Option<SceneSourceStamp> {
        self.source.source_stamp()
    }
}

impl fmt::Debug for CurrentScene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CurrentScene")
            .field("source", &self.source)
            .field("instance_id", &self.receipt.instance_id())
            .finish_non_exhaustive()
    }
}

pub use startup::{current_scene_bindings, current_scene_inspection_bindings};

/// An unpublished Scene replacement paired with the source used to stage it.
#[must_use]
pub struct StagedCurrentSceneReplacement {
    scene: StagedSceneReplacement,
    source: Arc<SceneSource>,
    expected: SpawnedSceneInstance,
}

impl StagedCurrentSceneReplacement {
    /// Publishes the staged instance and its source together on success.
    pub fn commit(self, world: &mut World) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        let Self {
            scene,
            source,
            expected,
        } = self;
        world.flush();
        if let Err(report) = current_scene_bindings(world, &expected) {
            return Err((
                Self {
                    scene,
                    source,
                    expected,
                },
                report,
            ));
        }
        if nara_ecs::transaction::validate_resource_scope::<CurrentScene>(world).is_err() {
            return Err((
                Self {
                    scene,
                    source,
                    expected,
                },
                owner_lifecycle_error(),
            ));
        }
        let report = match scene.commit(world) {
            Ok(report) => report,
            Err((scene, report)) => {
                return Err((
                    Self {
                        scene,
                        source,
                        expected,
                    },
                    report,
                ));
            }
        };
        let receipt = report
            .instance
            .as_ref()
            .expect("Scene commit returns its receipt")
            .clone();
        *world.resource_mut::<CurrentScene>() = CurrentScene { source, receipt };
        Ok(report)
    }

    /// Drops the staged data and despawns unpublished entities in their originating World.
    pub fn discard(self, world: &mut World) -> Result<(), Self> {
        let Self {
            scene,
            source,
            expected,
        } = self;
        scene.discard(world).map_err(|scene| Self {
            scene,
            source,
            expected,
        })
    }
}

/// Stages replacement with the exact primary Scene receipt and receiving asset reservations.
/// A project integration also prepares its resource residency before staging.
pub fn stage_current_scene_replacement(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    source: Option<SceneSource>,
    assets: SceneAssetReservations,
    limits: SceneProductTransactionLimits,
    additional_retirements: &[SceneProductRetirement<'_>],
    configure: impl FnOnce(&mut SceneProductOverlayWriter<'_>),
) -> Result<StagedCurrentSceneReplacement, SceneSpawnReport> {
    let bindings = current_scene_bindings(world, expected)?;
    if nara_ecs::transaction::validate_resource_scope::<CurrentScene>(world).is_err() {
        return Err(owner_lifecycle_error());
    }
    let current = world.resource::<CurrentScene>();
    let source = source.map_or_else(|| Arc::clone(&current.source), Arc::new);
    if !source.matches_component_bindings(&bindings) {
        return Err(rejected(
            "scene.current.component-bindings-mismatch",
            "Current Scene candidate does not match the installed component definitions",
        ));
    }
    let scene = match source.materialization_input() {
        SceneMaterializationInput::Compiled(candidate) => bindings.stage_scene_replacement(
            world,
            candidate,
            source.asset_resolution(),
            expected,
            assets,
            limits,
            additional_retirements,
            configure,
        ),
        #[cfg(feature = "serde")]
        SceneMaterializationInput::Runtime(candidate) => bindings.stage_runtime_scene_replacement(
            world,
            candidate,
            expected,
            assets,
            limits,
            additional_retirements,
            configure,
        ),
    }?;
    Ok(StagedCurrentSceneReplacement {
        scene,
        source,
        expected: expected.clone(),
    })
}

fn owner_lifecycle_error() -> SceneSpawnReport {
    rejected(
        "scene.current.owner-lifecycle-unsupported",
        "Current Scene ownership cannot run lifecycle effects during replacement",
    )
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("Scene diagnostic code"),
        SafeSummary::new(summary).expect("Scene diagnostic summary"),
    ));
    report
}
