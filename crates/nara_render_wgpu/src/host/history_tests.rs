//! The actual Host owns publication; the state-machine tests alone are not native evidence.

use super::*;
use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuEmptyHistoryReason,
    WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeaturePrepareContext, WgpuHistoryLineage, WgpuHistoryTexture, WgpuPolicyBindingContext,
    WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyFrameFacts, WgpuPolicyProgram,
    WgpuProgramTextureDescriptor, WgpuRenderFeature, WgpuRenderPolicy, WgpuRenderProgramBuilder,
    WgpuRenderProgramError,
};
use nara_core::Mat3;
use nara_ecs::Entity;
use nara_render::{
    Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, RenderViewId, ViewLoadOp,
    ViewMapping2d, ViewportRect, build_image_render_frame_packet,
};

struct HistoryColor;
struct FinalColor;
struct IdentityOnlyFeature;

impl WgpuRenderFeature for IdentityOnlyFeature {
    type Capture = ();
    type State = ();
    type Prepared = ();
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        panic!("history declaration does not request a native Feature operation")
    }
    fn prepare(
        &self,
        _: &mut (),
        _: &(),
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        unreachable!()
    }
    fn encode(
        &self,
        _: &(),
        _: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        unreachable!()
    }
}

struct TemporalPolicy;
struct TemporalCapture {
    lineage: WgpuHistoryLineage,
    effects: [(
        crate::WgpuHistoryView,
        WgpuFeatureDeclaration<IdentityOnlyFeature>,
    ); 4],
    observations: Arc<std::sync::Mutex<Vec<(Option<u64>, Option<WgpuEmptyHistoryReason>)>>>,
}

impl WgpuRenderPolicy for TemporalPolicy {
    type Capture = TemporalCapture;
    type Bindings = Vec<WgpuHistoryTexture<HistoryColor>>;

    fn history_lineage(&self, capture: &Self::Capture) -> Option<WgpuHistoryLineage> {
        Some(capture.lineage.clone())
    }

    fn bind(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        capture
            .effects
            .iter()
            .map(|(view, effect)| context.history_texture(view, effect, 0, history_descriptor()))
            .collect()
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut builder = WgpuRenderProgramBuilder::new(
            context.packet().source_generation(),
            context.packet().frame_index(),
            GraphLimits::default(),
        )?;
        let mut outputs = Vec::new();
        let mut history = Vec::new();
        for (index, slot) in bindings.iter().enumerate() {
            capture.observations.lock().unwrap().push((
                context
                    .previous_texture(slot)?
                    .map(|previous| previous.frame_index()),
                context.empty_texture_reason(slot)?,
            ));
            let output = builder.texture::<HistoryColor>(history_descriptor())?;
            builder.initialize(
                index as u64,
                &[output.clear(wgpu::Color::WHITE, AccessRole::Attachment)],
            )?;
            outputs.push(output.output());
            history.push(slot.bind(&output));
        }
        let final_texture = builder.texture::<FinalColor>(WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("history-host-final"),
                size: wgpu::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: context.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        })?;
        builder.initialize(
            9,
            &[final_texture.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )?;
        outputs.push(final_texture.output());
        Ok(WgpuPolicyProgram::new(
            builder.finish(&outputs)?,
            &final_texture,
            WgpuPolicyFrameFacts::default(),
        )?
        .with_history(history))
    }
}

fn history_descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("history-host-slot"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

struct Fixture {
    capture: Arc<TemporalCapture>,
    installation: crate::WgpuPolicyInstallation<TemporalPolicy>,
    target: RenderImagePacket,
    views: ExtractedViews,
}

impl Fixture {
    fn new() -> Self {
        let source = RenderFrameSourceGeneration::try_acquire().unwrap();
        let lineage = WgpuHistoryLineage::new(source).unwrap();
        let first = RenderViewId::try_acquire().unwrap();
        let second = RenderViewId::try_acquire().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(2, 2).unwrap(),
        )
        .unwrap();
        let mut views = ExtractedViews::default();
        for (index, id) in [first, second].into_iter().enumerate() {
            let viewport = ViewportRect::new(0, 0, 2, 2).unwrap();
            views.push(ExtractedView {
                id,
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::Image(target.image()),
                viewport,
                mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
                order: index as i32,
                load: if index == 0 {
                    ViewLoadOp::Clear(Color::BLACK)
                } else {
                    ViewLoadOp::Load
                },
            });
        }
        let definition = WgpuFeatureDefinition::new(IdentityOnlyFeature);
        let first = crate::WgpuHistoryView::new(first);
        let second = crate::WgpuHistoryView::new(second);
        Self {
            capture: Arc::new(TemporalCapture {
                lineage,
                effects: [
                    (first.clone(), definition.declare()),
                    (first, definition.declare()),
                    (second.clone(), definition.declare()),
                    (second, definition.declare()),
                ],
                observations: Arc::default(),
            }),
            installation: WgpuPolicyDefinition::new(TemporalPolicy).install().unwrap(),
            target,
            views,
        }
    }

    fn input(&self, frame: u64) -> crate::WgpuRenderInput {
        let packet = build_image_render_frame_packet(
            self.capture.lineage.source_generation(),
            frame,
            &self.views,
            self.target,
        )
        .unwrap()
        .unwrap();
        self.installation
            .capture(packet, Arc::clone(&self.capture))
            .unwrap()
    }

    fn activate(&self, host: &mut WgpuRenderHost, producer: &WgpuRenderProducer) {
        let target = host
            .reserve_image_target(
                producer,
                self.capture.lineage.source_generation(),
                self.target,
            )
            .unwrap();
        host.activate_image_target(producer, target).unwrap();
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_history_publication_is_coherent_and_failed_attempt_retries_the_same_candidate() {
    let fixture = Fixture::new();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    fixture.activate(&mut host, &producer);
    let input = fixture.input(1);
    let candidate = host.prepare(&producer, &input).unwrap();
    let sibling = host.prepare(&producer, &input).unwrap();
    let original_packet = std::ptr::from_ref(candidate.input().packet());
    let bytes = host.retained_resource_bytes();
    host = host.with_resource_byte_limit(bytes);
    let refused = host.submit_candidate(&producer, candidate).unwrap_err();
    assert!(refused.is_retryable());
    assert_eq!(
        std::ptr::from_ref(refused.candidate.input().packet()),
        original_packet
    );
    assert!(
        host.views[&producer.id()]
            .admission
            .last_frame_index
            .is_none()
    );
    host = host.with_resource_byte_limit(512 * 1024 * 1024);
    let published = host.submit_candidate(&producer, refused.candidate).unwrap();
    for (view, effect) in &fixture.capture.effects {
        assert_eq!(
            published
                .history_texture::<HistoryColor, _>(view.id(), effect, 0)
                .unwrap()
                .frame_index(),
            1
        );
    }
    assert!(host.submit_candidate(&producer, sibling).is_err());
    let next = host.prepare(&producer, &fixture.input(2)).unwrap();
    assert!(
        fixture
            .capture
            .observations
            .lock()
            .unwrap()
            .iter()
            .rev()
            .take(4)
            .all(|(frame, reason)| *frame == Some(1) && reason.is_none())
    );
    let (view, effect) = &fixture.capture.effects[0];
    host.reset_history(
        &producer,
        &fixture.capture.lineage,
        &[crate::WgpuHistoryReset::texture::<HistoryColor, _>(
            view,
            effect,
            0,
            history_descriptor(),
        )],
    )
    .unwrap();
    assert!(matches!(
        host.submit_candidate(&producer, next).unwrap_err().error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::StaleHistory)
    ));
    let fresh = host.prepare(&producer, &fixture.input(2)).unwrap();
    let tail = fixture.capture.observations.lock().unwrap();
    assert_eq!(
        tail[tail.len() - 4],
        (None, Some(WgpuEmptyHistoryReason::Reset))
    );
    assert!(
        tail[tail.len() - 3..]
            .iter()
            .all(|(frame, _)| *frame == Some(1))
    );
    drop(tail);
    host.submit_candidate(&producer, fresh).unwrap();
    assert!(host.prepare(&producer, &input).is_err());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; unavailable hardware fails the prerequisite"]
fn native_history_actual_submission_failure_keeps_original_basis_and_loss_starts_explicit_reset() {
    let fixture = Fixture::new();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.begin_frame(1).unwrap();
    fixture.activate(&mut host, &producer);
    let first = host.prepare(&producer, &fixture.input(1)).unwrap();
    let published = host.submit_candidate(&producer, first).unwrap();
    let second_input = fixture.input(2);
    let second = host.prepare(&producer, &second_input).unwrap();
    PROGRAM_SUBMISSION_FAILURE.with(|failure| failure.set(true));
    let failed = host.submit_candidate(&producer, second).unwrap_err();
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    let original = std::ptr::from_ref(failed.candidate.input().packet());
    let second = host.submit_candidate(&producer, failed.candidate).unwrap();
    assert_eq!(second.image().receipt().render_transaction_frame_index(), 2);
    assert_eq!(original, std::ptr::from_ref(second_input.packet()));
    let device = host
        .with_image(published.image(), |device, _, _| device.clone())
        .unwrap();
    device.destroy();
    device.poll(wgpu::PollType::Poll).unwrap();
    let mut replacement = WgpuRenderHost::offscreen();
    let replacement_producer = replacement.create_producer().unwrap();
    replacement.begin_frame(1).unwrap();
    fixture.activate(&mut replacement, &replacement_producer);
    let reset = replacement
        .prepare(&replacement_producer, &second_input)
        .unwrap();
    assert!(
        fixture
            .capture
            .observations
            .lock()
            .unwrap()
            .iter()
            .rev()
            .take(4)
            .all(|(frame, reason)| frame.is_none()
                && *reason == Some(WgpuEmptyHistoryReason::DeviceRecreated))
    );
    replacement
        .submit_candidate(&replacement_producer, reset)
        .unwrap();
}
