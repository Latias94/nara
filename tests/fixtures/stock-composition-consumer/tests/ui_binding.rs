#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use nara::render::{Extent2d, RenderImage2d, RenderImagePacket, RenderTarget, ViewportRect};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDeclaration,
    WgpuFeatureDefinition, WgpuFeatureInstance, WgpuPolicyBindingContext, WgpuPolicyBuildContext,
    WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramTextureDescriptor,
    WgpuRenderHost, WgpuRenderHostError, WgpuRenderPolicy, WgpuRenderProgramBuilder,
    WgpuRenderProgramError, WgpuUiCapture, WgpuUiFeature, WgpuUiProgramBinding,
};
use nara_stock_composition_consumer::{oracle, readback};
use nara_ui_render::UiPaintPacket;

#[path = "support/hud.rs"]
mod hud;
#[path = "support/scene.rs"]
#[allow(dead_code)]
mod scene;

struct FinalUi;

#[derive(Clone, Copy, Debug)]
enum Wiring {
    Original,
    ForeignKey,
    ForeignInstance,
    EqualButDifferentCapture,
    OtherFinalVersion,
}

struct Capture {
    packet: Arc<UiPaintPacket>,
    wiring: Wiring,
}

struct UiPolicy {
    ui: WgpuFeatureDeclaration<WgpuUiFeature<FinalUi>>,
}

impl WgpuRenderPolicy for UiPolicy {
    type Capture = Capture;
    type Bindings = (
        Arc<WgpuFeatureInstance<WgpuUiFeature<FinalUi>>>,
        Arc<WgpuFeatureInstance<WgpuUiFeature<FinalUi>>>,
    );

    fn bind(
        &self,
        _: &Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        // A second declaration produces a different CPU instance with the same Rust type.
        let foreign = WgpuFeatureDefinition::new(WgpuUiFeature::new()).declare();
        Ok((context.feature(&self.ui)?, context.feature(&foreign)?))
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            GraphLimits::default(),
        )?;
        let extent = context.packet().target().extent();
        let descriptor = WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("real-final-ui-binding"),
                size: wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: context.format(),
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            transfer: if context.format().is_srgb() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        };
        let ui_target = builder.texture::<FinalUi>(descriptor.clone())?;
        let viewport = ViewportRect::from_extent(extent).unwrap();
        let logical = capture.packet.target();
        let original = Arc::new(WgpuUiCapture::new(
            &[Arc::clone(&capture.packet)],
            logical,
            None,
            ui_target.clone(),
            viewport,
        )?);
        let bound_capture = if matches!(capture.wiring, Wiring::EqualButDifferentCapture) {
            Arc::new(WgpuUiCapture::new(
                &[Arc::clone(&capture.packet)],
                logical,
                None,
                ui_target.clone(),
                viewport,
            )?)
        } else {
            Arc::clone(&original)
        };
        builder.operation(
            7,
            &bindings.0,
            original,
            &[],
            &[ui_target.clear(wgpu::Color::BLUE, AccessRole::Attachment)],
        )?;
        let other = builder.texture::<FinalUi>(descriptor)?;
        // Identical copied pixels still do not make this the original UI operation's target.
        builder.initialize(
            8,
            &[other.preserve(&ui_target, AccessRole::CopyDestination)],
        )?;
        let binding = WgpuUiProgramBinding::new(
            if matches!(capture.wiring, Wiring::ForeignKey) {
                8
            } else {
                7
            },
            if matches!(capture.wiring, Wiring::ForeignInstance) {
                &bindings.1
            } else {
                &bindings.0
            },
            bound_capture,
        );
        let final_target = if matches!(capture.wiring, Wiring::OtherFinalVersion) {
            &other
        } else {
            &ui_target
        };
        WgpuPolicyProgram::new(
            builder.finish(&[ui_target.output(), other.output()])?,
            final_target,
            WgpuPolicyFrameFacts::default(),
        )?
        .with_ui(binding)
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; exercises the actual UI Feature and binding"]
fn real_final_ui_binding_rejects_foreign_key_instance_capture_and_equal_pixel_successor() {
    readback::require_native_selection();
    let source = scene::capture(false);
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    for wiring in [
        Wiring::Original,
        Wiring::ForeignKey,
        Wiring::ForeignInstance,
        Wiring::EqualButDifferentCapture,
        Wiring::OtherFinalVersion,
    ] {
        let producer = host.create_producer().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(scene::WIDTH, scene::HEIGHT).unwrap(),
        )
        .unwrap();
        let installation = WgpuPolicyDefinition::new(UiPolicy {
            ui: WgpuFeatureDefinition::new(WgpuUiFeature::new()).declare(),
        })
        .install()
        .unwrap();
        let input = installation
            .capture(
                source.packet().clone(),
                Arc::new(Capture {
                    packet: hud::packet(RenderTarget::PrimaryWindow, scene::WIDTH, scene::HEIGHT),
                    wiring,
                }),
            )
            .unwrap()
            .with_image_target(target)
            .unwrap();
        let reservation = host
            .reserve_image_target(&producer, input.packet().source_generation(), target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        match (wiring, host.prepare(&producer, &input)) {
            (Wiring::Original, Ok(candidate)) => {
                let frame = host
                    .submit_candidate(&producer, candidate)
                    .unwrap_or_else(|failure| panic!("{:?}", failure.error));
                let output = frame.final_texture::<FinalUi>().unwrap();
                let actual = readback::read(&mut host, &output);
                let mut expected = oracle::Image {
                    width: scene::WIDTH,
                    height: scene::HEIGHT,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    pixels: vec![[0, 0, 255, 255]; (scene::WIDTH * scene::HEIGHT) as usize],
                };
                expected = expected.hud();
                expected.assert_matches(&actual, 0, "actual original UI operation");
            }
            (Wiring::Original, Err(error)) => panic!("original UI binding rejected: {error}"),
            (_, Err(WgpuRenderHostError::Program(WgpuRenderProgramError::Feature { message }))) => {
                let expected = match wiring {
                    Wiring::ForeignKey => "original Feature operation key",
                    Wiring::ForeignInstance => "original Feature instance",
                    Wiring::EqualButDifferentCapture => "original typed capture Arc",
                    Wiring::OtherFinalVersion => "exact final texture",
                    Wiring::Original => unreachable!(),
                };
                assert!(message.contains(expected), "{wiring:?}: {message}");
            }
            (_, Err(error)) => panic!("{wiring:?} failed for an unrelated reason: {error}"),
            (_, Ok(_)) => panic!("{wiring:?} incorrectly accepted an unrelated real UI binding"),
        }
        host.retire_producer(&producer);
    }
    readback::finish(&mut host);
}
