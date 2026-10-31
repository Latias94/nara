//! Private ownership of one stock backend's device domain.

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{
    WgpuRenderError,
    backend::WgpuBackendState,
    host::WgpuRenderProducerId,
    requirements::{WgpuDeviceRequirements, WgpuDeviceRequirementsError},
    telemetry::WgpuAdapterSummary,
};

static NEXT_WGPU_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
const WGPU_FORCE_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK";
const INVALID_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK must be either 0 or 1";

fn force_fallback_adapter_from_value(value: Option<&OsStr>) -> Result<bool, &'static str> {
    match value {
        None => Ok(false),
        Some(value) if value == OsStr::new("0") => Ok(false),
        Some(value) if value == OsStr::new("1") => Ok(true),
        Some(_) => Err(INVALID_FALLBACK_ADAPTER_ENV),
    }
}

fn force_fallback_adapter_from_environment() -> Result<bool, &'static str> {
    let value = std::env::var_os(WGPU_FORCE_FALLBACK_ADAPTER_ENV);
    force_fallback_adapter_from_value(value.as_deref())
}

#[derive(Debug)]
struct WgpuBackendInstanceId(u64);

impl Default for WgpuBackendInstanceId {
    fn default() -> Self {
        let id = NEXT_WGPU_BACKEND_INSTANCE_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .unwrap_or_else(|_| std::process::abort());
        Self(id)
    }
}

#[derive(Debug, Default)]
struct DeviceLossSignal {
    lost: Arc<AtomicBool>,
    presentations: Mutex<DeviceWorldPresentations>,
}

#[derive(Debug, Default)]
struct DeviceWorldPresentations {
    #[cfg(test)]
    runtime: Option<nara_render::__private::WorldPresentationRevoker>,
    editor: BTreeMap<WgpuRenderProducerId, nara_render::__private::WorldPresentationRevoker>,
}

impl DeviceLossSignal {
    fn mark_lost(&self) {
        self.lost.store(true, Ordering::Release);
        let presentations = self
            .presentations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        #[cfg(test)]
        if let Some(presentation) = &presentations.runtime {
            presentation.invalidate();
        }
        for presentation in presentations.editor.values() {
            presentation.invalidate();
        }
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    #[cfg(test)]
    fn retain_presentation(&self, revoker: nara_render::__private::WorldPresentationRevoker) {
        let mut presentations = self
            .presentations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_lost() {
            revoker.invalidate();
        } else if let Some(previous) = presentations.runtime.replace(revoker) {
            previous.invalidate();
        }
    }

    fn retain_editor_presentation(
        &self,
        view: WgpuRenderProducerId,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) {
        let mut presentations = self
            .presentations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_lost() {
            revoker.invalidate();
        } else if let Some(previous) = presentations.editor.insert(view, revoker) {
            previous.invalidate();
        }
    }

    fn retire_editor_presentation(&self, view: WgpuRenderProducerId) {
        if let Some(previous) = self
            .presentations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .editor
            .remove(&view)
        {
            previous.invalidate();
        }
    }
}

#[derive(Debug)]
pub(super) struct WgpuReadyDevice {
    pub(super) queue: wgpu::Queue,
    pub(super) device: wgpu::Device,
    pub(super) adapter: wgpu::Adapter,
    loss: Arc<DeviceLossSignal>,
}

#[derive(Debug, Default)]
enum DeviceDomain {
    #[default]
    Uninitialized,
    Initializing,
    Ready(WgpuReadyDevice),
    Unavailable,
    // Native draw fixtures lend an inseparable Device/Queue pair without an Adapter or Instance.
    // This is deliberately not a production Ready generation.
    #[cfg(test)]
    Probe {
        device_queue: Option<(wgpu::Device, wgpu::Queue)>,
        loss: Arc<DeviceLossSignal>,
    },
}

/// Owns initialization, namespace identity and loss for every stock execution path.
///
/// The Instance can exist before acquisition so the backend can create an owning Surface.
/// Ready always contains the complete Adapter/Device/Queue generation and its exact loss signal.
#[derive(Debug, Default)]
pub(super) struct WgpuDeviceOwner {
    instance_id: WgpuBackendInstanceId,
    domain: DeviceDomain,
    instance: Option<wgpu::Instance>,
    last_epoch: u64,
    // Once a device epoch exists this is the original closed request, including across clear.
    // Late subset admission cannot change the request used to recover that device domain.
    requirements: WgpuDeviceRequirements,
}

impl WgpuDeviceOwner {
    pub(super) fn output_epoch_guard(&self) -> Result<Arc<AtomicBool>, WgpuRenderError> {
        self.loss_signal()
            .map(|loss| Arc::clone(&loss.lost))
            .ok_or(WgpuRenderError::BackendNotReady)
    }
    pub(super) const fn instance_id(&self) -> u64 {
        self.instance_id.0
    }

    pub(super) const fn state(&self) -> WgpuBackendState {
        match &self.domain {
            DeviceDomain::Uninitialized => WgpuBackendState::Uninitialized,
            DeviceDomain::Initializing => WgpuBackendState::Initializing,
            DeviceDomain::Ready(_) => WgpuBackendState::Ready,
            DeviceDomain::Unavailable => WgpuBackendState::Unavailable,
            #[cfg(test)]
            DeviceDomain::Probe { .. } => WgpuBackendState::Uninitialized,
        }
    }

    pub(super) const fn device_epoch(&self) -> u64 {
        self.last_epoch
    }

    pub(super) fn instance(&self) -> Result<&wgpu::Instance, WgpuRenderError> {
        self.instance
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)
    }

    pub(super) fn ready(&self) -> Result<&WgpuReadyDevice, WgpuRenderError> {
        match &self.domain {
            DeviceDomain::Ready(ready) => Ok(ready),
            _ => Err(WgpuRenderError::BackendNotReady),
        }
    }

    pub(super) fn device_queue(&self) -> Option<(&wgpu::Device, &wgpu::Queue)> {
        match &self.domain {
            DeviceDomain::Ready(ready) => Some((&ready.device, &ready.queue)),
            #[cfg(test)]
            DeviceDomain::Probe { device_queue, .. } => {
                device_queue.as_ref().map(|(device, queue)| (device, queue))
            }
            _ => None,
        }
    }

    pub(super) fn adapter_summary(&self) -> Option<WgpuAdapterSummary> {
        let info = self.ready().ok()?.adapter.get_info();
        Some(WgpuAdapterSummary::from_parts(
            &info.name,
            info.vendor,
            info.device,
            device_type_label(info.device_type),
            backend_label(info.backend),
            &info.driver,
            &info.driver_info,
        ))
    }

    /// Contributes before the first device, or admits a subset of the original enabled request.
    ///
    /// No native acquisition, epoch transition or healthy-device replacement occurs here.
    pub(super) fn request_device_requirements(
        &mut self,
        requirements: &WgpuDeviceRequirements,
    ) -> Result<(), WgpuDeviceRequirementsError> {
        requirements.validate_request()?;
        match &self.domain {
            DeviceDomain::Uninitialized if self.last_epoch == 0 => {
                self.requirements = self.requirements.union(requirements);
                return Ok(());
            }
            DeviceDomain::Ready(ready) => {
                if ready.loss.is_lost() {
                    return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
                }
                requirements.validate_late_request(
                    ready.adapter.features(),
                    &ready.adapter.limits(),
                    ready.device.features(),
                    &ready.device.limits(),
                )?;
            }
            DeviceDomain::Unavailable => {
                return Err(WgpuDeviceRequirementsError::BackendUnavailable);
            }
            DeviceDomain::Initializing => {
                return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
            }
            DeviceDomain::Uninitialized => {}
            #[cfg(test)]
            DeviceDomain::Probe { device_queue, loss } => {
                let Some((device, _)) = device_queue else {
                    return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
                };
                if loss.is_lost()
                    || !requirements.is_satisfied_by(device.features(), &device.limits())
                {
                    return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
                }
            }
        }
        // Recovery is the same admitted request. Even an implementation exposing capabilities
        // beyond the request cannot promise them for a later epoch of this original owner.
        if !requirements.is_satisfied_by(
            self.requirements.required_features(),
            self.requirements.required_limits(),
        ) {
            return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
        }
        Ok(())
    }

    pub(super) fn ensure_instance(&mut self) -> Result<(), WgpuRenderError> {
        if self.state() == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }
        if self.instance.is_none() {
            self.instance = Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            ));
        }
        Ok(())
    }

    /// Acquires the normal stock device with or without a surface selection constraint.
    ///
    /// A returned epoch identifies a newly installed complete generation. Existing Ready devices
    /// are never reselected; the backend independently validates/configures each actual surface.
    pub(super) fn acquire(
        &mut self,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Option<u64>, WgpuRenderError> {
        self.fail_if_device_lost()?;
        match &self.domain {
            DeviceDomain::Ready(_) => return Ok(None),
            DeviceDomain::Unavailable => return Err(WgpuRenderError::BackendUnavailable),
            _ => {}
        }
        let epoch = self
            .last_epoch
            .checked_add(1)
            .ok_or(WgpuRenderError::DeviceEpochExhausted)?;
        let force_fallback_adapter =
            force_fallback_adapter_from_environment().map_err(|message| {
                WgpuRenderError::AdapterUnavailable {
                    message: message.to_owned(),
                }
            })?;
        self.ensure_instance()?;
        self.invalidate_presentations();
        self.domain = DeviceDomain::Initializing;
        let requested = (|| {
            let adapter = pollster::block_on(self.instance()?.request_adapter(
                &wgpu::RequestAdapterOptions {
                    compatible_surface,
                    force_fallback_adapter,
                    ..Default::default()
                },
            ))
            .map_err(|error| WgpuRenderError::AdapterUnavailable {
                message: error.to_string(),
            })?;
            self.requirements
                .validate_adapter(adapter.features(), &adapter.limits())
                .map_err(WgpuRenderError::DeviceRequirements)?;
            let descriptor = self.requirements.descriptor();
            let (device, queue) =
                pollster::block_on(adapter.request_device(&descriptor)).map_err(|error| {
                    WgpuRenderError::DeviceUnavailable {
                        message: error.to_string(),
                    }
                })?;
            self.requirements
                .validate_enabled(device.features(), &device.limits())
                .map_err(WgpuRenderError::DeviceRequirements)?;
            let loss = Arc::new(DeviceLossSignal::default());
            let callback_signal = Arc::clone(&loss);
            device.set_device_lost_callback(move |_reason, _message| callback_signal.mark_lost());
            Ok(WgpuReadyDevice {
                queue,
                device,
                adapter,
                loss,
            })
        })();
        let ready = match requested {
            Ok(ready) => ready,
            Err(error) => {
                // This synchronous request has finished; no operation remains to advance.
                // Keep the Instance alive for any surface already held by the backend.
                self.domain = DeviceDomain::Uninitialized;
                return Err(error);
            }
        };
        self.domain = DeviceDomain::Ready(ready);
        self.last_epoch = epoch;
        Ok(Some(epoch))
    }

    pub(super) fn fail_if_device_lost(&self) -> Result<(), WgpuRenderError> {
        // Poll before inspecting the signal, including a destroyed device with no queued work.
        // A correlated loss takes priority over a poll error, as in the stock backend path.
        let polled = self
            .device_queue()
            .map(|(device, _)| device.poll(wgpu::PollType::Poll));
        if self.loss_signal().is_some_and(DeviceLossSignal::is_lost) {
            Err(WgpuRenderError::DeviceLost)
        } else {
            polled
                .transpose()
                .map(|_| ())
                .map_err(|_| WgpuRenderError::TelemetryCompletionPollFailed)
        }
    }

    #[cfg(test)]
    pub(super) fn retain_presentation(
        &self,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) {
        if let Some(loss) = self.loss_signal() {
            loss.retain_presentation(revoker);
        } else {
            revoker.invalidate();
        }
    }

    pub(super) fn retain_editor_presentation(
        &self,
        view: WgpuRenderProducerId,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) -> Result<(), WgpuRenderError> {
        self.loss_signal()
            .ok_or(WgpuRenderError::BackendNotReady)?
            .retain_editor_presentation(view, revoker);
        Ok(())
    }

    pub(super) fn retire_editor_presentation(&self, view: WgpuRenderProducerId) {
        if let Some(loss) = self.loss_signal() {
            loss.retire_editor_presentation(view);
        }
    }

    pub(super) fn invalidate_presentations(&self) {
        if let Some(loss) = self.loss_signal() {
            loss.mark_lost();
        }
    }

    /// Releases native state without changing the original request, namespace or terminal failure.
    pub(super) fn clear(&mut self) {
        self.invalidate_presentations();
        let unavailable = self.state() == WgpuBackendState::Unavailable;
        self.domain = if unavailable {
            DeviceDomain::Unavailable
        } else {
            DeviceDomain::Uninitialized
        };
        self.instance = None;
    }

    pub(super) fn mark_unavailable(&mut self) {
        self.clear();
        self.domain = DeviceDomain::Unavailable;
    }

    fn loss_signal(&self) -> Option<&DeviceLossSignal> {
        match &self.domain {
            DeviceDomain::Ready(ready) => Some(ready.loss.as_ref()),
            #[cfg(test)]
            DeviceDomain::Probe { loss, .. } => Some(loss.as_ref()),
            _ => None,
        }
    }

    /// Sets only an already-retired epoch watermark for CPU receipt/cleanup fixtures.
    #[cfg(test)]
    pub(super) fn with_retired_epoch(epoch: std::num::NonZeroU64) -> Self {
        let mut owner = Self::default();
        owner.last_epoch = epoch.get();
        owner
    }

    #[cfg(test)]
    pub(super) fn install_probe_device(&mut self, device: wgpu::Device, queue: wgpu::Queue) {
        assert!(matches!(self.domain, DeviceDomain::Uninitialized));
        let epoch = self
            .last_epoch
            .checked_add(1)
            .expect("test epoch exhausted");
        self.domain = DeviceDomain::Probe {
            device_queue: Some((device, queue)),
            loss: Arc::new(DeviceLossSignal::default()),
        };
        self.last_epoch = epoch;
    }

    #[cfg(test)]
    pub(super) fn with_loss_observation() -> Self {
        let loss = Arc::new(DeviceLossSignal::default());
        loss.mark_lost();
        let mut owner = Self::default();
        owner.domain = DeviceDomain::Probe {
            device_queue: None,
            loss,
        };
        owner
    }

    #[cfg(test)]
    pub(super) fn mark_probe_lost(&self) {
        let DeviceDomain::Probe { loss, .. } = &self.domain else {
            panic!("loss injection requires an explicit probe fixture");
        };
        loss.mark_lost();
    }
}

impl Drop for WgpuDeviceOwner {
    fn drop(&mut self) {
        self.invalidate_presentations();
    }
}

const fn device_type_label(device_type: wgpu::DeviceType) -> &'static str {
    match device_type {
        wgpu::DeviceType::Other => "other",
        wgpu::DeviceType::IntegratedGpu => "integrated-gpu",
        wgpu::DeviceType::DiscreteGpu => "discrete-gpu",
        wgpu::DeviceType::VirtualGpu => "virtual-gpu",
        wgpu::DeviceType::Cpu => "cpu",
    }
}

const fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Noop => "noop",
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        wgpu::Backend::BrowserWebGpu => "browser-webgpu",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_adapter_environment_is_absent_or_explicitly_boolean() {
        assert_eq!(force_fallback_adapter_from_value(None), Ok(false));
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("0"))),
            Ok(false)
        );
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("1"))),
            Ok(true)
        );
    }

    #[test]
    fn fallback_adapter_environment_rejects_ambiguous_values() {
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("true"))),
            Err(INVALID_FALLBACK_ADAPTER_ENV)
        );
    }

    #[test]
    fn pre_device_requirements_aggregate_without_native_acquisition() {
        let mut owner = WgpuDeviceOwner::default();
        let first = WgpuDeviceRequirements::new(
            wgpu::Features::TIMESTAMP_QUERY,
            wgpu::Limits {
                max_buffer_size: 512 * 1024 * 1024,
                ..Default::default()
            },
        );
        let second = WgpuDeviceRequirements::new(
            wgpu::Features::TEXTURE_COMPRESSION_BC,
            wgpu::Limits {
                min_storage_buffer_offset_alignment: 64,
                ..Default::default()
            },
        );
        owner.request_device_requirements(&first).unwrap();
        owner.request_device_requirements(&second).unwrap();
        assert_eq!(owner.requirements, first.union(&second));
        assert_eq!(owner.state(), WgpuBackendState::Uninitialized);
        assert_eq!(owner.device_epoch(), 0);
        assert!(owner.instance().is_err());
        assert!(owner.ready().is_err());
        owner.clear();
        assert_eq!(owner.requirements, first.union(&second));
    }

    #[test]
    fn retired_epoch_keeps_its_request_and_refuses_new_recovery_capabilities() {
        let original =
            WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
        let mut owner = WgpuDeviceOwner::with_retired_epoch(std::num::NonZeroU64::new(7).unwrap());
        owner.requirements = original.clone();
        owner.clear();
        assert_eq!(owner.request_device_requirements(&original), Ok(()));
        assert_eq!(
            owner.request_device_requirements(&WgpuDeviceRequirements::default()),
            Ok(())
        );
        let added_feature = WgpuDeviceRequirements::new(
            wgpu::Features::TEXTURE_COMPRESSION_BC,
            wgpu::Limits::default(),
        );
        assert_eq!(
            owner.request_device_requirements(&added_feature),
            Err(WgpuDeviceRequirementsError::ReconstructionRequired)
        );
        let added_limit = WgpuDeviceRequirements::new(
            wgpu::Features::empty(),
            wgpu::Limits {
                min_storage_buffer_offset_alignment: 64,
                ..Default::default()
            },
        );
        assert_eq!(
            owner.request_device_requirements(&added_limit),
            Err(WgpuDeviceRequirementsError::ReconstructionRequired)
        );
        assert_eq!(owner.requirements, original);
        assert_eq!(owner.device_epoch(), 7);
        assert!(owner.instance().is_err());
        assert!(owner.ready().is_err());
    }

    #[test]
    fn rejected_requirements_do_not_change_or_reopen_the_owner() {
        let mut owner = WgpuDeviceOwner::default();
        let experimental = WgpuDeviceRequirements::new(
            wgpu::Features::all_experimental_mask(),
            wgpu::Limits::default(),
        );
        assert!(matches!(
            owner.request_device_requirements(&experimental),
            Err(WgpuDeviceRequirementsError::ExperimentalFeaturesUnsupported { .. })
        ));
        assert_eq!(owner.requirements, WgpuDeviceRequirements::default());
        assert!(owner.instance().is_err());
        owner.mark_unavailable();
        assert_eq!(
            owner.request_device_requirements(&WgpuDeviceRequirements::default()),
            Err(WgpuDeviceRequirementsError::BackendUnavailable)
        );
        assert_eq!(owner.state(), WgpuBackendState::Unavailable);
        assert_eq!(owner.requirements, WgpuDeviceRequirements::default());
        assert_eq!(owner.device_epoch(), 0);
    }

    #[test]
    #[ignore = "requires a native GPU with timestamp queries and another stable feature; acquisition failure is a failed prerequisite"]
    fn private_device_requirements_enable_exact_features_and_survive_original_epoch_recovery() {
        let mut owner = WgpuDeviceOwner::default();
        let original =
            WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
        owner.request_device_requirements(&original).unwrap();
        assert_eq!(owner.acquire(None).unwrap(), Some(1));
        let ready = owner.ready().unwrap();
        let old_device = ready.device.clone();
        assert_eq!(ready.device.features(), original.required_features());
        assert!(
            original
                .required_limits()
                .check_limits(&ready.device.limits())
        );
        let supported_extras = ready.adapter.features()
            - ready.device.features()
            - wgpu::Features::all_experimental_mask();
        assert!(
            !supported_extras.is_empty(),
            "the selected native adapter must expose an additional stable feature"
        );
        let supported_size = ready.adapter.limits().max_buffer_size;
        let unsupported_size = supported_size.checked_add(1).unwrap();
        assert_eq!(
            owner.request_device_requirements(&WgpuDeviceRequirements::default()),
            Ok(())
        );
        assert_eq!(
            owner.request_device_requirements(&WgpuDeviceRequirements::new(
                supported_extras,
                wgpu::Limits::default(),
            )),
            Err(WgpuDeviceRequirementsError::ReconstructionRequired)
        );
        assert_eq!(
            owner.request_device_requirements(&WgpuDeviceRequirements::new(
                wgpu::Features::empty(),
                wgpu::Limits {
                    max_buffer_size: unsupported_size,
                    ..Default::default()
                },
            )),
            Err(WgpuDeviceRequirementsError::UnsupportedLimit {
                name: "max_buffer_size",
                required: unsupported_size,
                supported: supported_size,
            })
        );
        assert_eq!(owner.requirements, original);
        assert_eq!(owner.ready().unwrap().device, old_device);
        assert_eq!(owner.acquire(None).unwrap(), None);
        assert_eq!(owner.device_epoch(), 1);

        old_device.destroy();
        assert_eq!(
            owner.fail_if_device_lost(),
            Err(WgpuRenderError::DeviceLost)
        );
        owner.clear();
        assert_eq!(owner.requirements, original);
        assert_eq!(owner.acquire(None).unwrap(), Some(2));
        assert_eq!(
            owner.ready().unwrap().device.features(),
            original.required_features()
        );
        assert!(
            original
                .required_limits()
                .check_limits(&owner.ready().unwrap().device.limits())
        );
        assert_eq!(owner.requirements, original);
        assert_eq!(owner.device_epoch(), 2);
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn private_surface_less_acquisition_keeps_one_complete_generation_and_monotonic_epoch() {
        let mut owner = WgpuDeviceOwner::default();
        let incarnation = owner.instance_id();
        assert_eq!(owner.acquire(None).unwrap(), Some(1));
        assert_eq!(owner.state(), WgpuBackendState::Ready);
        assert!(owner.instance().is_ok());
        assert!(owner.adapter_summary().is_some());
        let old_device = owner.ready().unwrap().device.clone();
        let old_queue = owner.ready().unwrap().queue.clone();
        assert_eq!(owner.acquire(None).unwrap(), None);
        assert_eq!(owner.ready().unwrap().device, old_device);
        assert_eq!(owner.ready().unwrap().queue, old_queue);
        assert_eq!(owner.device_epoch(), 1);

        owner.clear();
        assert_eq!(owner.state(), WgpuBackendState::Uninitialized);
        assert_eq!(owner.instance_id(), incarnation);
        assert_eq!(owner.device_epoch(), 1);
        assert!(owner.instance().is_err());
        assert!(owner.ready().is_err());
        assert!(owner.device_queue().is_none());

        assert_eq!(owner.acquire(None).unwrap(), Some(2));
        assert_eq!(owner.state(), WgpuBackendState::Ready);
        assert_eq!(owner.instance_id(), incarnation);
        assert_eq!(owner.device_epoch(), 2);
        // Native IDs and equality can repeat across Instances. The owner/epoch namespace and
        // independent native behavior establish generation isolation, not handle inequality.
        // A late native loss callback for the retained predecessor cannot poison its successor.
        old_device.destroy();
        old_device.poll(wgpu::PollType::Poll).unwrap();
        assert_eq!(owner.fail_if_device_lost(), Ok(()));
        assert_eq!(owner.device_epoch(), 2);
        let ready = owner.ready().unwrap();
        let validation = ready.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let buffer = ready.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("successor-after-predecessor-loss"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = ready
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.clear_buffer(&buffer, 0, None);
        let submission = ready.queue.submit([encoder.finish()]);
        ready
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
        assert!(pollster::block_on(validation.pop()).is_none());

        owner.mark_unavailable();
        owner.clear();
        assert_eq!(owner.state(), WgpuBackendState::Unavailable);
        assert_eq!(owner.device_epoch(), 2);
        assert_eq!(
            owner.acquire(None),
            Err(WgpuRenderError::BackendUnavailable)
        );
        assert!(owner.instance().is_err());
        assert!(owner.ready().is_err());
        assert!(owner.device_queue().is_none());
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn private_surface_less_device_observes_native_loss_without_queued_work() {
        let mut owner = WgpuDeviceOwner::default();
        assert_eq!(owner.acquire(None).unwrap(), Some(1));
        owner.ready().unwrap().device.destroy();
        assert_eq!(
            owner.fail_if_device_lost(),
            Err(WgpuRenderError::DeviceLost)
        );
        assert_eq!(owner.acquire(None), Err(WgpuRenderError::DeviceLost));
        assert_eq!(owner.device_epoch(), 1);
        owner.mark_unavailable();
        assert_eq!(owner.state(), WgpuBackendState::Unavailable);
        assert_eq!(owner.fail_if_device_lost(), Ok(()));
        assert!(owner.device_queue().is_none());
    }

    #[test]
    fn exhausted_epoch_cannot_publish_a_ready_generation_or_rewind_on_clear() {
        let mut owner =
            WgpuDeviceOwner::with_retired_epoch(std::num::NonZeroU64::new(u64::MAX).unwrap());
        assert_eq!(
            owner.acquire(None),
            Err(WgpuRenderError::DeviceEpochExhausted)
        );
        assert_eq!(owner.state(), WgpuBackendState::Uninitialized);
        assert!(owner.instance().is_err());
        assert!(owner.ready().is_err());
        assert!(owner.device_queue().is_none());
        owner.clear();
        assert_eq!(owner.state(), WgpuBackendState::Uninitialized);
        assert_eq!(owner.device_epoch(), u64::MAX);
    }

    #[test]
    fn device_loss_revokes_runtime_and_editor_world_before_render_or_registration() {
        use nara_ecs::{World, bevy_ecs::system::SystemState};
        use nara_render::{
            __private::prepare_world_presentation, Color, ExtractedView, ExtractedViews,
            RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect, WorldPointer,
            build_render_frame_packet,
        };
        use nara_window::Window;

        let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
        let generation = runtime.generation();
        for (editor, lost_before_registration) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let mut world = World::new();
            let window = Window {
                resolution: nara_window::WindowResolution::new(64, 64),
                ..Default::default()
            };
            world.spawn(window.clone());
            let camera = world.spawn_empty().id();
            let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
            let mut views = ExtractedViews::default();
            views.push(ExtractedView {
                id: nara_render::RenderViewId::try_acquire().unwrap(),
                camera_entity: camera,
                target: RenderTarget::Window(window.id),
                viewport,
                mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 64.0, viewport)
                    .unwrap(),
                order: 0,
                load: ViewLoadOp::Clear(Color::BLACK),
            });
            let packet =
                build_render_frame_packet(generation, 1, Some(window.id), [&window], &views)
                    .unwrap()
                    .unwrap();
            world.insert_resource(generation);
            let (frame, guard) = prepare_world_presentation(&packet).unwrap();
            let signal = DeviceLossSignal::default();
            if lost_before_registration {
                signal.mark_lost();
            }
            let view = WgpuRenderProducerId::new(1);
            if editor {
                signal.retain_editor_presentation(view, guard.revoker());
            } else {
                signal.retain_presentation(guard.revoker());
            }
            guard.mark_presented();
            world.insert_resource(frame);
            let mut parameter = SystemState::<WorldPointer>::new(&mut world);
            let point = nara_core::Vec2::splat(32.0);
            if !lost_before_registration {
                assert!(
                    parameter
                        .get(&world)
                        .unwrap()
                        .window_to_world(camera, point)
                        .is_some()
                );
                signal.mark_lost();
            }
            // No backend Render invocation or device polling is needed to revoke gameplay input.
            assert!(signal.is_lost());
            assert!(
                parameter
                    .get(&world)
                    .unwrap()
                    .window_to_world(camera, point)
                    .is_none()
            );
            signal.retire_editor_presentation(view);
            assert!(signal.presentations.lock().unwrap().editor.is_empty());
        }
        runtime.finish().unwrap();
    }
}
