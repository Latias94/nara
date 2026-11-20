#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use nara::render::{Extent2d, RenderImage2d, RenderImagePacket, RenderTarget, ViewportRect};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureInstance, WgpuPolicyBindingContext,
    WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyProgram,
    WgpuProgramTextureDescriptor, WgpuRenderHost, WgpuRenderHostError, WgpuRenderPolicy,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, WgpuUiCapture, WgpuUiFeature,
    WgpuUiProgramBinding,
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
            interpretation: TextureInterpretation::Color {
                transfer: if context.format().has_srgb_suffix() {
                    TextureTransfer::Srgb
                } else {
                    TextureTransfer::Linear
                },
                alpha: TextureAlpha::Premultiplied,
            },
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

#[test]
#[ignore = "requires an explicitly selected native GPU for stock UI pixels"]
fn snapped_button_hit_edges_match_the_original_rendered_pixels() {
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiInteraction, UiKey,
        UiLayer, UiLocalKey, UiNode, UiPoint, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme,
        UiVisual,
    };
    use nara_ui_render::{UiPreparedImages, UiPresentationPlanner};

    let root = UiRootKey::runtime("consumer.stock-composition", "hit-edges").unwrap();
    let foreground = UiLocalKey::runtime("consumer.stock-composition", "foreground").unwrap();
    let background = UiLocalKey::runtime("consumer.stock-composition", "background").unwrap();
    let front_key = UiKey::scoped(root.clone(), foreground.clone()).unwrap();
    let back_key = UiKey::scoped(root.clone(), background.clone()).unwrap();
    let (width, height) = (scene::WIDTH, scene::HEIGHT);
    let extent = Extent2d::new(width, height).unwrap();
    let source = scene::capture(false);
    let mut compiler =
        UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut planner = UiPresentationPlanner::default();
    let installation = WgpuPolicyDefinition::new(UiPolicy {
        ui: WgpuFeatureDefinition::new(WgpuUiFeature::new()).declare(),
    })
    .install()
    .unwrap();
    readback::require_native_selection();
    let mut host = WgpuRenderHost::offscreen();
    let mut aligned: Option<oracle::Image> = None;
    for (revision, x) in [(1, 20.0), (2, 20.4)] {
        let authored = UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill()).with_children([
                    UiNode::new(UiStyle::absolute(0.0, 0.0, width as f32, height as f32))
                        .with_key(background.clone())
                        .with_interaction(UiInteraction::hit_target())
                        .with_visual(UiVisual::solid(UiColor::rgba(0.0, 0.0, 1.0, 1.0))),
                    UiNode::new(UiStyle::absolute(x, 3.0, 5.0, 6.0))
                        .with_key(foreground.clone())
                        .with_interaction(UiInteraction::hit_target())
                        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0))),
                ]),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            width as f32,
            height as f32,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let frame = compiler
            .compile(&authored, environment, UiTheme::default())
            .unwrap();
        let packet = planner
            .plan(
                Arc::clone(&frame),
                RenderTarget::PrimaryWindow,
                &UiPreparedImages::new(),
            )
            .unwrap();
        host.begin_frame(revision).unwrap();
        let producer = host.create_producer().unwrap();
        let target =
            RenderImagePacket::new(RenderImage2d::allocate_handle().unwrap(), extent).unwrap();
        let input = installation
            .capture(
                source.packet().clone(),
                Arc::new(Capture {
                    packet,
                    wiring: Wiring::Original,
                }),
            )
            .unwrap()
            .with_image_target(target)
            .unwrap();
        let reservation = host
            .reserve_image_target(&producer, input.packet().source_generation(), target)
            .unwrap();
        host.activate_image_target(&producer, reservation).unwrap();
        let candidate = host.prepare(&producer, &input).unwrap();
        let submitted = host
            .submit_candidate(&producer, candidate)
            .unwrap_or_else(|failure| panic!("{:?}", failure.error));
        let actual = readback::read(&mut host, &submitted.final_texture::<FinalUi>().unwrap());
        host.retire_producer(&producer);
        for cursor_x in [19, 20, 24, 25] {
            let (target, color) = if (20..25).contains(&cursor_x) {
                (&front_key, [255, 0, 0, 255])
            } else {
                (&back_key, [0, 0, 255, 255])
            };
            assert_eq!(actual.pixels[(6 * width + cursor_x) as usize], color);
            assert_eq!(
                frame
                    .hit_test(UiPoint::new(cursor_x as f32, 6.0))
                    .unwrap()
                    .key(),
                target
            );
        }
        if let Some(original) = &aligned {
            original.assert_matches(&actual, 0, "aligned and fractional origins");
        } else {
            aligned = Some(actual);
        }
    }
    readback::finish(&mut host);
}
