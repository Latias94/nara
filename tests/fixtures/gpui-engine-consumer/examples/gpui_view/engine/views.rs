//! Independent finite graph-value previews on the original rendering Host.

use super::*;
use nara_gpui_engine_consumer::graph::{
    GraphCommand, NodeKind,
    preview::{GraphPreviewBasis, GraphPreviewRequestId, GraphPreviewValue},
};

use super::super::graph_panel::{GraphIntent, GraphSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum ViewKey {
    A,
    B,
}

impl ViewKey {
    pub const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

pub(in super::super) struct EngineViewImage {
    pub key: ViewKey,
    pub basis: GraphPreviewBasis,
    pub request: GraphPreviewRequestId,
    pub value: Option<f64>,
    pub display: WgpuImageDisplay,
    pub clear: [f32; 4],
}

/// The original immutable placement joined to its actual native composite.
pub(in super::super) struct EngineViewPresentation {
    image: Rc<EngineViewImage>,
    mapping: DisplayMapping2d,
    composite: WgpuImageCompositeReceipt,
}

impl EngineViewPresentation {
    pub fn image(&self) -> &Rc<EngineViewImage> {
        &self.image
    }

    pub fn mapping(&self) -> DisplayMapping2d {
        self.mapping
    }

    pub fn composite(&self) -> WgpuImageCompositeReceipt {
        self.composite
    }
}

pub(super) struct ViewStreams {
    streams: [ViewStream; 2],
}

struct ViewStream {
    key: ViewKey,
    producer: Option<WgpuRenderProducer>,
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    target: RenderImagePacket,
    policy: WgpuPolicyInstallation<WgpuStockPolicy>,
    frame: u64,
    current: Option<Rc<EngineViewImage>>,
    presented: Option<Rc<EngineViewPresentation>>,
    last_presentation: u64,
}

impl ViewStream {
    fn new(host: &mut WgpuRenderHost, key: ViewKey) -> Result<Self> {
        let source = RenderFrameSourceGeneration::try_acquire()
            .context("graph preview source identities are exhausted")?;
        let view =
            RenderViewId::try_acquire().context("graph preview view identities are exhausted")?;
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle()?,
            Extent2d::new(320, 180).context("the graph preview extent must be nonempty")?,
        )?;
        let policy = WgpuPolicyDefinition::new(WgpuStockPolicy::new()).install()?;
        let producer = host.create_producer()?;
        Ok(Self {
            key,
            producer: Some(producer),
            source,
            view,
            target,
            policy,
            frame: 0,
            current: None,
            presented: None,
            last_presentation: 0,
        })
    }

    fn publish(
        &mut self,
        host: &mut WgpuRenderHost,
        host_frame: &mut u64,
        value: &GraphPreviewValue,
    ) -> Result<()> {
        let producer = self
            .producer
            .as_ref()
            .context("the graph preview is closed")?;
        let frame = self
            .frame
            .checked_add(1)
            .context("graph preview frame identities are exhausted")?;
        let clear = clear_color(self.key, value.value);
        let viewport = ViewportRect::from_extent(self.target.extent())
            .context("the graph preview viewport must be nonempty")?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: self.view,
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(self.target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 320.0, viewport)
                .context("the graph preview must have a finite mapping")?,
            order: 0,
            load: ViewLoadOp::Clear(Color::rgba(clear[0], clear[1], clear[2], clear[3])),
        });
        let packet = build_image_render_frame_packet(self.source, frame, &views, self.target)?
            .context("the graph preview did not produce an image packet")?;
        let input = self
            .policy
            .capture(packet, Arc::new(WgpuStockCapture::new()))?;
        *host_frame = (*host_frame)
            .checked_add(1)
            .context("Host transaction identities are exhausted")?;
        host.begin_frame(*host_frame)?;
        if !host.image_target_matches(producer, self.source, self.target) {
            let reservation = host.reserve_image_target(producer, self.source, self.target)?;
            host.activate_image_target(producer, reservation)?;
        }
        let output = host.submit(producer, &input)?;
        self.frame = frame;
        self.presented = None;
        self.current = None;
        let display = host.display_image(output)?;
        self.current = Some(Rc::new(EngineViewImage {
            key: self.key,
            basis: value.basis,
            request: value.request,
            value: value.value,
            display,
            clear,
        }));
        Ok(())
    }

    fn retire(&mut self, host: &mut WgpuRenderHost, retiring: &mut Vec<WgpuSubmissionPrefix>) {
        self.presented = None;
        self.current = None;
        if let Some(producer) = self.producer.take()
            && let Some(prefix) = host.retire_producer(&producer)
        {
            retiring.push(prefix);
        }
    }
}

impl Engine {
    pub fn open_views(&mut self) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        ensure!(
            self.view_streams.is_none(),
            "the graph previews are already initialized"
        );
        let mut first = ViewStream::new(&mut self.host, ViewKey::A)?;
        let second = match ViewStream::new(&mut self.host, ViewKey::B) {
            Ok(stream) => stream,
            Err(error) => {
                first.retire(&mut self.host, &mut self.retiring);
                return Err(error);
            }
        };
        self.view_streams = Some(ViewStreams {
            streams: [first, second],
        });
        Ok(())
    }

    pub fn publish_views(&mut self, value: &GraphPreviewValue) -> Result<()> {
        self.require_view_basis(value.basis)?;
        ensure!(
            value.value.is_none_or(f64::is_finite),
            "the graph preview value must be finite"
        );
        let streams = self
            .view_streams
            .as_mut()
            .context("the graph previews are not open")?;
        for stream in &mut streams.streams {
            if stream.producer.is_some() {
                stream.publish(&mut self.host, &mut self.host_frame, value)?;
            }
        }
        Ok(())
    }

    pub fn pulse_view(&mut self, key: ViewKey) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let streams = self
            .view_streams
            .as_mut()
            .context("the graph previews are not open")?;
        let stream = &mut streams.streams[key.index()];
        ensure!(stream.producer.is_some(), "the graph preview is closed");
        let image = stream
            .current
            .as_ref()
            .context("the graph preview has no image")?;
        let value = GraphPreviewValue {
            basis: image.basis,
            request: image.request,
            value: image.value,
        };
        stream.publish(&mut self.host, &mut self.host_frame, &value)
    }

    pub fn redisplay_view(&mut self, key: ViewKey) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let streams = self
            .view_streams
            .as_mut()
            .context("the graph previews are not open")?;
        let stream = &mut streams.streams[key.index()];
        ensure!(stream.producer.is_some(), "the graph preview is closed");
        let image = stream
            .current
            .as_ref()
            .context("the graph preview has no image")?;
        let display = self.host.display_image(image.display.output().clone())?;
        stream.current = Some(Rc::new(EngineViewImage {
            key,
            basis: image.basis,
            request: image.request,
            value: image.value,
            display,
            clear: image.clear,
        }));
        stream.presented = None;
        Ok(())
    }

    pub fn view_images(&self) -> [Option<Rc<EngineViewImage>>; 2] {
        std::array::from_fn(|index| {
            self.view_streams
                .as_ref()
                .and_then(|streams| streams.streams[index].current.clone())
        })
    }

    pub fn accepts_view(&self, image: &EngineViewImage) -> bool {
        self.require_view_image(image).is_ok() && self.require_view_basis(image.basis).is_ok()
    }

    pub fn complete_view(
        &mut self,
        image: Rc<EngineViewImage>,
        composite: WgpuImageCompositeReceipt,
        mapping: DisplayMapping2d,
    ) -> Result<Rc<EngineViewPresentation>> {
        self.require_view_basis(image.basis)?;
        let stream = self.require_view_image(&image)?;
        let receipt = image.display.receipt();
        let window = composite.window();
        ensure!(
            composite.image() == receipt
                && composite.occurrence() == image.display.occurrence()
                && window.window_id() == WindowId::PRIMARY
                && window.backend_instance_id() == self.host.backend_instance_id()
                && window.device_epoch() == self.host.device_epoch()
                && window
                    .surface_generation()
                    .is_some_and(|generation| generation > 0)
                && window.display_frame_index() > 0
                && window.display_frame_index() <= self.host_frame
                && composite.presentation_generation() > stream.last_presentation
                && mapping.target() == receipt.target().extent(),
            "the GPUI frame did not display this exact current graph preview occurrence"
        );
        ensure!(
            stream
                .presented
                .as_ref()
                .is_none_or(|previous| previous.mapping == mapping),
            "a changed graph preview mapping requires a new display occurrence"
        );
        let key = image.key;
        let presentation = Rc::new(EngineViewPresentation {
            image,
            mapping,
            composite,
        });
        let streams = self
            .view_streams
            .as_mut()
            .context("the graph previews are not open")?;
        let stream = &mut streams.streams[key.index()];
        stream.last_presentation = composite.presentation_generation();
        stream.presented = Some(Rc::clone(&presentation));
        Ok(presentation)
    }

    pub fn activate_view(
        &mut self,
        presentation: &Rc<EngineViewPresentation>,
        window_point: [f32; 2],
    ) -> Result<()> {
        let image = &presentation.image;
        let snapshot = self.require_view_basis(image.basis)?;
        let stream = self.require_view_image(image)?;
        ensure!(
            stream
                .presented
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, presentation))
                && presentation.composite.image() == image.display.receipt()
                && presentation.composite.occurrence() == image.display.occurrence()
                && presentation.composite.presentation_generation() == stream.last_presentation,
            "the graph preview presentation is stale"
        );
        ensure!(
            presentation.mapping.target() == image.display.receipt().target().extent()
                && presentation.mapping.panel_rect().contains(window_point)
                && presentation.mapping.panel_to_target(window_point).is_some(),
            "the graph preview point is outside the displayed viewport"
        );
        let (node, scalar) = snapshot
            .view
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Scalar(value) => Some((node.id, *value)),
                _ => None,
            })
            .nth(image.key.index())
            .context("the graph preview has no corresponding scalar node")?;
        let value = scalar + 1.0;
        ensure!(
            value.is_finite(),
            "the graph preview edit must remain finite"
        );
        self.graph_intent(GraphIntent::Edit {
            document: image.basis.document,
            revision: image.basis.revision,
            command: GraphCommand::SetScalar { node, value },
        })
    }

    pub fn close_view(&mut self, key: ViewKey) -> Result<()> {
        if let Some(streams) = &mut self.view_streams {
            streams.streams[key.index()].retire(&mut self.host, &mut self.retiring);
        }
        Ok(())
    }

    pub fn close_views(&mut self) -> Result<()> {
        for key in [ViewKey::A, ViewKey::B] {
            self.close_view(key)?;
        }
        Ok(())
    }

    pub fn view_retirement_progress(&self) -> Result<(usize, bool)> {
        let mut complete = true;
        for prefix in &self.retiring {
            complete &= prefix.poll()?;
            ensure!(
                prefix.cleanup_fault_count() == 0,
                "a graph View prefix failed cleanup"
            );
        }
        Ok((self.retiring.len(), complete))
    }

    fn require_view_basis(&self, basis: GraphPreviewBasis) -> Result<GraphSnapshot> {
        ensure!(!self.closing, "the engine is closing");
        let snapshot = self.graph_snapshot()?;
        ensure!(
            snapshot.document == basis.document && snapshot.revision == basis.revision,
            "the graph preview document or revision is stale"
        );
        Ok(snapshot)
    }

    fn require_view_image(&self, image: &EngineViewImage) -> Result<&ViewStream> {
        ensure!(!self.closing, "the engine is closing");
        let streams = self
            .view_streams
            .as_ref()
            .context("the graph previews are not open")?;
        let stream = &streams.streams[image.key.index()];
        let producer = stream
            .producer
            .as_ref()
            .context("the graph preview is closed")?;
        let receipt = image.display.receipt();
        ensure!(
            stream
                .current
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.as_ref(), image))
                && image.display.output().is_current()
                && receipt.producer_id() == producer.id()
                && receipt.source_generation() == stream.source
                && receipt.target() == stream.target
                && receipt.render_transaction_frame_index() == stream.frame
                && receipt.backend_instance_id() == self.host.backend_instance_id()
                && receipt.device_epoch() == self.host.device_epoch()
                && self
                    .host
                    .image_target_matches(producer, stream.source, stream.target),
            "the graph preview image, source, output or device is stale"
        );
        Ok(stream)
    }
}

fn clear_color(key: ViewKey, value: Option<f64>) -> [f32; 4] {
    let level = value.map(|value| (value.rem_euclid(16.0) / 16.0) as f32);
    let (low, high) = match level {
        Some(level) => (0.16 + 0.24 * level, 0.68 + 0.28 * level),
        None => (0.10, 0.42),
    };
    match key {
        ViewKey::A => [0.06, low, high, 1.0],
        ViewKey::B => [0.06, high, low, 1.0],
    }
}
