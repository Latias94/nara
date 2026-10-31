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
    WgpuDeviceRequirements, WgpuFeatureDefinition, WgpuFeatureInstance, WgpuProgramTexture,
    WgpuRenderFeature, WgpuRenderProducerId, WgpuRenderProgram, WgpuRenderProgramError,
    graph::{ResourceDescriptor, ResourceVersion},
};

static NEXT_POLICY_DEFINITION: AtomicU64 = AtomicU64::new(1);
static NEXT_POLICY_INSTALLATION: AtomicU64 = AtomicU64::new(1);
const MAX_POLICIES_PER_PRODUCER: usize = 64;

/// A renderer's captured CPU data and exact graph construction implementation.
///
/// Instances contain CPU Feature identities, not native mutable state. Capture systems read
/// ordinary ECS parameters; these methods receive no World, Device, Queue or presentation owner.
pub trait WgpuRenderPolicy: Send + Sync + 'static {
    type Capture: Send + Sync + 'static;
    type Instances: Send + Sync + 'static;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        WgpuDeviceRequirements::default()
    }

    fn create_instances(
        &self,
        context: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<Self::Instances, WgpuRenderProgramError>;

    fn validate_capture(
        &self,
        _packet: &RenderFramePacket,
        _capture: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn build(
        &self,
        instances: &Self::Instances,
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
        Ok(Arc::new(TypedPolicyCapture::<P> {
            admission: Arc::clone(&self.admission),
            definition: Arc::clone(&self.definition.0),
            capture,
        }))
    }
}

/// Immutable reconstruction input; clone retains the exact packet, capture and definition.
#[derive(Clone)]
pub struct WgpuRenderInput {
    pub(crate) packet: Arc<RenderFramePacket>,
    pub(crate) capture: Arc<dyn CapturedPolicy>,
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
        Ok(Self {
            packet: Arc::new(packet),
            capture,
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
        Self::from_capture(packet, Arc::clone(&self.capture))
    }

    pub(crate) fn requirements(&self) -> &WgpuDeviceRequirements {
        self.capture.requirements()
    }
}

/// CPU Feature construction restricted to one policy's original closed device request.
pub struct WgpuPolicyInstanceContext<'a> {
    backend_instance_id: u64,
    producer: WgpuRenderProducerId,
    closed: &'a Arc<AtomicBool>,
    requirements: &'a WgpuDeviceRequirements,
}

impl WgpuPolicyInstanceContext<'_> {
    pub fn feature<F: WgpuRenderFeature>(
        &mut self,
        definition: &WgpuFeatureDefinition<F>,
    ) -> Result<WgpuFeatureInstance<F>, WgpuRenderProgramError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        definition.device_requirements().validate_enabled(
            self.requirements.required_features(),
            self.requirements.required_limits(),
        )?;
        WgpuFeatureInstance::new(
            self.backend_instance_id,
            self.producer,
            Arc::clone(self.closed),
            definition,
        )
    }
}

/// Actual captured target and native destination facts, without native execution authority.
#[derive(Clone, Copy)]
pub struct WgpuPolicyBuildContext<'a> {
    packet: &'a RenderFramePacket,
    format: wgpu::TextureFormat,
}

impl<'a> WgpuPolicyBuildContext<'a> {
    #[must_use]
    pub const fn packet(self) -> &'a RenderFramePacket {
        self.packet
    }

    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        self.format
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
            #[cfg(feature = "stock-ui")]
            ui: None,
        })
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
    fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError>;
    fn validate(&self, packet: &RenderFramePacket) -> Result<(), WgpuRenderProgramError>;
    fn create_instances(
        &self,
        context: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, WgpuRenderProgramError>;
    fn build(
        &self,
        instances: &(dyn Any + Send + Sync),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError>;
}

struct TypedPolicyCapture<P: WgpuRenderPolicy> {
    admission: Arc<PolicyCaptureAdmission>,
    definition: Arc<PolicyDefinition<P>>,
    capture: Arc<P::Capture>,
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

    fn metadata_bytes(&self) -> Result<usize, WgpuRenderProgramError> {
        // Exact inline values and their original Arc counters. Indirect policy-owned data
        // remains the capture/factory's responsibility, just as for native Feature captures.
        [
            size_of::<Self>(),
            size_of::<P::Capture>(),
            size_of::<P::Instances>(),
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

    fn create_instances(
        &self,
        context: &mut WgpuPolicyInstanceContext<'_>,
    ) -> Result<Arc<dyn Any + Send + Sync>, WgpuRenderProgramError> {
        Ok(Arc::new(self.definition.policy.create_instances(context)?))
    }

    fn build(
        &self,
        instances: &(dyn Any + Send + Sync),
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let instances = instances
            .downcast_ref::<P::Instances>()
            .ok_or(WgpuRenderProgramError::PolicyState)?;
        self.definition
            .policy
            .build(instances, &self.capture, context)
    }
}

struct PolicyInstanceEntry {
    admission: Weak<PolicyCaptureAdmission>,
    installation: WgpuPolicyInstallationId,
    definition: u64,
    value: Arc<dyn Any + Send + Sync>,
}

/// CPU instance retention follows actual installations and accepted inputs, not lifetime IDs.
pub(crate) struct PolicyInstances([Option<PolicyInstanceEntry>; MAX_POLICIES_PER_PRODUCER]);

impl Default for PolicyInstances {
    fn default() -> Self {
        Self(std::array::from_fn(|_| None))
    }
}

pub(crate) struct PolicyExecutionCustody {
    _capture: Arc<dyn CapturedPolicy>,
    _instances: Arc<dyn Any + Send + Sync>,
}

pub(crate) struct BuiltPolicy {
    pub(crate) output: WgpuPolicyProgram,
    pub(crate) custody: Arc<PolicyExecutionCustody>,
    pub(crate) metadata_bytes: usize,
}

impl PolicyInstances {
    pub(crate) fn maintain(&mut self) {
        let mut retained = 0;
        for index in 0..self.0.len() {
            if self.0[index]
                .as_ref()
                .is_some_and(|entry| entry.admission.strong_count() != 0)
            {
                self.0.swap(retained, index);
                retained += 1;
            } else {
                self.0[index] = None;
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.iter().flatten().count()
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
    ) -> Result<BuiltPolicy, WgpuRenderProgramError> {
        if closed.load(Ordering::Acquire) {
            return Err(WgpuRenderProgramError::FeatureClosed);
        }
        self.maintain();
        let installation = input.installation();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            input.capture.validate(&input.packet)?;
            if !self
                .0
                .iter()
                .flatten()
                .any(|entry| entry.installation == installation)
            {
                let index = self
                    .0
                    .iter()
                    .position(Option::is_none)
                    .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
                let mut context = WgpuPolicyInstanceContext {
                    backend_instance_id,
                    producer,
                    closed,
                    requirements: input.requirements(),
                };
                let value = input.capture.create_instances(&mut context)?;
                self.0[index] = Some(PolicyInstanceEntry {
                    admission: Arc::downgrade(input.capture.admission()),
                    installation,
                    definition: input.capture.definition_id(),
                    value,
                });
            }
            let entry = self
                .0
                .iter()
                .flatten()
                .find(|entry| entry.installation == installation)
                .ok_or(WgpuRenderProgramError::PolicyState)?;
            if entry.definition != input.capture.definition_id() {
                return Err(WgpuRenderProgramError::PolicyState);
            }
            let instances = Arc::clone(&entry.value);
            let output = input.capture.build(
                instances.as_ref(),
                WgpuPolicyBuildContext {
                    packet: &input.packet,
                    format,
                },
            )?;
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
            let metadata_bytes = input
                .capture
                .metadata_bytes()?
                .checked_add(size_of::<PolicyExecutionCustody>() + 2 * size_of::<usize>())
                .and_then(|bytes| bytes.checked_add(size_of::<Self>()))
                .and_then(|bytes| bytes.checked_add(root_metadata))
                .ok_or(WgpuRenderProgramError::PolicyCapacity)?;
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
            Ok(BuiltPolicy {
                output,
                custody: Arc::new(PolicyExecutionCustody {
                    _capture: Arc::clone(&input.capture),
                    _instances: instances,
                }),
                metadata_bytes,
            })
        }));
        outcome.unwrap_or_else(|_| {
            for entry in &mut self.0 {
                if entry
                    .as_ref()
                    .is_some_and(|entry| entry.installation == installation)
                {
                    *entry = None;
                }
            }
            self.maintain();
            Err(WgpuRenderProgramError::PolicyPanicked)
        })
    }
}

#[cfg(test)]
mod tests;
