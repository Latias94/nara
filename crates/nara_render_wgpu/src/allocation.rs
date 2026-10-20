#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::texture::WgpuTextureError;

/// Checks a single resource operation before its returned handle can be consumed.
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
pub(crate) fn checked_allocation<T>(
    device: &wgpu::Device,
    label: &'static str,
    create: impl FnOnce() -> T,
) -> Result<T, WgpuTextureError> {
    checked_resource(device, label, create).map_err(|error| match error {
        crate::WgpuRenderError::UploadDeferred { message } => {
            WgpuTextureError::AllocationFailed { label, message }
        }
        error => WgpuTextureError::ResourceRejected {
            label,
            message: error.to_string(),
        },
    })
}

/// Finite allocation checks also serve targets that have no sprite or UI submitter.
pub(crate) fn checked_resource<T>(
    device: &wgpu::Device,
    label: &'static str,
    create: impl FnOnce() -> T,
) -> Result<T, crate::WgpuRenderError> {
    let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let resource = create();
    let validation_error = pollster::block_on(validation_scope.pop());
    let allocation_error =
        pollster::block_on(allocation_scope.pop()).map(|error| error.to_string());
    #[cfg(test)]
    let allocation_error = allocation_error.or_else(|| take_test_failure(label));
    // OOM can also invalidate a returned handle. Keep it retryable even when validation fails.
    if let Some(message) = allocation_error {
        return Err(crate::WgpuRenderError::UploadDeferred {
            message: format!("{label}: {message}"),
        });
    }
    if let Some(error) = validation_error {
        return Err(crate::WgpuRenderError::ContentRejected {
            message: format!("{label}: {error}"),
        });
    }
    Ok(resource)
}

/// Unlike DeviceExt::create_buffer_init, allocation is checked before accessing the mapping.
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
pub(crate) fn create_buffer_with_data(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> Result<wgpu::Buffer, WgpuTextureError> {
    create_checked_buffer_with_data(device, label, contents, usage).map_err(|error| match error {
        crate::WgpuRenderError::UploadDeferred { message } => {
            WgpuTextureError::AllocationFailed { label, message }
        }
        error => WgpuTextureError::ResourceRejected {
            label,
            message: error.to_string(),
        },
    })
}

/// Checks allocation before accessing the initial mapping, including base-backend uniforms.
pub(crate) fn create_checked_buffer_with_data(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> Result<wgpu::Buffer, crate::WgpuRenderError> {
    let size = u64::try_from(contents.len())
        .ok()
        .and_then(|size| size.checked_add(wgpu::COPY_BUFFER_ALIGNMENT - 1))
        .map(|size| size & !(wgpu::COPY_BUFFER_ALIGNMENT - 1))
        .ok_or_else(|| crate::WgpuRenderError::ContentRejected {
            message: format!("{label}: buffer size cannot be represented"),
        })?;
    let buffer = checked_resource(device, label, || {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: !contents.is_empty(),
        })
    })?;
    if !contents.is_empty() {
        {
            let mut mapping = buffer.get_mapped_range_mut(..).map_err(|error| {
                crate::WgpuRenderError::ContentRejected {
                    message: format!("{label}: initial buffer mapping failed: {error}"),
                }
            })?;
            mapping.slice(..contents.len()).copy_from_slice(contents);
        }
        buffer.unmap();
    }
    Ok(buffer)
}

#[cfg(test)]
std::thread_local! {
    static ALLOCATION_FAILURE: std::cell::Cell<Option<&'static str>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn take_test_failure(label: &'static str) -> Option<String> {
    ALLOCATION_FAILURE.with(|failure| {
        if failure.get() == Some(label) {
            failure.set(None);
            Some("injected allocation failure".to_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
pub(crate) struct AllocationFailureGuard;

#[cfg(test)]
impl AllocationFailureGuard {
    pub(crate) fn at(label: &'static str) -> Self {
        ALLOCATION_FAILURE.with(|failure| {
            assert!(failure.replace(Some(label)).is_none());
        });
        Self
    }

    pub(crate) fn was_consumed(&self) -> bool {
        ALLOCATION_FAILURE.with(|failure| failure.get().is_none())
    }
}

#[cfg(test)]
impl Drop for AllocationFailureGuard {
    fn drop(&mut self) {
        ALLOCATION_FAILURE.with(|failure| failure.set(None));
    }
}

#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
mod tests;

#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
pub(crate) fn take_test_batches() -> Option<Vec<crate::quad::WgpuQuadBatch>> {
    tests::take_batches()
}
