//! Complete captured UI geometry and automatic native preparation costs.

use std::{
    alloc::Layout,
    mem::{size_of, size_of_val},
};

use super::{
    UiCapture, WgpuLoweredUiPacketFacts,
    mesh::{MeshPipeline, MeshVertex, UiDraw},
};
use crate::{
    GraphError, LimitKind, WgpuFeatureAllocation, WgpuFeaturePreflightContext,
    WgpuRenderProgramError, output::OutputReservation, quad::WgpuQuadBatch,
    submission::WgpuSubmissionReservation,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct UiPreparationCost {
    pub(super) custody_entries: usize,
    pub(super) metadata_bytes: usize,
    pub(super) vertex_bytes: u64,
    pub(super) index_bytes: u64,
}

pub(super) fn preflight<S: 'static>(
    capture: &UiCapture<S>,
    context: &mut WgpuFeaturePreflightContext<'_>,
) -> Result<(), WgpuRenderProgramError> {
    let cost = UiPreparationCost::for_capture(capture)?;
    for required in [cost.vertex_bytes, cost.index_bytes] {
        if required > context.limits().max_buffer_size {
            return Err(WgpuRenderProgramError::DeviceResourceLimit {
                name: "max_buffer_size",
                required,
                enabled: context.limits().max_buffer_size,
            });
        }
    }
    context.add_native_cost(cost.custody_entries, cost.metadata_bytes)
}

impl UiPreparationCost {
    /// Counts the capture's Arc allocation, exact Arc slice layouts and Vec capacities,
    /// native buffer reservation records and pipeline wrappers. Shared references held by
    /// capture, prepared work, reports and policy bindings do not repeat an allocation's charge.
    /// Original packet/frame storage and image/glyph source payloads remain source-owned and
    /// excluded, as do allocator rounding, BTree node overhead and native driver allocations.
    /// Native buffer payloads are separately charged by the original preparation's byte budget.
    pub(super) fn for_capture<S: 'static>(
        capture: &UiCapture<S>,
    ) -> Result<Self, WgpuRenderProgramError> {
        let overflow = || GraphError::LimitExceeded(LimitKind::MetadataBytes);
        let mut metadata_bytes = arc_allocation_bytes::<UiCapture<S>>(1)?;
        for bytes in [
            arc_allocation_bytes::<WgpuLoweredUiPacketFacts>(capture.packet_facts.len())?,
            arc_allocation_bytes::<WgpuQuadBatch>(capture.batches.len())?,
            arc_allocation_bytes::<UiDraw>(capture.draws.len())?,
            arc_allocation_bytes::<MeshVertex>(capture.mesh_vertices.len())?,
            arc_allocation_bytes::<u32>(capture.mesh_indices.len())?,
        ] {
            metadata_bytes = metadata_bytes.checked_add(bytes).ok_or_else(overflow)?;
        }
        for batch in capture.batches.iter() {
            let bytes = batch
                .instances
                .capacity()
                .checked_mul(size_of::<crate::quad::WgpuQuadInstance>())
                .ok_or_else(overflow)?;
            metadata_bytes = metadata_bytes.checked_add(bytes).ok_or_else(overflow)?;
        }
        for facts in capture.packet_facts.iter() {
            let bytes = facts
                .resources
                .len()
                .checked_mul(size_of::<crate::quad::WgpuQuadMaterialKey>())
                .ok_or_else(overflow)?;
            metadata_bytes = metadata_bytes.checked_add(bytes).ok_or_else(overflow)?;
        }
        let vertex_bytes =
            u64::try_from(size_of_val(capture.mesh_vertices.as_ref())).map_err(|_| overflow())?;
        let index_bytes =
            u64::try_from(size_of_val(capture.mesh_indices.as_ref())).map_err(|_| overflow())?;
        let mut custody_entries = 0;
        if index_bytes != 0 {
            custody_entries = 4;
            metadata_bytes = metadata_bytes
                .checked_add(arc_allocation_bytes::<MeshPipeline>(1)?)
                .ok_or_else(overflow)?;
            let mut buffer_record = arc_allocation_bytes::<OutputReservation>(1)?;
            for bytes in [
                WgpuSubmissionReservation::modeled_retain_bytes::<WgpuFeatureAllocation>(),
                WgpuSubmissionReservation::modeled_retain_bytes::<(
                    wgpu::Buffer,
                    WgpuFeatureAllocation,
                )>(),
            ] {
                buffer_record = buffer_record.checked_add(bytes).ok_or_else(overflow)?;
            }
            metadata_bytes = buffer_record
                .checked_mul(2)
                .and_then(|bytes| bytes.checked_add(metadata_bytes))
                .ok_or_else(overflow)?;
        }
        Ok(Self {
            custody_entries,
            metadata_bytes,
            vertex_bytes,
            index_bytes,
        })
    }
}

/// One element models a sized Arc; a slice uses its exact captured element count.
fn arc_allocation_bytes<T>(len: usize) -> Result<usize, WgpuRenderProgramError> {
    let overflow = || GraphError::LimitExceeded(LimitKind::MetadataBytes);
    let payload = Layout::array::<T>(len).map_err(|_| overflow())?;
    let (allocation, _) = Layout::new::<[usize; 2]>()
        .extend(payload)
        .map_err(|_| overflow())?;
    Ok(allocation.pad_to_align().size())
}
