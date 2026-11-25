use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::{NativePreparation, NativePreparationError, WgpuFeatureAllocation};
use crate::gpu::{WgpuRenderError, allocation::checked_resource};

const MAX_SLOTS: usize = 8;
const MAX_CACHED_SLOT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct BufferStorage {
    destination: wgpu::Buffer,
    staging: wgpu::Buffer,
    allocation: WgpuFeatureAllocation,
}

#[derive(Debug)]
struct Slot {
    storage: BufferStorage,
    complete: Arc<AtomicBool>,
    mapped: Arc<AtomicBool>,
}

/// Reuses bounded staging and device-local buffers across original Host submissions.
///
/// A slot becomes writable after the original Host prefix settles and its staging map returns.
/// Growth and spills proceed under the same allocation budget.
#[derive(Debug, Default)]
pub(crate) struct FrameRingBuffer {
    slots: Vec<Slot>,
}

/// Tracks the free slots seen by preflight without borrowing native resources or reserving a slot.
pub(crate) struct FrameRingAvailability {
    sizes: [u64; MAX_SLOTS],
    vacant: usize,
}

impl FrameRingAvailability {
    pub(crate) fn allocation_bytes(&mut self, required: u64) -> u64 {
        if required == 0 {
            return 0;
        }
        if let Some(index) = self.best_fit(required) {
            self.sizes[index] = 0;
            return 0;
        }
        if required <= MAX_CACHED_SLOT_BYTES {
            if self.vacant != 0 {
                self.vacant -= 1;
            } else if let Some(index) = self.best_fit(1) {
                self.sizes[index] = 0;
            }
        }
        required.saturating_mul(2)
    }

    fn best_fit(&self, required: u64) -> Option<usize> {
        self.sizes
            .iter()
            .enumerate()
            .filter(|(_, size)| **size >= required)
            .min_by_key(|(_, size)| **size)
            .map(|(index, _)| index)
    }
}

impl FrameRingBuffer {
    pub(crate) const fn modeled_slot_bytes() -> usize {
        4 * (std::mem::size_of::<Slot>() + std::mem::size_of::<(wgpu::Buffer, Arc<AtomicBool>)>())
            + std::mem::size_of::<wgpu::Buffer>()
            + std::mem::size_of::<[usize; 3]>()
    }

    pub(crate) fn clear(&mut self) {
        self.slots.clear();
    }

    pub(crate) fn available(&self, usage: wgpu::BufferUsages) -> FrameRingAvailability {
        let mut sizes = [0; MAX_SLOTS];
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        for (size, slot) in sizes.iter_mut().zip(&self.slots) {
            if slot.storage.destination.usage() == usage
                && slot.complete.load(Ordering::Acquire)
                && slot.mapped.load(Ordering::Acquire)
            {
                *size = slot.storage.destination.size();
            }
        }
        FrameRingAvailability {
            sizes,
            vacant: MAX_SLOTS - self.slots.len(),
        }
    }

    pub(crate) fn upload(
        &mut self,
        preparation: &mut NativePreparation<'_>,
        label: &'static str,
        usage: wgpu::BufferUsages,
        size: u64,
        fill: impl FnOnce(&mut wgpu::BufferViewMut),
    ) -> Result<(wgpu::Buffer, WgpuFeatureAllocation), NativePreparationError> {
        let size = wgpu::BufferSize::new(size)
            .filter(|size| size.get().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT))
            .ok_or(NativePreparationError::ResourceCapacity)?;
        let available = self.available(usage);
        let reusable = available.best_fit(size.get());
        let storage = if let Some(index) = reusable {
            self.slots[index].storage.clone()
        } else {
            let bytes = size
                .get()
                .checked_mul(2)
                .ok_or(NativePreparationError::ResourceCapacity)?;
            let allocation = preparation.reserve_bytes(bytes)?;
            let device = preparation.device();
            let mut create = |label, usage, mapped_at_creation| {
                checked_resource(device, label, || {
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(label),
                        size: size.get(),
                        usage,
                        mapped_at_creation,
                    });
                    preparation.work.record_buffer_creation(size.get());
                    buffer
                })
            };
            let destination = create(label, usage | wgpu::BufferUsages::COPY_DST, false)?;
            let staging = create(
                "nara_frame_upload_staging",
                wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                true,
            )?;
            BufferStorage {
                destination,
                staging,
                allocation,
            }
        };
        preparation.retain(storage.clone())?;
        {
            let mut view = storage
                .staging
                .get_mapped_range_mut(0..size.get())
                .map_err(|error| WgpuRenderError::UploadDeferred {
                    message: format!("{label}: could not map a frame upload buffer: {error}"),
                })?;
            fill(&mut view);
        }
        let slot = reusable.or_else(|| {
            if size.get() > MAX_CACHED_SLOT_BYTES {
                return None;
            }
            if self.slots.len() < MAX_SLOTS {
                return self.slots.try_reserve(1).ok().map(|_| self.slots.len());
            }
            self.slots
                .iter()
                .position(|slot| {
                    slot.complete.load(Ordering::Acquire) && !slot.mapped.load(Ordering::Acquire)
                })
                .or_else(|| available.best_fit(1))
        });
        let mapped = slot.map(|_| {
            reusable.map_or_else(
                || Arc::new(AtomicBool::new(true)),
                |index| Arc::clone(&self.slots[index].mapped),
            )
        });
        if let (Some(index), Some(mapped)) = (slot, &mapped) {
            let entry = Slot {
                storage: storage.clone(),
                complete: Arc::clone(preparation.reservation.observe().gpu_complete()),
                mapped: Arc::clone(mapped),
            };
            if index == self.slots.len() {
                self.slots.push(entry);
            } else {
                self.slots[index] = entry;
            }
        }
        preparation
            .reservation
            .stage_frame_upload(&storage.staging, &storage.destination, size.get(), mapped)
            .map_err(|_| NativePreparationError::ResourceCapacity)?;
        preparation.record_buffer_write(size.get());
        Ok((storage.destination, storage.allocation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::readback_tests::ReadbackGpu;

    #[test]
    fn failed_original_prefix_controls_reuse_of_exposed_slots() {
        let gpu = ReadbackGpu::new();
        let mut ring = FrameRingBuffer::default();
        let usage = wgpu::BufferUsages::VERTEX;
        let (rejected, prefix) = gpu.with_preparation::<()>(|preparation| {
            let first = ring.upload(preparation, "test.frame_ring", usage, 4, |write| {
                write.copy_from_slice(&[1; 4]);
            })?;
            let second = ring.upload(preparation, "test.frame_ring", usage, 4, |write| {
                write.copy_from_slice(&[2; 4]);
            })?;
            assert_ne!(first.0, second.0);
            assert_eq!(ring.available(usage).allocation_bytes(4), 8);
            assert_eq!(preparation.work_stats().created_buffer_calls, 4);
            Err(WgpuRenderError::ContentRejected {
                message: "reject after frame uploads".to_owned(),
            })
        });
        assert!(rejected.is_err());
        gpu.wait_for_prefix(&prefix);

        let mut available = ring.available(usage);
        assert_eq!(available.allocation_bytes(4), 0);
        assert_eq!(available.allocation_bytes(4), 0);
        assert_eq!(available.allocation_bytes(4), 8);
        let (prepared, prefix) = gpu.with_preparation(|preparation| {
            ring.upload(preparation, "test.frame_ring", usage, 4, |write| {
                write.copy_from_slice(&[3; 4]);
            })?;
            assert_eq!(preparation.work_stats().created_buffer_calls, 0);
            assert_eq!(preparation.work_stats().issued_buffer_write_bytes, 4);
            ring.clear();
            assert_eq!(gpu.budget.retained(), 8);
            Ok(((), None))
        });
        prepared.unwrap();
        gpu.wait_for_prefix(&prefix);
        assert_eq!(gpu.budget.retained(), 0);
    }
}
