use std::sync::mpsc::{self, Receiver, Sender};

use super::{WgpuNativeWorkStats, WgpuRenderError};

const MAX_SLOTS: usize = 24;
const MAX_CACHED_SIZE: u64 = 8 * 1024 * 1024;

struct Upload {
    staging: wgpu::Buffer,
    destination: wgpu::Buffer,
}

/// Reuses mapped staging and device-local buffers after WGPU returns their maps.
pub(super) struct FrameRingBuffer {
    returned: Receiver<Option<Upload>>,
    return_slot: Sender<Option<Upload>>,
    idle: Vec<Upload>,
    recording: Vec<Upload>,
    mapping: usize,
}

impl Default for FrameRingBuffer {
    fn default() -> Self {
        let (return_slot, returned) = mpsc::channel();
        Self {
            returned,
            return_slot,
            idle: Vec::new(),
            recording: Vec::new(),
            mapping: 0,
        }
    }
}

impl FrameRingBuffer {
    pub(super) fn begin_frame(&mut self) {
        for upload in self.returned.try_iter() {
            self.mapping -= 1;
            self.idle.extend(upload);
        }
    }

    /// Call after the encoder was submitted or dropped. WGPU maps after the last GPU use.
    pub(super) fn end_frame(&mut self) {
        for upload in self.recording.drain(..) {
            let return_slot = self.return_slot.clone();
            let staging = upload.staging.clone();
            self.mapping += 1;
            staging.map_async(wgpu::MapMode::Write, .., move |result| {
                let _ = return_slot.send(result.is_ok().then_some(upload));
            });
        }
    }

    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        work: &mut WgpuNativeWorkStats,
        label: &'static str,
        usage: wgpu::BufferUsages,
        bytes: &[u8],
    ) -> Result<wgpu::Buffer, WgpuRenderError> {
        let length = bytes.len() as u64;
        if length == 0 || !length.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT) {
            return Err(WgpuRenderError::Validation(format!(
                "{label}: upload must be nonempty and aligned to four bytes"
            )));
        }
        let limit = device.limits().max_buffer_size;
        if length > limit {
            return Err(WgpuRenderError::Capacity(label));
        }
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        let best = self
            .idle
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.destination.usage() == usage && slot.destination.size() >= length
            })
            .min_by_key(|(_, slot)| slot.destination.size())
            .map(|(index, _)| index);
        let (slot, cache) = if let Some(index) = best {
            (self.idle.swap_remove(index), true)
        } else {
            let mut cache = length <= MAX_CACHED_SIZE;
            if cache && self.idle.len() + self.recording.len() + self.mapping == MAX_SLOTS {
                cache = self.idle.pop().is_some();
            }
            let size = if cache {
                length.next_power_of_two().min(limit)
            } else {
                length
            };
            let create = |label, usage, mapped_at_creation| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation,
                })
            };
            let destination = create(label, usage, false);
            let staging = create(
                "nara_frame_upload",
                wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                true,
            );
            work.created_buffer_calls += 2;
            (
                Upload {
                    staging,
                    destination,
                },
                cache,
            )
        };
        {
            let mut mapping = slot.staging.get_mapped_range_mut(..).map_err(|error| {
                WgpuRenderError::Validation(format!("{label}: frame upload map: {error}"))
            })?;
            mapping.slice(..bytes.len()).copy_from_slice(bytes);
        }
        slot.staging.unmap();
        encoder.copy_buffer_to_buffer(&slot.staging, 0, &slot.destination, 0, length);
        work.issued_buffer_write_calls += 1;
        let destination = slot.destination.clone();
        if cache {
            self.recording.push(slot);
        }
        Ok(destination)
    }
}
