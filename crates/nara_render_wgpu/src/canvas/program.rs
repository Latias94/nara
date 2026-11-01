//! Typed graph adapter for the same nearest composite used by the transition cache.

use std::{marker::PhantomData, sync::Arc};

use nara_render::WorldCanvasFrame;

use super::{
    CANVAS_MAPPING_BYTES, CanvasCompositeMapping, CanvasCompositePipeline, WORLD_CANVAS_FORMAT,
};
use crate::{
    TextureTransfer, WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeaturePreflightContext,
    WgpuFeaturePrepareContext, WgpuProgramTexture, WgpuRenderFeature, WgpuRenderProgramError,
};

pub(crate) struct CanvasCompositeFeature<Source: 'static, Dest: 'static>(
    PhantomData<fn() -> (Source, Dest)>,
);

impl<Source: 'static, Dest: 'static> CanvasCompositeFeature<Source, Dest> {
    pub(crate) const fn new() -> Self {
        Self(PhantomData)
    }
}

/// Exact graph versions and immutable backend-neutral mapping facts; no hidden world raster.
/// The caller declares a sampled source read and an initialized attachment destination.
pub(crate) struct CanvasCompositeCapture<Source: 'static, Dest: 'static> {
    pub(crate) source: WgpuProgramTexture<Source>,
    pub(crate) target: WgpuProgramTexture<Dest>,
    pub(crate) frame: WorldCanvasFrame,
}

impl<Source: 'static, Dest: 'static> CanvasCompositeCapture<Source, Dest> {
    pub(crate) fn new(
        source: WgpuProgramTexture<Source>,
        target: WgpuProgramTexture<Dest>,
        frame: WorldCanvasFrame,
    ) -> Self {
        Self {
            source,
            target,
            frame,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), WgpuRenderProgramError> {
        let source = self.source.descriptor();
        let destination = self.target.descriptor();
        let extent = self.frame.extent();
        if source.native.size.width != extent.width
            || source.native.size.height != extent.height
            || source.native.format != WORLD_CANVAS_FORMAT
            || source.transfer != TextureTransfer::Srgb
        {
            return Err(WgpuRenderProgramError::feature(
                "world canvas source must match the captured raster extent and sRGB format",
            ));
        }
        let mapping = self.frame.mapping();
        let physical = mapping
            .panel_rect()
            .max
            .map(|edge| (edge * mapping.pixels_per_point()).round() as u32);
        if [
            destination.native.size.width,
            destination.native.size.height,
        ] != physical
        {
            return Err(WgpuRenderProgramError::feature(
                "world canvas destination must match the captured physical target extent",
            ));
        }
        let transfer = if destination.native.format.is_srgb() {
            TextureTransfer::Srgb
        } else {
            TextureTransfer::Linear
        };
        if !matches!(
            destination.native.format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(WgpuRenderProgramError::feature(
                "world canvas destination must preserve the supported RGBA color and alpha channels",
            ));
        }
        if destination.transfer != transfer || source.alpha != destination.alpha {
            return Err(WgpuRenderProgramError::feature(
                "world canvas composite must preserve alpha and use the native color transfer",
            ));
        }
        Ok(())
    }
}

/// NativeFeatureStates owns this cache for the original instance and device epoch.
#[derive(Default)]
pub(crate) struct CanvasCompositeState {
    pipeline: Option<Arc<CanvasCompositePipeline>>,
}

pub(crate) struct CanvasCompositePrepared<Dest: 'static> {
    target: WgpuProgramTexture<Dest>,
    pipeline: Arc<CanvasCompositePipeline>,
    mapping: CanvasCompositeMapping,
    _mapping_allocation: WgpuFeatureAllocation,
}

impl<Source: 'static, Dest: 'static> WgpuRenderFeature for CanvasCompositeFeature<Source, Dest> {
    type Capture = CanvasCompositeCapture<Source, Dest>;
    type State = CanvasCompositeState;
    type Prepared = CanvasCompositePrepared<Dest>;
    type Report = ();

    fn preflight(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        capture.validate()?;
        let metadata = std::mem::size_of::<CanvasCompositePipeline>()
            + std::mem::size_of::<crate::output::OutputReservation>()
            + 4 * std::mem::size_of::<usize>()
            + crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
                WgpuFeatureAllocation,
            >()
            + crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<(
                wgpu::Buffer,
                WgpuFeatureAllocation,
            )>();
        context.add_native_cost(2, metadata)
    }

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(CanvasCompositeState::default())
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let resources = context.resources();
        let source = resources.texture(&capture.source)?;
        // Verify both exact graph ports before allocating adapter-private native resources.
        resources.texture(&capture.target)?;
        let format = capture.target.descriptor().native.format;
        let pipeline = match state
            .pipeline
            .as_ref()
            .filter(|pipeline| pipeline.destination_format == format)
        {
            Some(pipeline) => Arc::clone(pipeline),
            None => Arc::new(CanvasCompositePipeline::create(context.device(), format)?),
        };
        let allocation = context.reserve_bytes(CANVAS_MAPPING_BYTES)?;
        let mapping = pipeline.prepare_mapping(context.device(), source, capture.frame)?;
        // The actual checked, mapped buffer and its shared guard survive the original prefix,
        // including any later preparation or encoding failure.
        context.retain((mapping.buffer.clone(), allocation.clone()))?;
        state.pipeline = Some(Arc::clone(&pipeline));
        Ok(CanvasCompositePrepared {
            target: capture.target.clone(),
            pipeline,
            mapping,
            _mapping_allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let destination = context.resources().texture(&prepared.target)?;
        // Graph initialization owns the full background, including bars and empty coverage.
        prepared.pipeline.encode_composite(
            &prepared.mapping,
            context.encoder(),
            destination,
            wgpu::LoadOp::Load,
        );
        Ok(())
    }
}
