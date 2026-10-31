//! Exact UI presentation observations attached to one original policy operation.

use std::{any::Any, mem::size_of, sync::Arc};

use crate::{
    WgpuFeatureInstance, WgpuProgramOutputs, WgpuRenderProgram, WgpuRenderProgramError,
    graph::ResourceVersion,
    ui::{UiCapture, UiFeature, UiReport, WgpuLoweredUiPacketFacts},
};

/// Retains the original UI operation, Feature instance and immutable typed capture.
/// Attaching this binding validates all three and the exact final texture before native work.
/// Packet observations alone do not authorize completion of a presentation permit.
pub struct WgpuUiProgramBinding {
    operation_key: u64,
    instance: Arc<dyn Any + Send + Sync>,
    capture: Arc<dyn UiCaptureBinding>,
}

impl std::fmt::Debug for WgpuUiProgramBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuUiProgramBinding")
            .field("operation_key", &self.operation_key)
            .field("packet_count", &self.facts().len())
            .finish_non_exhaustive()
    }
}

impl WgpuUiProgramBinding {
    /// Binds the exact capture Arc passed to the original UI Feature operation.
    /// Equal packet contents or a second instance of the same Feature do not match.
    #[must_use]
    pub fn new<S: 'static>(
        operation_key: u64,
        instance: &WgpuFeatureInstance<UiFeature<S>>,
        capture: Arc<UiCapture<S>>,
    ) -> Self {
        let identity = Arc::clone(&instance.identity);
        Self {
            operation_key,
            instance: identity,
            capture,
        }
    }

    pub(crate) fn validate(
        &self,
        program: &WgpuRenderProgram,
        final_version: &ResourceVersion,
    ) -> Result<(), WgpuRenderProgramError> {
        let operation = program
            .0
            .operations
            .iter()
            .find(|operation| operation.key == self.operation_key)
            .and_then(|operation| operation.capture.as_ref())
            .ok_or_else(|| {
                WgpuRenderProgramError::feature(
                    "the UI binding requires its original Feature operation key",
                )
            })?;
        let identity = Arc::clone(operation.identity());
        let identity: Arc<dyn Any + Send + Sync> = identity;
        if !Arc::ptr_eq(&self.instance, &identity) {
            return Err(WgpuRenderProgramError::feature(
                "the UI binding requires its original Feature instance",
            ));
        }
        if !self.capture.matches(operation.capture_value()) {
            return Err(WgpuRenderProgramError::feature(
                "the UI binding requires its original typed capture Arc",
            ));
        }
        if self.capture.target_version() != final_version {
            return Err(WgpuRenderProgramError::feature(
                "the UI binding must draw into the policy's exact final texture",
            ));
        }
        Ok(())
    }

    /// Captured observations for admission and capacity planning, before native execution.
    pub(crate) fn facts(&self) -> &[WgpuLoweredUiPacketFacts] {
        self.capture.packet_facts()
    }

    /// Borrows actual encode observations only from this binding's original submitted operation.
    /// The success path allocates no storage and returns facts retained by the admitted outputs.
    pub(crate) fn submitted_facts<'a>(
        &self,
        outputs: &'a WgpuProgramOutputs,
    ) -> Result<&'a [WgpuLoweredUiPacketFacts], WgpuRenderProgramError> {
        self.submitted_report_facts(outputs.reports.iter().map(|report| {
            let instance: Arc<dyn Any + Send + Sync> = report.instance.clone();
            (report.key, instance, report.report.as_any())
        }))
    }

    // Match the CPU report observations independently of native prefix creation.
    fn submitted_report_facts<'a>(
        &self,
        reports: impl IntoIterator<Item = (u64, Arc<dyn Any + Send + Sync>, &'a dyn Any)>,
    ) -> Result<&'a [WgpuLoweredUiPacketFacts], WgpuRenderProgramError> {
        let (_, _, report) = reports
            .into_iter()
            .find(|(key, instance, _)| {
                *key == self.operation_key && Arc::ptr_eq(&self.instance, instance)
            })
            .ok_or_else(|| {
                WgpuRenderProgramError::feature(
                    "the UI binding requires its submitted Feature operation and instance",
                )
            })?;
        let report = report.downcast_ref::<UiReport>().ok_or_else(|| {
            WgpuRenderProgramError::feature("the UI binding requires its original typed UI report")
        })?;
        if !Arc::ptr_eq(self.capture.packet_facts(), &report.packet_facts) {
            return Err(WgpuRenderProgramError::feature(
                "the UI report must retain the original capture's packet facts",
            ));
        }
        Ok(&report.packet_facts)
    }

    /// Capture observations only; successful presentation uses the submitted report instead.
    pub(crate) fn payload_facts(&self) -> crate::frame::WgpuUiPayloadFacts {
        let mut facts = crate::frame::WgpuUiPayloadFacts::default();
        for packet in self.facts() {
            facts.packet_count = facts.packet_count.saturating_add(1);
            facts.packet_identity = (facts.packet_count == 1).then_some(packet.packet);
            facts.primitives = facts.primitives.saturating_add(packet.primitive_count);
            facts.glyph_primitives = facts
                .glyph_primitives
                .saturating_add(packet.glyph_primitive_count);
            facts.glyph_pages = facts.glyph_pages.saturating_add(packet.glyph_page_count);
        }
        facts
    }

    /// Exact fixed wrapper only. Both Arcs reuse their original allocations.
    pub(crate) fn metadata_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

trait UiCaptureBinding: Send + Sync {
    fn matches(&self, capture: &dyn Any) -> bool;
    fn target_version(&self) -> &ResourceVersion;
    fn packet_facts(&self) -> &Arc<[WgpuLoweredUiPacketFacts]>;
}

impl<S: 'static> UiCaptureBinding for UiCapture<S> {
    fn matches(&self, capture: &dyn Any) -> bool {
        capture
            .downcast_ref::<Self>()
            .is_some_and(|capture| std::ptr::eq(self, capture))
    }

    fn target_version(&self) -> &ResourceVersion {
        &self.target.version
    }

    fn packet_facts(&self) -> &Arc<[WgpuLoweredUiPacketFacts]> {
        &self.packet_facts
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use nara_render::{RenderFrameSourceGeneration, RenderTarget, ViewportRect};
    use nara_text::bundled_font_face;
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
        UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
    };
    use nara_ui_render::UiPaintPacket;
    use nara_window::WindowId;

    use super::*;
    use crate::{
        AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
        WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderProducerId,
        WgpuRenderProgramBuilder,
    };

    // Semantic markers require no Send/Sync bounds even though the capture is retained.
    type Composite = std::rc::Rc<()>;
    const UI_OPERATION: u64 = 7;

    #[test]
    fn binding_accepts_exact_operation_instance_and_capture() {
        let instance = instance();
        let (builder, target, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, Arc::clone(&capture));
        let program = finish(builder, &target, &instance, Arc::clone(&capture));

        binding.validate(&program, &target.version).unwrap();
        assert!(std::ptr::eq(binding.facts(), capture.packet_facts.as_ref(),));
        assert_eq!(binding.facts().len(), 1);
        assert_eq!(binding.facts()[0].target, RenderTarget::PrimaryWindow);
        assert_eq!(
            capture.packets().collect::<Vec<_>>(),
            [(binding.facts()[0].packet, RenderTarget::PrimaryWindow)],
        );
        assert_eq!(binding.metadata_bytes(), size_of::<WgpuUiProgramBinding>());
    }

    #[test]
    fn binding_rejects_foreign_operation_key() {
        let instance = instance();
        let (builder, target, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION + 1, &instance, Arc::clone(&capture));
        let program = finish(builder, &target, &instance, capture);

        assert!(binding.validate(&program, &target.version).is_err());
    }

    #[test]
    fn binding_rejects_initialization_without_a_feature_capture() {
        let instance = instance();
        let (mut builder, target, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, capture);
        builder
            .initialize(
                UI_OPERATION,
                &[target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        let program = builder.finish(&[target.output()]).unwrap();

        assert!(binding.validate(&program, &target.version).is_err());
    }

    #[test]
    fn binding_rejects_another_instance_with_the_same_typed_capture() {
        let definition = WgpuFeatureDefinition::new(UiFeature::<Composite>::new());
        let closed = Arc::new(AtomicBool::new(false));
        let make_instance = || {
            WgpuFeatureInstance::new(
                1,
                WgpuRenderProducerId::new(1),
                Arc::clone(&closed),
                &definition,
            )
            .unwrap()
        };
        let instance = make_instance();
        let foreign_instance = make_instance();
        let (builder, target, capture) = candidate();
        let binding =
            WgpuUiProgramBinding::new(UI_OPERATION, &foreign_instance, Arc::clone(&capture));
        let program = finish(builder, &target, &instance, capture);

        assert!(binding.validate(&program, &target.version).is_err());
    }

    #[test]
    fn binding_rejects_same_instance_with_identical_but_distinct_capture() {
        let instance = instance();
        let (builder, target, capture) = candidate();
        let replacement = Arc::new(UiCapture {
            target: capture.target.clone(),
            viewport: capture.viewport,
            batches: Arc::clone(&capture.batches),
            packet_facts: Arc::clone(&capture.packet_facts),
        });
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, replacement);
        let program = finish(builder, &target, &instance, capture);

        assert!(binding.validate(&program, &target.version).is_err());
    }

    #[test]
    fn binding_rejects_red_ui_on_scratch_when_the_final_image_only_clears_black() {
        let instance = instance();
        let (mut builder, ui_target, capture) = candidate();
        let final_color = builder
            .texture::<Composite>(ui_target.descriptor().clone())
            .unwrap();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, Arc::clone(&capture));
        assert!(!capture.batches.is_empty());
        builder
            .operation(
                UI_OPERATION,
                &instance,
                capture,
                &[],
                &[ui_target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        builder
            .initialize(
                UI_OPERATION + 1,
                &[final_color.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )
            .unwrap();
        let program = builder.finish(&[final_color.output()]).unwrap();

        assert!(binding.validate(&program, &final_color.version).is_err());
        let policy = crate::WgpuPolicyProgram::new(
            program,
            &final_color,
            crate::WgpuPolicyFrameFacts::default(),
        )
        .unwrap();
        assert!(policy.with_ui(binding).is_err());
    }

    #[test]
    fn submitted_facts_borrow_the_exact_report_after_matching_operation_and_instance() {
        let instance = instance();
        let foreign_instance = self::instance();
        let (_, _, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, Arc::clone(&capture));
        let report = UiReport {
            packet_facts: Arc::clone(&capture.packet_facts),
            ..Default::default()
        };
        let facts = binding
            .submitted_report_facts([
                report_observation(UI_OPERATION + 1, &instance, &report),
                report_observation(UI_OPERATION, &foreign_instance, &report),
                report_observation(UI_OPERATION, &instance, &report),
            ])
            .unwrap();

        // The returned borrow belongs to the report, independently of the binding's lifetime.
        drop(binding);
        assert!(std::ptr::eq(facts, report.packet_facts.as_ref()));
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].packet, capture.packet_facts[0].packet);
    }

    #[test]
    fn submitted_facts_reject_missing_or_foreign_operation_and_instance() {
        let instance = instance();
        let foreign_instance = self::instance();
        let (_, _, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, Arc::clone(&capture));
        let report = UiReport {
            packet_facts: Arc::clone(&capture.packet_facts),
            ..Default::default()
        };

        assert!(binding.submitted_report_facts(std::iter::empty()).is_err());
        for (key, instance) in [
            (UI_OPERATION + 1, &instance),
            (UI_OPERATION, &foreign_instance),
        ] {
            assert!(
                binding
                    .submitted_report_facts([report_observation(key, instance, &report)])
                    .is_err()
            );
        }
    }

    #[test]
    fn submitted_facts_reject_another_report_type_for_the_exact_operation_and_instance() {
        let instance = instance();
        let (_, _, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, capture);

        assert!(
            binding
                .submitted_report_facts([report_observation(UI_OPERATION, &instance, &())])
                .is_err()
        );
    }

    #[test]
    fn submitted_facts_reject_identical_packet_facts_from_another_allocation() {
        let instance = instance();
        let (_, _, capture) = candidate();
        let binding = WgpuUiProgramBinding::new(UI_OPERATION, &instance, Arc::clone(&capture));
        let report = UiReport {
            packet_facts: capture.packet_facts.as_ref().to_vec().into(),
            ..Default::default()
        };
        assert_eq!(
            capture.packets().collect::<Vec<_>>(),
            report.packets().collect::<Vec<_>>(),
        );
        assert!(!Arc::ptr_eq(&capture.packet_facts, &report.packet_facts));

        assert!(
            binding
                .submitted_report_facts([report_observation(UI_OPERATION, &instance, &report)])
                .is_err()
        );
    }

    fn report_observation<'a>(
        key: u64,
        instance: &WgpuFeatureInstance<UiFeature<Composite>>,
        report: &'a dyn Any,
    ) -> (u64, Arc<dyn Any + Send + Sync>, &'a dyn Any) {
        (key, instance.identity.clone(), report)
    }

    fn instance() -> WgpuFeatureInstance<UiFeature<Composite>> {
        WgpuFeatureInstance::new(
            1,
            WgpuRenderProducerId::new(1),
            Arc::new(AtomicBool::new(false)),
            &WgpuFeatureDefinition::new(UiFeature::new()),
        )
        .unwrap()
    }

    fn candidate() -> (
        WgpuRenderProgramBuilder,
        WgpuProgramTexture<Composite>,
        Arc<UiCapture<Composite>>,
    ) {
        let mut builder = WgpuRenderProgramBuilder::new(
            RenderFrameSourceGeneration::try_acquire().unwrap(),
            1,
            GraphLimits::default(),
        )
        .unwrap();
        let target = builder
            .texture(WgpuProgramTextureDescriptor {
                native: wgpu::TextureDescriptor {
                    label: Some("ui-policy-binding"),
                    size: wgpu::Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                },
                transfer: TextureTransfer::Linear,
                alpha: TextureAlpha::Premultiplied,
            })
            .unwrap();
        let capture = Arc::new(
            UiCapture::new(
                &[packet()],
                RenderTarget::Window(WindowId::PRIMARY),
                Some(WindowId::PRIMARY),
                target.clone(),
                ViewportRect::new(0, 0, 8, 8).unwrap(),
            )
            .unwrap(),
        );
        (builder, target, capture)
    }

    fn finish(
        mut builder: WgpuRenderProgramBuilder,
        target: &WgpuProgramTexture<Composite>,
        instance: &WgpuFeatureInstance<UiFeature<Composite>>,
        capture: Arc<UiCapture<Composite>>,
    ) -> WgpuRenderProgram {
        builder
            .operation(
                UI_OPERATION,
                instance,
                capture,
                &[],
                &[target.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        builder.finish(&[target.output()]).unwrap()
    }

    fn packet() -> Arc<UiPaintPacket> {
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("ui-policy-binding", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::fill())
                        .with_key(UiLocalKey::runtime("ui-policy-binding", "fill").unwrap())
                        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0))),
                ),
            )
            .build()
            .unwrap();
        let environment =
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 8.0, 8.0, 1.0, 1, 1, 1, 1).unwrap();
        let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, RenderTarget::PrimaryWindow, &Default::default())
            .unwrap()
    }
}
