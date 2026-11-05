//! One original WGPU Host; the frontend supplies captured software pixels only.

#[allow(dead_code)]
#[path = "../../../render-composition-consumer/tests/support/window_gpu.rs"]
mod window_gpu;

use std::{
    io,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_editor::native::EditorPanelImageFrame;
use nara_editor_ui_host_consumer::{
    Result,
    conformance::{Snapshot, SoftwarePainter},
    native_upload::{CpuImage, CpuImagePolicy},
};
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, RenderViewId, RenderWindowPacket, ViewLoadOp, ViewMapping2d,
    ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuImageDisplay, WgpuImageOutput, WgpuPolicyDefinition, WgpuPolicyInstallation,
    WgpuRenderHost, WgpuRenderProducer, WgpuWindowFrameOutcome,
};
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use window_gpu::ImagePainter;
use winit::window::Window;

pub struct Graphics {
    host: WgpuRenderHost,
    producer: WgpuRenderProducer,
    policy: WgpuPolicyInstallation<CpuImagePolicy>,
    target: RenderImagePacket,
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    painter: ImagePainter,
    pub frame: u64,
    pub presentations: usize,
    pub readback_pixels: usize,
}

fn packet(window: &Window) -> RenderWindowPacket {
    let size = window.inner_size();
    RenderWindowPacket {
        id: WindowId::PRIMARY,
        resolution: WindowResolution::new(size.width, size.height)
            .with_scale_factor(window.scale_factor()),
        present_mode: PresentMode::AutoNoVsync,
    }
}

impl Graphics {
    pub fn new(window: &Arc<Window>) -> Result<Self> {
        let mut host = WgpuRenderHost::new(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::clone(window)),
        )?;
        let painter = host.with_device(packet(window), |scope| {
            ImagePainter::new(scope.device(), scope.surface_format(), false)
        })?;
        let source = RenderFrameSourceGeneration::try_acquire().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle()?,
            Extent2d::new(400, 180).unwrap(),
        )?;
        let producer = host.create_producer()?;
        let reservation = host.reserve_image_target(&producer, source, target)?;
        host.activate_image_target(&producer, reservation)?;
        Ok(Self {
            host,
            producer,
            policy: WgpuPolicyDefinition::new(CpuImagePolicy::default()).install()?,
            target,
            source,
            view: RenderViewId::try_acquire().unwrap(),
            painter,
            frame: 0,
            presentations: 0,
            readback_pixels: 0,
        })
    }

    pub fn begin(&mut self, window: &Window) -> Result<()> {
        self.frame += 1;
        self.host.begin_window_frame(packet(window), self.frame)?;
        Ok(())
    }

    pub fn render(
        &mut self,
        snapshot: &Snapshot,
        painter: &mut SoftwarePainter,
        readback: bool,
    ) -> Result<WgpuImageDisplay> {
        let pixels: Arc<[u8]> = painter.paint(snapshot, 1.0)?.into();
        let viewport = ViewportRect::from_extent(self.target.extent()).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: self.view,
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(self.target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 180.0, viewport).unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let input = self.policy.capture(
            build_image_render_frame_packet(self.source, self.frame, &views, self.target)?.unwrap(),
            Arc::new(CpuImage {
                extent: self.target.extent(),
                pixels: Arc::clone(&pixels),
            }),
        )?;
        let output = self.host.submit(&self.producer, &input)?;
        if readback {
            self.assert_pixels(&output, &pixels)?;
        }
        Ok(self.host.display_image(output)?)
    }

    pub fn mirror(&mut self, display: &WgpuImageDisplay) -> Result<WgpuImageDisplay> {
        Ok(self.host.display_image(display.output().clone())?)
    }

    pub fn present(
        &mut self,
        window: &Window,
        frame: &EditorPanelImageFrame<Snapshot>,
        sample: bool,
    ) -> Result<WgpuWindowFrameOutcome> {
        self.present_count(window, frame, if sample { frame.images().len() } else { 0 })
    }

    pub fn present_count(
        &mut self,
        window: &Window,
        frame: &EditorPanelImageFrame<Snapshot>,
        count: usize,
    ) -> Result<WgpuWindowFrameOutcome> {
        assert!(count <= frame.images().len());
        let groups = frame
            .images()
            .iter()
            .take(count)
            .map(|image| {
                self.host
                    .with_image(image.display().output(), |device, _, view| {
                        self.painter.bind(device, view, view)
                    })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let displays = frame
            .images()
            .iter()
            .take(count)
            .map(|image| image.display().clone())
            .collect::<Vec<_>>();
        let outcome = self.host.present_window(
            packet(window),
            self.frame,
            &displays,
            |native| {
                if count > 0 {
                    native.encode(|_, _, encoder, target| {
                        for (index, (image, group)) in
                            frame.images().iter().zip(&groups).enumerate()
                        {
                            let mapping = image.mapping();
                            let content = mapping.content_rect();
                            let scale = mapping.pixels_per_point();
                            let clip = image.clip();
                            let min = clip.min.map(|value| (value * scale).round() as u32);
                            let max = clip.max.map(|value| (value * scale).round() as u32);
                            self.painter.draw_mapped(
                                encoder,
                                target,
                                group,
                                [
                                    content.min[0] * scale,
                                    content.min[1] * scale,
                                    (content.max[0] - content.min[0]) * scale,
                                    (content.max[1] - content.min[1]) * scale,
                                ],
                                [min[0], min[1], max[0] - min[0], max[1] - min[1]],
                                if index == 0 {
                                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                                } else {
                                    wgpu::LoadOp::Load
                                },
                            );
                        }
                    });
                    for display in &displays {
                        native.record_image_sample(display)?;
                    }
                } else {
                    native.encode(|_, _, encoder, target| {
                        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("conformance-missing-image-control"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: target,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                    });
                }
                Ok(())
            },
            || window.pre_present_notify(),
        )?;
        if outcome.window().is_some() {
            self.presentations += 1;
        }
        Ok(outcome)
    }

    fn assert_pixels(&mut self, output: &WgpuImageOutput, expected: &[u8]) -> Result<()> {
        let row = 400 * 4u32;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let (readback, prefix) = self.host.submit_image_use(
            std::slice::from_ref(output),
            |device, _, encoder, views| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("conformance-full-image-readback"),
                    size: u64::from(padded) * 180,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    views[0].texture().as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded),
                            rows_per_image: Some(180),
                        },
                    },
                    wgpu::Extent3d {
                        width: 400,
                        height: 180,
                        depth_or_array_layers: 1,
                    },
                );
                Ok(buffer)
            },
        )?;
        let (sender, receiver) = mpsc::sync_channel(1);
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut mapped = false;
        loop {
            let complete = prefix.poll()?;
            self.host
                .with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))??;
            if !mapped {
                match receiver.try_recv() {
                    Ok(result) => {
                        result?;
                        mapped = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(io::Error::other("conformance map callback disappeared").into());
                    }
                }
            }
            if mapped && complete {
                break;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("conformance image readback timed out").into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        {
            let actual = readback.get_mapped_range(..)?;
            for (y, (actual, expected)) in actual
                .chunks_exact(padded as usize)
                .zip(expected.chunks_exact(row as usize))
                .enumerate()
            {
                assert_eq!(
                    &actual[..row as usize],
                    expected,
                    "original conformance image row {y}"
                );
            }
        }
        readback.unmap();
        self.readback_pixels += 400 * 180;
        Ok(())
    }

    pub fn close(mut self) -> Result<()> {
        self.policy.close();
        let prefix = self.host.retire_producer(&self.producer);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.host.poll_retirement()? {
            if Instant::now() >= deadline {
                return Err(
                    io::Error::other("original conformance GPU Host retirement timed out").into(),
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(prefix) = prefix {
            assert!(prefix.poll()?);
        }
        assert_eq!(self.host.cleanup_fault_count(), 0);
        assert_eq!(self.host.abandoned_submission_count(), 0);
        Ok(())
    }
}
