//! The conformance frontend's captured RGBA image, uploaded by the original GPU Host.

use std::sync::Arc;

use nara_render::Extent2d;
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureAllocation,
    WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureEncodeContext, WgpuFeatureInstance,
    WgpuFeaturePrepareContext, WgpuPolicyBindingContext, WgpuPolicyBuildContext,
    WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramTexture, WgpuProgramTextureDescriptor,
    WgpuRenderFeature, WgpuRenderPolicy, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

pub struct CpuImage {
    pub extent: Extent2d,
    pub pixels: Arc<[u8]>,
}

pub struct CpuImagePolicy {
    upload: WgpuFeatureDeclaration<Upload>,
}

impl Default for CpuImagePolicy {
    fn default() -> Self {
        Self {
            upload: WgpuFeatureDefinition::new(Upload).declare(),
        }
    }
}

impl WgpuRenderPolicy for CpuImagePolicy {
    type Capture = CpuImage;
    type Bindings = Arc<WgpuFeatureInstance<Upload>>;

    fn bind(
        &self,
        _: &CpuImage,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        context.feature(&self.upload)
    }

    fn build(
        &self,
        upload: &Self::Bindings,
        capture: &CpuImage,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        if context.packet().target().extent() != capture.extent
            || context.format() != wgpu::TextureFormat::Rgba8Unorm
            || capture.extent.width > 2048
            || capture.extent.height > 2048
        {
            return Err(WgpuRenderProgramError::feature(
                "the conformance frontend needs one bounded RGBA8 image",
            ));
        }
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            GraphLimits {
                max_resources: 1,
                max_operations: 1,
                max_accesses: 1,
                max_outputs: 1,
                max_texture_dimension_2d: 2048,
                max_modeled_resource_bytes: 2048 * 2048 * 4,
                max_modeled_metadata_bytes: 1024 * 1024,
                ..GraphLimits::default()
            },
        )?;
        let destination = builder.texture::<PanelColor>(WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("independent-panel-cpu-image"),
                size: extent(capture.extent),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: context.format(),
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        })?;
        builder.operation(
            1,
            upload,
            Arc::new(UploadCapture {
                destination: destination.clone(),
                extent: capture.extent,
                pixels: Arc::clone(&capture.pixels),
            }),
            &[],
            &[destination.clear(wgpu::Color::TRANSPARENT, AccessRole::CopyDestination)],
        )?;
        WgpuPolicyProgram::new(
            builder.finish(&[destination.output()])?,
            &destination,
            WgpuPolicyFrameFacts::default(),
        )
    }
}

fn extent(value: Extent2d) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: value.width,
        height: value.height,
        depth_or_array_layers: 1,
    }
}

pub struct PanelColor;
pub struct Upload;
pub struct UploadCapture {
    destination: WgpuProgramTexture<PanelColor>,
    extent: Extent2d,
    pixels: Arc<[u8]>,
}
pub struct Uploaded {
    destination: WgpuProgramTexture<PanelColor>,
    staging: wgpu::Texture,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for Upload {
    type Capture = UploadCapture;
    type State = ();
    type Prepared = Uploaded;
    type Report = ();

    fn preflight(
        &self,
        capture: &UploadCapture,
        _: &mut nara_render_wgpu::WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let expected = u64::from(capture.extent.width) * u64::from(capture.extent.height) * 4;
        if u64::try_from(capture.pixels.len()).ok() != Some(expected) {
            return Err(WgpuRenderProgramError::feature(
                "the conformance image needs complete RGBA pixels",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _: &mut (),
        capture: &UploadCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Uploaded, WgpuRenderProgramError> {
        let allocation =
            context
                .reserve_bytes(u64::try_from(capture.pixels.len()).map_err(|_| {
                    WgpuRenderProgramError::feature("image byte count exceeds u64")
                })?)?;
        let staging = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("independent-panel-staging-image"),
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            ..capture.destination.descriptor().native.clone()
        });
        context.upload_texture(
            staging.as_image_copy(),
            &capture.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(capture.extent.width * 4),
                rows_per_image: Some(capture.extent.height),
            },
            extent(capture.extent),
        )?;
        Ok(Uploaded {
            destination: capture.destination.clone(),
            staging,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &Uploaded,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let destination = resources.texture_native(&prepared.destination)?;
        encoder.copy_texture_to_texture(
            prepared.staging.as_image_copy(),
            destination.as_image_copy(),
            destination.size(),
        );
        Ok(())
    }
}
