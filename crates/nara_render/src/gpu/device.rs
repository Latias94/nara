use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::WgpuRenderError;

/// Observes the callbacks installed by the owner of a shared device.
pub trait GpuDeviceErrors: Send + Sync {
    /// Returns the latest uncaptured error once for each observer's cursor.
    fn observe_error(&self, cursor: &mut u64) -> Option<String>;

    /// Registers a weak observer for device loss, invoking it immediately if already lost.
    fn on_device_lost(&self, observer: &Arc<dyn Fn() + Send + Sync>);
}

enum ErrorSource {
    Owned(Arc<Mutex<(u64, String)>>),
    Shared {
        source: Arc<dyn GpuDeviceErrors>,
        _loss_observer: Arc<dyn Fn() + Send + Sync>,
    },
}

/// The device, queue, and error callbacks used by a renderer for its entire lifetime.
#[derive(Clone)]
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    errors: Arc<ErrorSource>,
    lost: Arc<AtomicBool>,
}

impl GpuContext {
    /// Uses the application's handles and observes its existing callbacks.
    pub fn from_shared(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        errors: Arc<dyn GpuDeviceErrors>,
    ) -> Self {
        let lost = Arc::new(AtomicBool::new(false));
        let loss_observer: Arc<dyn Fn() + Send + Sync> = {
            let lost = Arc::clone(&lost);
            Arc::new(move || lost.store(true, Ordering::Release))
        };
        errors.on_device_lost(&loss_observer);
        Self {
            instance,
            adapter,
            device,
            queue,
            errors: Arc::new(ErrorSource::Shared {
                source: errors,
                _loss_observer: loss_observer,
            }),
            lost,
        }
    }

    /// Requests an application-owned GPU context before starting native rendering.
    #[cfg(not(target_family = "wasm"))]
    pub fn request(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, WgpuRenderError> {
        pollster::block_on(Self::request_async(instance, compatible_surface))
    }

    pub async fn request_async(
        instance: wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, WgpuRenderError> {
        let force_fallback_adapter = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
            Ok("1") => true,
            Ok("0") | Err(std::env::VarError::NotPresent) => false,
            _ => {
                return Err(WgpuRenderError::Device(
                    "NARA_WGPU_FORCE_FALLBACK must be either 0 or 1".into(),
                ));
            }
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface,
                force_fallback_adapter,
                ..Default::default()
            })
            .await
            .map_err(|error| WgpuRenderError::Device(error.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nara_render"),
                ..Default::default()
            })
            .await
            .map_err(|error| WgpuRenderError::Device(error.to_string()))?;

        let lost = Arc::new(AtomicBool::new(false));
        let loss_signal = Arc::clone(&lost);
        device.set_device_lost_callback(move |_, _| loss_signal.store(true, Ordering::Release));
        let errors = Arc::new(Mutex::new((0_u64, String::new())));
        let error_signal = Arc::clone(&errors);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            let mut latest = error_signal
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            latest.0 = latest.0.wrapping_add(1);
            latest.1 = error.to_string();
        }));

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            errors: Arc::new(ErrorSource::Owned(errors)),
            lost,
        })
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    pub(super) fn loss_signal(&self) -> &Arc<AtomicBool> {
        &self.lost
    }

    pub(super) fn check_signal(&self, cursor: &mut u64) -> Result<(), WgpuRenderError> {
        if self.is_lost() {
            return Err(WgpuRenderError::DeviceLost);
        }
        let error = match self.errors.as_ref() {
            ErrorSource::Owned(latest) => {
                let latest = latest.lock().unwrap_or_else(|error| error.into_inner());
                (latest.0 != *cursor).then(|| {
                    *cursor = latest.0;
                    latest.1.clone()
                })
            }
            ErrorSource::Shared { source, .. } => source.observe_error(cursor),
        };
        error.map_or(Ok(()), |error| Err(WgpuRenderError::Device(error)))
    }
}

/// Uses a validation scope only for application-supplied shader code and its pipeline.
#[cfg(feature = "sprite-render")]
pub(super) fn validate_custom_pipeline<T>(
    device: &wgpu::Device,
    create: impl FnOnce() -> T,
) -> Result<T, WgpuRenderError> {
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = create();
    let mut context = Context::from_waker(Waker::noop());
    match pin!(scope.pop()).poll(&mut context) {
        Poll::Ready(None) => Ok(result),
        Poll::Ready(Some(error)) => Err(WgpuRenderError::Validation(error.to_string())),
        Poll::Pending => Err(WgpuRenderError::Validation(
            "shader or pipeline validation is still pending".into(),
        )),
    }
}
