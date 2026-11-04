//! Shared native preparation under the original Host's allocation budget and submission custody.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    WgpuNativeResources, WgpuRenderError,
    allocation::create_checked_buffer_with_data,
    output::{OutputBudget, OutputReservation},
    submission::{SubmissionCapacity, WgpuSubmissionReservation},
};

#[derive(Debug, Error)]
pub(crate) enum NativePreparationError {
    #[error("native render-resource reservations are at capacity")]
    ResourceCapacity,
    #[error("native submission-resource reservations are at capacity")]
    SubmissionCapacity,
    #[error(transparent)]
    Backend(#[from] WgpuRenderError),
}

impl From<NativePreparationError> for WgpuRenderError {
    fn from(error: NativePreparationError) -> Self {
        match error {
            NativePreparationError::ResourceCapacity => Self::UploadDeferred {
                message: "native render-resource reservations are at capacity".into(),
            },
            NativePreparationError::SubmissionCapacity => Self::ContentRejected {
                message: "native submission-resource reservations are at capacity".into(),
            },
            NativePreparationError::Backend(error) => error,
        }
    }
}

/// Shared retained-byte accounting for an adapter-private native allocation.
///
/// Reserve before allocating and keep a clone alongside the actual allocation in Feature state
/// or prepared work. Clones share one charge. The original submission also retains the guard,
/// including when preparation fails after an upload.
#[derive(Clone, Debug)]
pub struct WgpuFeatureAllocation {
    _reservation: Arc<OutputReservation>,
}

/// The exact native pair, byte budget and original custody used by every native preparation path.
///
/// This helper cannot submit or present. Only its upload methods access the Queue, after admitting
/// the destination into the original reservation. The Host must bind that reservation on failure
/// as well as success, since an earlier upload may already have been accepted.
pub(crate) struct NativePreparation<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    budget: &'a Arc<OutputBudget>,
    reservation: &'a mut WgpuSubmissionReservation,
    work: &'a mut crate::WgpuNativeWorkStats,
}

impl<'a> NativePreparation<'a> {
    #[cfg(test)]
    pub(crate) fn work_stats(&self) -> crate::WgpuNativeWorkStats {
        *self.work
    }

    #[cfg(test)]
    pub(crate) fn reserve_entries(&mut self, count: usize) -> Result<(), NativePreparationError> {
        self.reservation
            .reserve_entries(count)
            .map_err(|error| match error {
                SubmissionCapacity::Limit => NativePreparationError::SubmissionCapacity,
                SubmissionCapacity::Allocation => NativePreparationError::ResourceCapacity,
            })
    }
    pub(crate) fn new(
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        budget: &'a Arc<OutputBudget>,
        reservation: &'a mut WgpuSubmissionReservation,
        work: &'a mut crate::WgpuNativeWorkStats,
    ) -> Self {
        Self {
            device,
            queue,
            budget,
            reservation,
            work,
        }
    }

    pub(crate) fn device(&self) -> &'a wgpu::Device {
        self.device
    }

    pub(crate) fn reserve_bytes(
        &mut self,
        bytes: u64,
    ) -> Result<WgpuFeatureAllocation, NativePreparationError> {
        let allocation = WgpuFeatureAllocation {
            _reservation: Arc::new(
                self.budget
                    .reserve(bytes)
                    .ok_or(NativePreparationError::ResourceCapacity)?,
            ),
        };
        self.retain(allocation.clone())?;
        Ok(allocation)
    }

    pub(crate) fn retain<T: WgpuNativeResources>(
        &mut self,
        value: T,
    ) -> Result<(), NativePreparationError> {
        self.reservation.retain(value).map_err(|(error, value)| {
            // Refused resources must leave the custody lock before their destructors run.
            drop(value);
            match error {
                SubmissionCapacity::Limit => NativePreparationError::SubmissionCapacity,
                SubmissionCapacity::Allocation => NativePreparationError::ResourceCapacity,
            }
        })
    }

    pub(crate) fn upload_buffer(
        &mut self,
        destination: &wgpu::Buffer,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativePreparationError> {
        self.retain(destination.clone())?;
        self.queue.write_buffer(destination, offset, data);
        self.work.issued_buffer_write_calls = self.work.issued_buffer_write_calls.saturating_add(1);
        self.work.issued_buffer_write_bytes = self
            .work
            .issued_buffer_write_bytes
            .saturating_add(data.len() as u64);
        Ok(())
    }

    pub(crate) fn upload_texture(
        &mut self,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), NativePreparationError> {
        self.retain(destination.texture.clone())?;
        self.queue.write_texture(destination, data, layout, size);
        self.work.issued_texture_write_calls =
            self.work.issued_texture_write_calls.saturating_add(1);
        self.work.issued_texture_write_bytes = self
            .work
            .issued_texture_write_bytes
            .saturating_add(data.len() as u64);
        Ok(())
    }

    /// Reserves the actual aligned allocation before creating and initializing its mapping.
    pub(crate) fn create_buffer_with_data(
        &mut self,
        label: &'static str,
        contents: &[u8],
        usage: wgpu::BufferUsages,
    ) -> Result<(wgpu::Buffer, WgpuFeatureAllocation), NativePreparationError> {
        let size = u64::try_from(contents.len())
            .ok()
            .and_then(|size| size.checked_add(wgpu::COPY_BUFFER_ALIGNMENT - 1))
            .map(|size| size & !(wgpu::COPY_BUFFER_ALIGNMENT - 1))
            .ok_or(NativePreparationError::ResourceCapacity)?;
        let allocation = self.reserve_bytes(size)?;
        let buffer = create_checked_buffer_with_data(self.device, label, contents, usage)?;
        if !contents.is_empty() {
            self.work.initialized_buffer_calls =
                self.work.initialized_buffer_calls.saturating_add(1);
            self.work.initialized_buffer_payload_bytes = self
                .work
                .initialized_buffer_payload_bytes
                .saturating_add(contents.len() as u64);
            self.work.initialized_buffer_storage_bytes = self
                .work
                .initialized_buffer_storage_bytes
                .saturating_add(size);
        }
        debug_assert_eq!(buffer.size(), size);
        self.retain((buffer.clone(), allocation.clone()))?;
        Ok((buffer, allocation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_allocation_clones_share_one_charge_until_the_last_use_releases() {
        let budget = Arc::new(OutputBudget::new(64));
        let allocation = WgpuFeatureAllocation {
            _reservation: Arc::new(budget.reserve(64).unwrap()),
        };
        let retained_use = allocation.clone();
        assert_eq!(budget.retained(), 64);
        assert!(budget.reserve(1).is_none());
        drop(allocation);
        assert_eq!(budget.retained(), 64);
        drop(retained_use);
        assert_eq!(budget.retained(), 0);
    }

    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    #[test]
    fn mapped_buffer_storage_is_reserved_before_allocation_and_held_until_completion() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _, _| {
            let budget = Arc::new(OutputBudget::new(4));
            let mut reservation = WgpuSubmissionReservation::new(device, queue, ());
            let mut work = crate::WgpuNativeWorkStats::default();
            let mut preparation =
                NativePreparation::new(device, queue, &budget, &mut reservation, &mut work);
            let failure =
                crate::allocation::AllocationFailureGuard::at("preparation-aligned-buffer");
            assert!(matches!(
                preparation.create_buffer_with_data(
                    "preparation-aligned-buffer",
                    &[7; 5],
                    wgpu::BufferUsages::VERTEX,
                ),
                Err(NativePreparationError::ResourceCapacity)
            ));
            assert!(
                !failure.was_consumed(),
                "budget refusal must precede native allocation"
            );
            assert_eq!(*preparation.work, crate::WgpuNativeWorkStats::default());
            assert_eq!(budget.retained(), 0);
            drop(failure);
            let (buffer, allocation) = preparation
                .create_buffer_with_data(
                    "preparation-aligned-buffer",
                    &[7; 3],
                    wgpu::BufferUsages::VERTEX,
                )
                .unwrap();
            assert_eq!(buffer.size(), 4);
            assert_eq!(preparation.work.initialized_buffer_calls, 1);
            assert_eq!(preparation.work.initialized_buffer_payload_bytes, 3);
            assert_eq!(preparation.work.initialized_buffer_storage_bytes, 4);
            assert_eq!(budget.retained(), buffer.size());
            drop(buffer);
            drop(allocation);
            assert_eq!(
                budget.retained(),
                4,
                "the original submission still owns the allocation"
            );
            let index = queue.submit([]);
            let prefix = reservation.submitted(index.clone());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            assert!(prefix.poll().unwrap());
            assert_eq!(budget.retained(), 0);
            Ok(())
        })
        .unwrap();
    }

    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    #[test]
    fn submission_capacity_refuses_uploads_before_the_queue_can_observe_them() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _, _| {
            let budget = Arc::new(OutputBudget::new(4));
            let mut reservation = WgpuSubmissionReservation::new(device, queue, ());
            for _ in 1..crate::submission::MAX_SUBMISSION_RESOURCES {
                assert!(reservation.retain(()).is_ok());
            }
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("preparation-refused-buffer"),
                size: 4,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("preparation-refused-texture"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut work = crate::WgpuNativeWorkStats::default();
            let mut preparation =
                NativePreparation::new(device, queue, &budget, &mut reservation, &mut work);
            // Both writes are invalid if they reach WGPU. Custody refusal must happen first.
            assert!(matches!(
                preparation.upload_buffer(&buffer, 0, &[7]),
                Err(NativePreparationError::SubmissionCapacity)
            ));
            assert!(matches!(
                preparation.upload_texture(
                    texture.as_image_copy(),
                    &[7; 4],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(8),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: 2,
                        height: 1,
                        depth_or_array_layers: 1
                    },
                ),
                Err(NativePreparationError::SubmissionCapacity)
            ));
            assert!(matches!(
                preparation.reserve_bytes(4),
                Err(NativePreparationError::SubmissionCapacity)
            ));
            assert_eq!(
                budget.retained(),
                0,
                "a refused guard must refund its charge"
            );
            assert!(pollster::block_on(validation.pop()).is_none());
            assert_eq!(work, crate::WgpuNativeWorkStats::default());
            let index = queue.submit([]);
            let prefix = reservation.submitted(index.clone());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            assert!(prefix.poll().unwrap());
            Ok(())
        })
        .unwrap();
    }

    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    #[test]
    fn mapped_initialization_is_counted_when_later_retention_refuses() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _, _| {
            let budget = Arc::new(OutputBudget::new(4));
            let mut reservation = WgpuSubmissionReservation::new(device, queue, ());
            for _ in 1..crate::submission::MAX_SUBMISSION_RESOURCES - 1 {
                reservation.retain(()).unwrap();
            }
            let mut work = crate::WgpuNativeWorkStats::default();
            let mut preparation =
                NativePreparation::new(device, queue, &budget, &mut reservation, &mut work);
            assert!(matches!(
                preparation.create_buffer_with_data(
                    "initialized-before-retention-refusal",
                    &[7; 3],
                    wgpu::BufferUsages::VERTEX
                ),
                Err(NativePreparationError::SubmissionCapacity)
            ));
            assert_eq!(
                work,
                crate::WgpuNativeWorkStats {
                    initialized_buffer_calls: 1,
                    initialized_buffer_payload_bytes: 3,
                    initialized_buffer_storage_bytes: 4,
                    ..Default::default()
                }
            );
            assert_eq!(budget.retained(), 4);
            let index = queue.submit([]);
            let prefix = reservation.submitted(index.clone());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            assert!(prefix.poll().unwrap());
            assert_eq!(budget.retained(), 0);
            assert_eq!(work.initialized_buffer_payload_bytes, 3);
            Ok(())
        })
        .unwrap();
    }

    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    #[test]
    fn returned_empty_and_invalid_writes_count_requests_without_claiming_transfer() {
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _, _| {
            let budget = Arc::new(OutputBudget::new(4));
            let mut reservation = WgpuSubmissionReservation::new(device, queue, ());
            let mut work = crate::WgpuNativeWorkStats::default();
            let mut preparation =
                NativePreparation::new(device, queue, &budget, &mut reservation, &mut work);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("issued-write-buffer"),
                size: 4,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            preparation.upload_buffer(&buffer, 0, &[]).unwrap();
            assert!(pollster::block_on(validation.pop()).is_none());
            assert_eq!(preparation.work.issued_buffer_write_calls, 1);
            assert_eq!(preparation.work.issued_buffer_write_bytes, 0);

            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            preparation.upload_buffer(&buffer, 0, &[3]).unwrap();
            assert!(pollster::block_on(validation.pop()).is_some());
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("issued-write-texture"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            preparation
                .upload_texture(
                    texture.as_image_copy(),
                    &[7; 4],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(8),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: 2,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                )
                .unwrap();
            assert!(pollster::block_on(validation.pop()).is_some());
            assert_eq!(
                work,
                crate::WgpuNativeWorkStats {
                    issued_buffer_write_calls: 2,
                    issued_buffer_write_bytes: 1,
                    issued_texture_write_calls: 1,
                    issued_texture_write_bytes: 4,
                    ..Default::default()
                }
            );
            let index = queue.submit([]);
            let prefix = reservation.submitted(index.clone());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            assert!(prefix.poll().unwrap());
            assert_eq!(work.issued_buffer_write_bytes, 1);
            Ok(())
        })
        .unwrap();
    }
}
