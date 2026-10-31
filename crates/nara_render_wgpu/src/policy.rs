//! Original-definition CPU capture and producer-local renderer policy construction.

#[cfg(feature = "stock-ui")]
mod ui;
#[cfg(feature = "stock-ui")]
pub use ui::WgpuUiProgramBinding;

use std::{
    any::Any,
    mem::size_of,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nara_ecs::Resource;
use nara_render::{FrameStats, RenderFramePacket, RenderImagePacket};

use crate::{
    WgpuDeviceRequirements, WgpuFeatureDeclaration, WgpuFeatureInstance, WgpuProgramTexture,
    WgpuRenderFeature, WgpuRenderProducerId, WgpuRenderProgram, WgpuRenderProgramError,
    graph::{ResourceDescriptor, ResourceVersion},
};

static NEXT_POLICY_DEFINITION: AtomicU64 = AtomicU64::new(1);
static NEXT_POLICY_INSTALLATION: AtomicU64 = AtomicU64::new(1);
const MAX_POLICIES_PER_PRODUCER: usize = 64;
const MAX_FEATURE_BINDINGS_PER_PRODUCER: usize = 1024;

/// A renderer's captured CPU data and exact graph construction implementation.
///
/// Bindings retain actual CPU Feature owners, not native mutable state. Capture systems read
/// ordinary ECS parameters; these methods receive no World, Device, Queue or presentation owner.
pub trait WgpuRenderPolicy: Send + Sync + 'static {
    type Capture: Send + Sync + 'static;
    type Bindings: Send + Sync + 'static;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        WgpuDeviceRequirements::default()
    }

    /// Selects the original CPU temporal clock retained by this capture, if any.
    /// It is evaluated at input admission, never through a later installed policy.
    fn history_lineage(&self, _capture: &Self::Capture) -> Option<crate::WgpuHistoryLineage> {
        None
    }

    /// Resolves this capture's original declarations into one immutable candidate snapshot.
    fn bind(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError>;

    fn validate_capture(
        &self,
        _packet: &RenderFramePacket,
        _capture: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError>;
}

struct PolicyDefinition<P: WgpuRenderPolicy> {
    id: u64,
    policy: P,
    requirements: WgpuDeviceRequirements,
}

/// Immutable implementation and device requirements, retained by every original capture.
pub struct WgpuPolicyDefinition<P: WgpuRenderPolicy>(Arc<PolicyDefinition<P>>);

impl<P: WgpuRenderPolicy> Clone for WgpuPolicyDefinition<P> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<P: WgpuRenderPolicy> std::fmt::Debug for WgpuPolicyDefinition<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuPolicyDefinition")
            .field("id", &self.0.id)
            .field("type", &std::any::type_name::<P>())
            .finish_non_exhaustive()
    }
}

impl<P: WgpuRenderPolicy> WgpuPolicyDefinition<P> {
    #[must_use]
    pub fn new(policy: P) -> Self {
        let id = NEXT_POLICY_DEFINITION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .unwrap_or_else(|_| std::process::abort());
        let requirements = policy.device_requirements();
        Self(Arc::new(PolicyDefinition {
            id,
            policy,
            requirements,
        }))
    }

    #[must_use]
    pub fn policy(&self) -> &P {
        &self.0.policy
    }

    #[must_use]
    pub fn device_requirements(&self) -> &WgpuDeviceRequirements {
        &self.0.requirements
    }

    pub fn install(&self) -> Result<WgpuPolicyInstallation<P>, WgpuRenderProgramError> {
        let id = NEXT_POLICY_INSTALLATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| WgpuRenderProgramError::IdentityExhausted)?;
        Ok(WgpuPolicyInstallation {
            admission: Arc::new(PolicyCaptureAdmission {
                id: WgpuPolicyInstallationId(id),
                open: AtomicBool::new(true),
            }),
            definition: self.clone(),
        })
    }
}

/// Process-local installation fact. Execution still requires its original accepted capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WgpuPolicyInstallationId(u64);

impl WgpuPolicyInstallationId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug)]
pub(crate) struct PolicyCaptureAdmission {
    pub(crate) id: WgpuPolicyInstallationId,
    open: AtomicBool,
}

impl PolicyCaptureAdmission {
    pub(crate) fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

/// Owns admission of new captures. Drop closes admission without revoking accepted input.
///
/// Move an installation into its capture owner's ECS Resource or keep it beside an independent
/// renderer. Retained inputs carry CPU data and the original definition across Host replacement.
#[derive(Resource)]
pub struct WgpuPolicyInstallation<P: WgpuRenderPolicy> {
    admission: Arc<PolicyCaptureAdmission>,
    definition: WgpuPolicyDefinition<P>,
}

impl<P: WgpuRenderPolicy> std::fmt::Debug for WgpuPolicyInstallation<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuPolicyInstallation")
            .field("id", &self.admission.id)
            .field("definition", &self.definition)
            .field("open", &self.is_open())
            .finish()
    }
}

impl<P: WgpuRenderPolicy> Drop for WgpuPolicyInstallation<P> {
    fn drop(&mut self) {
        self.admission.open.store(false, Ordering::Release);
    }
}

impl<P: WgpuRenderPolicy> WgpuPolicyInstallation<P> {
    #[must_use]
    pub fn id(&self) -> WgpuPolicyInstallationId {
        self.admission.id
    }

    #[must_use]
    pub fn definition(&self) -> &WgpuPolicyDefinition<P> {
        &self.definition
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.admission.is_open()
    }

    pub fn close(&mut self) {
        self.admission.open.store(false, Ordering::Release);
    }

    pub fn capture(
        &self,
        packet: RenderFramePacket,
        capture: Arc<P::Capture>,
    ) -> Result<WgpuRenderInput, WgpuRenderProgramError> {
        let capture = self.accept(capture)?;
        WgpuRenderInput::from_capture(packet, capture)
    }

    pub(crate) fn admission(&self) -> &Arc<PolicyCaptureAdmission> {
        &self.admission
    }

    pub(crate) fn accept(
        &self,
        capture: Arc<P::Capture>,
    ) -> Result<Arc<dyn CapturedPolicy>, WgpuRenderProgramError> {
        if !self.is_open() {
            return Err(WgpuRenderProgramError::PolicyClosed);
        }
        let lineage = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.definition.0.policy.history_lineage(&capture)
        }))
        .map_err(|_| WgpuRenderProgramError::PolicyPanicked)?;
        Ok(Arc::new(TypedPolicyCapture::<P> {
            admission: Arc::clone(&self.admission),
            definition: Arc::clone(&self.definition.0),
            capture,
            lineage,
        }))
    }
}

/// Immutable reconstruction input; clone retains the exact packet, capture and definition.
#[derive(Clone)]
pub struct WgpuRenderInput {
    pub(crate) packet: Arc<RenderFramePacket>,
    pub(crate) capture: Arc<dyn CapturedPolicy>,
    pub(crate) lineage: Option<crate::WgpuHistoryLineage>,
}

impl std::fmt::Debug for WgpuRenderInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuRenderInput")
            .field("packet", &self.packet)
            .field("installation", &self.capture.admission().id)
            .field("definition", &self.capture.definition_id())
            .finish_non_exhaustive()
    }
}

impl WgpuRenderInput {
    pub(crate) fn from_capture(
        packet: RenderFramePacket,
        capture: Arc<dyn CapturedPolicy>,
    ) -> Result<Self, WgpuRenderProgramError> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| capture.validate(&packet)))
            .unwrap_or(Err(WgpuRenderProgramError::PolicyPanicked))?;
        let lineage = capture.lineage().cloned();
        if lineage
            .as_ref()
            .is_some_and(|lineage| lineage.source_generation() != packet.source_generation())
        {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        Ok(Self {
            packet: Arc::new(packet),
            capture,
            lineage,
        })
    }

    #[must_use]
    pub fn packet(&self) -> &RenderFramePacket {
        &self.packet
    }

    #[must_use]
    pub fn installation(&self) -> WgpuPolicyInstallationId {
        self.capture.admission().id
    }

    /// Borrows this input's original typed capture for inspection, without current lookup.
    #[must_use]
    pub fn captured<P: WgpuRenderPolicy>(&self) -> Option<&P::Capture> {
        self.capture
            .as_any()
            .downcast_ref::<TypedPolicyCapture<P>>()
            .map(|capture| capture.capture.as_ref())
    }

    /// Preserves the exact captured meaning while adapting an equal-sized window to an image.
    pub fn with_image_target(
        &self,
        target: RenderImagePacket,
    ) -> Result<Self, WgpuRenderProgramError> {
        let packet = self
            .packet
            .with_image_target(target)
            .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.capture.validate(&packet)
        }))
        .unwrap_or(Err(WgpuRenderProgramError::PolicyPanicked))?;
        Ok(Self {
            packet: Arc::new(packet),
            capture: Arc::clone(&self.capture),
            lineage: self.lineage.clone(),
        })
    }

    pub(crate) fn requirements(&self) -> &WgpuDeviceRequirements {
        self.capture.requirements()
    }
}

/// Candidate-local binding within the original policy's closed device request.
///
/// Resolution uses the supplied original declaration, independently of the latest published
/// topology. New owners remain staged until the Host publishes the complete candidate.
pub struct WgpuPolicyBindingContext<'a> {
    pub(crate) backend_instance_id: u64,
    pub(crate) producer: WgpuRenderProducerId,
    pub(crate) closed: &'a Arc<AtomicBool>,
    pub(crate) requirements: &'a WgpuDeviceRequirements,
    pub(crate) resolutions: &'a mut FeatureResolutions,
    pub(crate) staged: &'a mut StagedFeatureBindings,
    pub(crate) packet: &'a RenderFramePacket,
    pub(crate) history: &'a mut crate::history::HistoryDeclarations,
    pub(crate) temporal: bool,
}

impl WgpuPolicyBindingContext<'_> {
    /// Declares one texture slot before the complete predecessor basis is pinned.
    pub fn history_texture<S: 'static, F: WgpuRenderFeature>(
        &mut self,
        view: &crate::WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: crate::WgpuProgramTextureDescriptor,
    ) -> Result<crate::WgpuHistoryTexture<S>, WgpuRenderProgramError> {
        if !self.temporal {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        self.feature(feature)?;
        self.history
            .texture(self.packet, view, feature, slot, descriptor)
    }

    pub fn history_buffer<S: 'static, F: WgpuRenderFeature>(
        &mut self,
        view: &crate::WgpuHistoryView,
        feature: &WgpuFeatureDeclaration<F>,
        slot: u64,
        descriptor: wgpu::BufferDescriptor<'static>,
    ) -> Result<crate::WgpuHistoryBuffer<S>, WgpuRenderProgramError> {
        if !self.temporal {
            return Err(WgpuRenderProgramError::HistoryDeclaration);
        }
        self.feature(feature)?;
        self.history
            .buffer(self.packet, view, feature, slot, descriptor)
    }

    pub fn feature<F: WgpuRenderFeature>(
        &mut self,
        declaration: &WgpuFeatureDeclaration<F>,
    ) -> Result<Arc<WgpuFeatureInstance<F>>, WgpuRenderProgramError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        declaration.device_requirements().validate_enabled(
            self.requirements.required_features(),
            self.requirements.required_limits(),
        )?;
        if let Some(binding) = self
            .staged
            .values
            .iter()
            .find(|binding| binding.declaration == declaration.id())
        {
            return binding.typed();
        }
        if self.staged.values.len() >= MAX_FEATURE_BINDINGS_PER_PRODUCER {
            return Err(WgpuRenderProgramError::PolicyCapacity);
        }
        let existing = self
            .resolutions
            .0
            .iter()
            .flatten()
            .find(|entry| entry.declaration == declaration.id())
            .and_then(|entry| entry.binding.upgrade());
        if let Some(binding) = existing {
            let metadata_bytes = self.staged.admit(binding.metadata_bytes)?;
            let instance = binding.typed()?;
            self.staged.metadata_bytes = metadata_bytes;
            self.staged.values.push(binding);
            return Ok(instance);
        }
        let index = self
            .resolutions
            .0
            .iter()
            .position(|entry| entry.is_none())
            .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
        let metadata_bytes = declaration
            .binding_metadata_bytes()
            .and_then(|bytes| bytes.checked_add(size_of::<FeatureBinding>()))
            .and_then(|bytes| bytes.checked_add(2 * size_of::<usize>()))
            .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
        // The fixed stage capacity and exact new entry metadata are admitted before owner
        // construction or cloning. A refused bind cannot change a published strong reference.
        let staged_metadata_bytes = self.staged.admit(metadata_bytes)?;
        let instance = Arc::new(WgpuFeatureInstance::new(
            self.backend_instance_id,
            self.producer,
            Arc::clone(self.closed),
            declaration.definition(),
        )?);
        let binding = Arc::new(FeatureBinding {
            slot: index,
            declaration: declaration.id(),
            value: Arc::clone(&instance) as Arc<dyn Any + Send + Sync>,
            metadata_bytes,
        });
        self.resolutions.0[index] = Some(FeatureResolution {
            declaration: declaration.id(),
            lifetime: declaration.lifetime(),
            binding: Arc::downgrade(&binding),
            published: None,
        });
        self.staged.metadata_bytes = staged_metadata_bytes;
        self.staged.values.push(binding);
        Ok(instance)
    }
}

/// Actual captured target and native destination facts, without native execution authority.
#[derive(Clone, Copy)]
pub struct WgpuPolicyBuildContext<'a> {
    pub(crate) packet: &'a RenderFramePacket,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) history: Option<&'a crate::history::PreparedHistory>,
}

impl<'a> WgpuPolicyBuildContext<'a> {
    #[must_use]
    pub const fn packet(&self) -> &'a RenderFramePacket {
        self.packet
    }

    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn previous_texture<S: 'static>(
        &self,
        slot: &crate::WgpuHistoryTexture<S>,
    ) -> Result<Option<crate::WgpuProgramTextureOutput<S>>, WgpuRenderProgramError> {
        self.history
            .ok_or(WgpuRenderProgramError::HistoryDeclaration)?
            .previous_texture(slot)
    }

    pub fn previous_buffer<S: 'static>(
        &self,
        slot: &crate::WgpuHistoryBuffer<S>,
    ) -> Result<Option<crate::WgpuProgramBufferOutput<S>>, WgpuRenderProgramError> {
        self.history
            .ok_or(WgpuRenderProgramError::HistoryDeclaration)?
            .previous_buffer(slot)
    }

    pub fn empty_texture_reason<S: 'static>(
        &self,
        slot: &crate::WgpuHistoryTexture<S>,
    ) -> Result<Option<crate::WgpuEmptyHistoryReason>, WgpuRenderProgramError> {
        self.history
            .ok_or(WgpuRenderProgramError::HistoryDeclaration)?
            .empty_texture_reason(slot)
    }

    pub fn empty_buffer_reason<S: 'static>(
        &self,
        slot: &crate::WgpuHistoryBuffer<S>,
    ) -> Result<Option<crate::WgpuEmptyHistoryReason>, WgpuRenderProgramError> {
        self.history
            .ok_or(WgpuRenderProgramError::HistoryDeclaration)?
            .empty_buffer_reason(slot)
    }
}

/// Captured observations. These counters confer no output or presentation authority.
#[derive(Clone, Copy, Debug, Default)]
pub struct WgpuPolicyFrameFacts {
    pub frame: FrameStats,
    pub image_sprites: u32,
    pub instance_buffer_bytes: u64,
}

/// One complete graph group and its exact physical final image.
pub struct WgpuPolicyProgram {
    pub(crate) program: WgpuRenderProgram,
    pub(crate) final_version: ResourceVersion,
    pub(crate) facts: WgpuPolicyFrameFacts,
    pub(crate) history: Vec<crate::WgpuHistoryBinding>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui: Option<WgpuUiProgramBinding>,
}

impl WgpuPolicyProgram {
    pub fn new<S: 'static>(
        program: WgpuRenderProgram,
        final_texture: &WgpuProgramTexture<S>,
        facts: WgpuPolicyFrameFacts,
    ) -> Result<Self, WgpuRenderProgramError> {
        let resource = program.0.graph.resource(&final_texture.version)?;
        if !matches!(&resource.desc, ResourceDescriptor::Texture { .. }) {
            return Err(WgpuRenderProgramError::ResourceKind);
        }
        if !program.0.graph.outputs().contains(&final_texture.version) {
            return Err(WgpuRenderProgramError::UnpublishedResource);
        }
        Ok(Self {
            program,
            final_version: final_texture.version.clone(),
            facts,
            history: Vec::new(),
            #[cfg(feature = "stock-ui")]
            ui: None,
        })
    }

    /// Selects all declared temporal destinations for the same complete output publication.
    #[must_use]
    pub fn with_history(mut self, bindings: Vec<crate::WgpuHistoryBinding>) -> Self {
        self.history = bindings;
        self
    }

    #[cfg(feature = "stock-ui")]
    pub fn with_ui(
        mut self,
        binding: WgpuUiProgramBinding,
    ) -> Result<Self, WgpuRenderProgramError> {
        binding.validate(&self.program, &self.final_version)?;
        self.ui = Some(binding);
        Ok(self)
    }
}

pub(crate) trait CapturedPolicy: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn admission(&self) -> &Arc<PolicyCaptureAdmission>;
    fn definition_id(&self) -> u64;
    fn requirements(&self) -> &WgpuDeviceRequirements;
    fn lineage(&self) -> Option<&crate::WgpuHistoryLineage>;
    fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError>;
    fn validate(&self, packet: &RenderFramePacket) -> Result<(), WgpuRenderProgramError>;
    fn bind(
        &self,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, WgpuRenderProgramError>;
    fn build(
        &self,
        bindings: &(dyn Any + Send + Sync),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError>;
}

struct TypedPolicyCapture<P: WgpuRenderPolicy> {
    admission: Arc<PolicyCaptureAdmission>,
    definition: Arc<PolicyDefinition<P>>,
    capture: Arc<P::Capture>,
    lineage: Option<crate::WgpuHistoryLineage>,
}

impl<P: WgpuRenderPolicy> CapturedPolicy for TypedPolicyCapture<P> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn admission(&self) -> &Arc<PolicyCaptureAdmission> {
        &self.admission
    }

    fn definition_id(&self) -> u64 {
        self.definition.id
    }

    fn requirements(&self) -> &WgpuDeviceRequirements {
        &self.definition.requirements
    }

    fn lineage(&self) -> Option<&crate::WgpuHistoryLineage> {
        self.lineage.as_ref()
    }

    fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError> {
        // Exact inline values and their original Arc counters. Indirect policy-owned data
        // remains the capture/factory's responsibility, just as for native Feature captures.
        [
            size_of::<Self>(),
            size_of::<P::Capture>(),
            size_of::<P::Bindings>(),
            size_of::<PolicyDefinition<P>>(),
            size_of::<PolicyCaptureAdmission>(),
            10 * size_of::<usize>(),
        ]
        .into_iter()
        .try_fold(0usize, |bytes, value| {
            bytes
                .checked_add(value)
                .ok_or(WgpuRenderProgramError::PolicyCapacity)
        })
    }

    fn validate(&self, packet: &RenderFramePacket) -> Result<(), WgpuRenderProgramError> {
        self.definition
            .policy
            .validate_capture(packet, &self.capture)
    }

    fn bind(
        &self,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, WgpuRenderProgramError> {
        Ok(Arc::new(
            self.definition.policy.bind(&self.capture, context)?,
        ))
    }

    fn build(
        &self,
        bindings: &(dyn Any + Send + Sync),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let bindings = bindings
            .downcast_ref::<P::Bindings>()
            .ok_or(WgpuRenderProgramError::PolicyState)?;
        self.definition
            .policy
            .build(bindings, &self.capture, context)
    }
}

struct FeatureBinding {
    slot: usize,
    declaration: u64,
    value: Arc<dyn Any + Send + Sync>,
    metadata_bytes: usize,
}

impl FeatureBinding {
    fn typed<F: WgpuRenderFeature>(
        &self,
    ) -> Result<Arc<WgpuFeatureInstance<F>>, WgpuRenderProgramError> {
        let instance = Arc::clone(&self.value)
            .downcast::<WgpuFeatureInstance<F>>()
            .map_err(|_| WgpuRenderProgramError::PolicyState)?;
        if instance.identity.is_closed() {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        Ok(instance)
    }
}

struct FeatureResolution {
    declaration: u64,
    lifetime: Weak<()>,
    // Weak reservations let concurrent unpublished candidates resolve the same actual owner.
    // They do not publish membership or retain an abandoned candidate's new instance.
    binding: Weak<FeatureBinding>,
    published: Option<Arc<FeatureBinding>>,
}

pub(crate) struct FeatureResolutions(Vec<Option<FeatureResolution>>);

impl Default for FeatureResolutions {
    fn default() -> Self {
        Self(Vec::new())
    }
}

pub(crate) struct StagedFeatureBindings {
    values: Vec<Arc<FeatureBinding>>,
    metadata_bytes: usize,
}

impl StagedFeatureBindings {
    fn new() -> Result<Self, WgpuRenderProgramError> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(MAX_FEATURE_BINDINGS_PER_PRODUCER)
            .map_err(|_| WgpuRenderProgramError::PolicyCapacity)?;
        let metadata_bytes = values
            .capacity()
            .checked_mul(size_of::<Arc<FeatureBinding>>())
            .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
        Ok(Self {
            values,
            metadata_bytes,
        })
    }

    fn admit(&self, bytes: usize) -> Result<usize, WgpuRenderProgramError> {
        self.metadata_bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= 512 * 1024)
            .ok_or(WgpuRenderProgramError::PolicyCapacity)
    }
}

struct PolicyBindings {
    _value: Arc<dyn Any + Send + Sync>,
    features: StagedFeatureBindings,
}

struct PolicyInstanceEntry {
    admission: Weak<PolicyCaptureAdmission>,
    installation: WgpuPolicyInstallationId,
    definition: u64,
    // Only the latest published snapshot is cached. Older snapshots live with their exact
    // candidates and submitted custody, rather than accumulating in a per-frame map.
    value: Option<Arc<PolicyBindings>>,
}

/// Bounded producer-local resolution, separate from the latest published policy memberships.
pub(crate) struct PolicyInstances {
    policies: Vec<Option<PolicyInstanceEntry>>,
    features: FeatureResolutions,
}

impl Default for PolicyInstances {
    fn default() -> Self {
        Self {
            policies: Vec::new(),
            features: FeatureResolutions::default(),
        }
    }
}

pub(crate) struct PolicyExecutionCustody {
    _capture: Arc<dyn CapturedPolicy>,
    _bindings: Arc<PolicyBindings>,
}

pub(crate) struct PolicyBindingStage {
    installation_slot: usize,
    installation: WgpuPolicyInstallationId,
    definition: u64,
    bindings: Arc<PolicyBindings>,
}

/// Drop only after every owner participating in the Host publication has swapped coherently.
/// Fixed storage makes publication infallible without running arbitrary user destructors.
#[must_use]
pub(crate) struct RetiredPolicyBindings {
    _bindings: Option<Arc<PolicyBindings>>,
    _features: [Option<Arc<FeatureBinding>>; MAX_FEATURE_BINDINGS_PER_PRODUCER],
}

pub(crate) struct BuiltPolicy {
    pub(crate) output: WgpuPolicyProgram,
    pub(crate) custody: Arc<PolicyExecutionCustody>,
    pub(crate) metadata_bytes: usize,
    pub(crate) staged: PolicyBindingStage,
    pub(crate) lineage: Option<crate::WgpuHistoryLineage>,
    pub(crate) history: Option<crate::history::PreparedHistory>,
}

impl PolicyInstances {
    fn reserve_tables(&mut self) -> Result<(), WgpuRenderProgramError> {
        // Keep large bounded tables out of ProducerState and therefore out of inline BTreeMap
        // nodes and native callback stacks. Allocate their full stable slot space before binding;
        // candidate indices stay valid across reconciliation and no publication allocates.
        if self.policies.is_empty() {
            self.policies
                .try_reserve_exact(MAX_POLICIES_PER_PRODUCER)
                .map_err(|_| WgpuRenderProgramError::PolicyCapacity)?;
            self.policies
                .resize_with(MAX_POLICIES_PER_PRODUCER, || None);
        }
        if self.features.0.is_empty() {
            self.features
                .0
                .try_reserve_exact(MAX_FEATURE_BINDINGS_PER_PRODUCER)
                .map_err(|_| WgpuRenderProgramError::PolicyCapacity)?;
            self.features
                .0
                .resize_with(MAX_FEATURE_BINDINGS_PER_PRODUCER, || None);
        }
        Ok(())
    }

    fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError> {
        self.policies
            .capacity()
            .checked_mul(size_of::<Option<PolicyInstanceEntry>>())
            .and_then(|bytes| {
                self.features
                    .0
                    .capacity()
                    .checked_mul(size_of::<Option<FeatureResolution>>())
                    .and_then(|features| bytes.checked_add(features))
            })
            .and_then(|bytes| bytes.checked_add(size_of::<Self>()))
            .ok_or(WgpuRenderProgramError::PolicyCapacity)
    }

    pub(crate) fn maintain(&mut self) {
        // Do not compact slots: accepted candidates retain their pre-admitted slot indices.
        for entry in &mut self.policies {
            if entry
                .as_ref()
                .is_some_and(|entry| entry.admission.strong_count() == 0)
            {
                *entry = None;
            }
        }
        for slot in &mut self.features.0 {
            let Some(entry) = slot else {
                continue;
            };
            if entry.lifetime.strong_count() == 0 {
                entry.published = None;
            }
            if entry.binding.strong_count() == 0 {
                *slot = None;
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.policies.iter().flatten().count()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn build(
        &mut self,
        input: &WgpuRenderInput,
        backend_instance_id: u64,
        producer: WgpuRenderProducerId,
        closed: &Arc<AtomicBool>,
        format: wgpu::TextureFormat,
        history_owner: Option<&crate::history::HistoryRealization>,
    ) -> Result<BuiltPolicy, WgpuRenderProgramError> {
        if closed.load(Ordering::Acquire) {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        let installation = input.installation();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.maintain();
            input.capture.validate(&input.packet)?;
            let capture_metadata = input.capture.metadata_bytes()?;
            if capture_metadata > 256 * 1024 {
                return Err(WgpuRenderProgramError::PolicyCapacity);
            }
            self.reserve_tables()?;
            let installation_slot = if let Some(index) = self.policies.iter().position(|entry| {
                entry
                    .as_ref()
                    .is_some_and(|entry| entry.installation == installation)
            }) {
                if self.policies[index]
                    .as_ref()
                    .is_some_and(|entry| entry.definition != input.capture.definition_id())
                {
                    return Err(WgpuRenderProgramError::PolicyState);
                }
                index
            } else {
                self.policies
                    .iter()
                    .position(Option::is_none)
                    .ok_or(WgpuRenderProgramError::PolicyCapacity)?
            };
            let mut staged_features = StagedFeatureBindings::new()?;
            let mut history_declarations = crate::history::HistoryDeclarations::default();
            // Bind every candidate from its original capture. Never substitute a current
            // installation's latest topology or reuse its per-candidate Bindings value.
            let value = {
                let mut context = WgpuPolicyBindingContext {
                    backend_instance_id,
                    producer,
                    closed,
                    requirements: input.requirements(),
                    resolutions: &mut self.features,
                    staged: &mut staged_features,
                    packet: input.packet(),
                    history: &mut history_declarations,
                    temporal: input.lineage.is_some(),
                };
                input.capture.bind(&mut context)?
            };
            let history = match (history_owner, input.lineage.as_ref()) {
                (Some(owner), Some(lineage)) if owner.id == lineage.0.id => {
                    Some(history_declarations.pin(owner, input.packet().frame_index())?)
                }
                (None, None) if history_declarations.is_empty() => None,
                _ => return Err(WgpuRenderProgramError::HistoryDeclaration),
            };
            let output = input.capture.build(
                value.as_ref(),
                WgpuPolicyBuildContext {
                    packet: &input.packet,
                    format,
                    history: history.as_ref(),
                },
            )?;
            match &history {
                Some(history) => history.validate_bindings(&output.program, &output.history)?,
                None if output.history.is_empty() => {}
                None => return Err(WgpuRenderProgramError::HistoryDeclaration),
            }
            if output.program.source_generation() != input.packet.source_generation()
                || output.program.frame_index() != input.packet.frame_index()
            {
                return Err(WgpuRenderProgramError::PolicyState);
            }
            // A policy cannot smuggle a Feature with requirements outside its original request.
            for operation in &output.program.0.operations {
                if let Some(capture) = &operation.capture {
                    capture.requirements().validate_enabled(
                        input.requirements().required_features(),
                        input.requirements().required_limits(),
                    )?;
                }
            }
            let root_metadata =
                crate::submission::WgpuSubmissionReservation::modeled_metadata_bytes::<(
                    WgpuRenderProgram,
                    Arc<PolicyExecutionCustody>,
                )>()
                .checked_sub(
                    crate::submission::WgpuSubmissionReservation::modeled_metadata_bytes::<
                        WgpuRenderProgram,
                    >(),
                )
                .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
            let table_metadata = self.metadata_bytes()?;
            let metadata_bytes = capture_metadata
                .checked_add(size_of::<PolicyExecutionCustody>() + 2 * size_of::<usize>())
                .and_then(|bytes| {
                    bytes.checked_add(size_of::<PolicyBindings>() + 2 * size_of::<usize>())
                })
                .and_then(|bytes| bytes.checked_add(size_of::<PolicyBindingStage>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RetiredPolicyBindings>()))
                .and_then(|bytes| bytes.checked_add(staged_features.metadata_bytes))
                .and_then(|bytes| bytes.checked_add(table_metadata))
                .and_then(|bytes| bytes.checked_add(root_metadata))
                .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
            let metadata_bytes = match &history {
                Some(history) => metadata_bytes
                    .checked_add(history.metadata_bytes()?)
                    .ok_or(WgpuRenderProgramError::HistoryCapacity)?,
                None => metadata_bytes,
            };
            #[cfg(feature = "stock-ui")]
            let metadata_bytes = match &output.ui {
                Some(binding) => {
                    binding.validate(&output.program, &output.final_version)?;
                    metadata_bytes
                        .checked_add(binding.metadata_bytes())
                        .ok_or(WgpuRenderProgramError::PolicyCapacity)?
                }
                None => metadata_bytes,
            };
            output
                .program
                .modeled_metadata_bytes()
                .checked_add(metadata_bytes)
                .filter(|bytes| *bytes <= output.program.0.max_modeled_metadata_bytes)
                .ok_or(crate::GraphError::LimitExceeded(
                    crate::LimitKind::MetadataBytes,
                ))?;
            let bindings = Arc::new(PolicyBindings {
                _value: value,
                features: staged_features,
            });
            if self.policies[installation_slot].is_none() {
                // This weak reservation holds no published Bindings. The original accepted
                // input/candidate keeps the slot stable until its commit or retirement.
                self.policies[installation_slot] = Some(PolicyInstanceEntry {
                    admission: Arc::downgrade(input.capture.admission()),
                    installation,
                    definition: input.capture.definition_id(),
                    value: None,
                });
            }
            Ok(BuiltPolicy {
                output,
                lineage: input.lineage.clone(),
                history,
                custody: Arc::new(PolicyExecutionCustody {
                    _capture: Arc::clone(&input.capture),
                    _bindings: Arc::clone(&bindings),
                }),
                metadata_bytes,
                staged: PolicyBindingStage {
                    installation_slot,
                    installation,
                    definition: input.capture.definition_id(),
                    bindings,
                },
            })
        }));
        outcome.unwrap_or(Err(WgpuRenderProgramError::PolicyPanicked))
    }

    /// Publishes only pre-admitted pointer replacements. The caller validates candidate,
    /// history and producer facts first, then retains the returned old owners until all
    /// participating publication pointers and watermarks have committed.
    pub(crate) fn commit(&mut self, built: &BuiltPolicy) -> RetiredPolicyBindings {
        let staged = &built.staged;
        let entry = self.policies[staged.installation_slot]
            .as_mut()
            .expect("a live candidate retains its admitted policy slot");
        debug_assert_eq!(entry.installation, staged.installation);
        debug_assert_eq!(entry.definition, staged.definition);
        let mut retired = RetiredPolicyBindings {
            _bindings: entry.value.replace(Arc::clone(&staged.bindings)),
            _features: std::array::from_fn(|_| None),
        };
        for (index, binding) in staged.bindings.features.values.iter().enumerate() {
            let resolution = self.features.0[binding.slot]
                .as_mut()
                .expect("a live candidate retains its admitted Feature slot");
            debug_assert_eq!(resolution.declaration, binding.declaration);
            retired._features[index] = resolution.published.replace(Arc::clone(binding));
        }
        retired
    }
}

#[cfg(test)]
mod tests;
