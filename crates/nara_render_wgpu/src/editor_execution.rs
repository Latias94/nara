use nara_render::{
    Extent2d, RenderFramePacket, RenderFrameTarget, RenderImagePacket, RenderWindowPacket,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowHandleProvider, WindowTargetError},
};
use thiserror::Error;

use crate::{
    WgpuFrameTransactionStats, WgpuRenderBackend, WgpuRenderError,
    backend::{WgpuEditorAcquiredSurface, WgpuEditorDeviceNamespace},
    presented::{
        WgpuEditorCompositeReceipt, WgpuEditorWindowPresentedReceipt, WgpuSubmittedImageReceipt,
    },
    surface::clear_color_to_wgpu,
};

const EDITOR_IMAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Failure emitted by the private first-party Editor GPU execution owner.
#[doc(hidden)]
#[derive(Debug, Error)]
pub enum WgpuEditorExecutionError {
    #[error(transparent)]
    Backend(#[from] WgpuRenderError),
    #[error(transparent)]
    WindowTarget(#[from] WindowTargetError),
    #[error("the Editor Scene submission requires an image-target render packet")]
    ExpectedImageTarget,
    #[error("the Editor Scene image target generation was exhausted")]
    TargetGenerationExhausted,
    #[error("the Editor composite presentation generation was exhausted")]
    PresentationGenerationExhausted,
    #[error("the Editor Scene image submission no longer belongs to the live target")]
    StaleImageSubmission,
    #[error("the egui texture registration generation must be non-zero")]
    InvalidTextureRegistrationGeneration,
    #[error("the Editor toolkit paint callback failed: {message}")]
    ToolkitPaint { message: String },
}

impl WgpuEditorExecutionError {
    #[must_use]
    pub fn toolkit_paint(message: impl Into<String>) -> Self {
        Self::ToolkitPaint {
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn requires_owner_replacement(&self) -> bool {
        matches!(
            self,
            Self::Backend(
                WgpuRenderError::BackendUnavailable
                    | WgpuRenderError::DeviceLost
                    | WgpuRenderError::DeviceEpochExhausted
            )
        )
    }
}

struct WgpuEditorImageTarget {
    packet: RenderImagePacket,
    generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl WgpuEditorImageTarget {
    fn matches_namespace(&self, namespace: &WgpuEditorDeviceNamespace) -> bool {
        self.backend_instance_id == namespace.backend_instance_id
            && self.device_epoch == namespace.device_epoch
    }

    fn matches_receipt(&self, receipt: WgpuSubmittedImageReceipt) -> bool {
        self.packet == receipt.target()
            && self.generation == receipt.target_generation()
            && self.backend_instance_id == receipt.backend_instance_id()
            && self.device_epoch == receipt.device_epoch()
    }
}

/// One scoped view of the device namespace used to initialize the private Editor toolkit renderer.
#[doc(hidden)]
pub struct WgpuEditorDeviceScope<'frame> {
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    surface_extent: Extent2d,
    backend_instance_id: u64,
    device_epoch: u64,
}

impl<'frame> WgpuEditorDeviceScope<'frame> {
    #[must_use]
    pub const fn device(&self) -> &'frame wgpu::Device {
        self.device
    }

    #[must_use]
    pub const fn queue(&self) -> &'frame wgpu::Queue {
        self.queue
    }

    #[must_use]
    pub const fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    #[must_use]
    pub const fn surface_extent(&self) -> Extent2d {
        self.surface_extent
    }

    #[must_use]
    pub const fn backend_instance_id(&self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.device_epoch
    }
}

/// Scoped Scene texture facts available only while one Editor GPU call is active.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct WgpuEditorSceneTexture<'frame> {
    view: &'frame wgpu::TextureView,
    receipt: WgpuSubmittedImageReceipt,
}

impl<'frame> WgpuEditorSceneTexture<'frame> {
    #[must_use]
    pub const fn view(self) -> &'frame wgpu::TextureView {
        self.view
    }

    #[must_use]
    pub const fn receipt(self) -> WgpuSubmittedImageReceipt {
        self.receipt
    }
}

#[derive(Clone, Copy)]
struct WgpuEditorSceneSample {
    image: WgpuSubmittedImageReceipt,
    texture_registration_generation: u64,
}

fn join_composite_receipt(
    last_presentation_generation: &mut u64,
    submitted_scene: Option<WgpuSubmittedImageReceipt>,
    sampled_scene: Option<WgpuEditorSceneSample>,
    window: WgpuEditorWindowPresentedReceipt,
) -> Result<Option<WgpuEditorCompositeReceipt>, WgpuEditorExecutionError> {
    let sampled_scene = sampled_scene.filter(|sample| {
        submitted_scene == Some(sample.image)
            && sample.image.backend_instance_id() == window.backend_instance_id()
            && sample.image.device_epoch() == window.device_epoch()
    });
    let Some(sample) = sampled_scene else {
        return Ok(None);
    };
    let generation = last_presentation_generation
        .checked_add(1)
        .ok_or(WgpuEditorExecutionError::PresentationGenerationExhausted)?;
    *last_presentation_generation = generation;
    Ok(Some(WgpuEditorCompositeReceipt::new(
        sample.image,
        window,
        sample.texture_registration_generation,
        generation,
    )))
}

/// Borrowed command-encoding scope for one Editor-window paint.
#[doc(hidden)]
pub struct WgpuEditorWindowFrame<'frame> {
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    encoder: &'frame mut wgpu::CommandEncoder,
    target_view: &'frame wgpu::TextureView,
    surface_format: wgpu::TextureFormat,
    surface_extent: Extent2d,
    scene: Option<WgpuEditorSceneTexture<'frame>>,
    leading_command_buffers: &'frame mut Vec<wgpu::CommandBuffer>,
    sampled_scene: &'frame mut Option<WgpuEditorSceneSample>,
}

impl<'frame> WgpuEditorWindowFrame<'frame> {
    #[must_use]
    pub const fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    #[must_use]
    pub const fn surface_extent(&self) -> Extent2d {
        self.surface_extent
    }

    #[must_use]
    pub const fn scene_texture(&self) -> Option<WgpuEditorSceneTexture<'frame>> {
        self.scene
    }

    pub fn encode<R>(
        &mut self,
        operation: impl for<'encode> FnOnce(
            &'encode wgpu::Device,
            &'encode wgpu::Queue,
            &'encode mut wgpu::CommandEncoder,
            &'encode wgpu::TextureView,
        ) -> R,
    ) -> R {
        operation(self.device, self.queue, self.encoder, self.target_view)
    }

    pub fn prepend_command_buffers(
        &mut self,
        command_buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
    ) {
        self.leading_command_buffers.extend(command_buffers);
    }

    pub fn record_scene_sample(
        &mut self,
        texture_registration_generation: u64,
    ) -> Result<(), WgpuEditorExecutionError> {
        if texture_registration_generation == 0 {
            return Err(WgpuEditorExecutionError::InvalidTextureRegistrationGeneration);
        }
        let Some(scene) = self.scene else {
            return Err(WgpuEditorExecutionError::StaleImageSubmission);
        };
        *self.sampled_scene = Some(WgpuEditorSceneSample {
            image: scene.receipt,
            texture_registration_generation,
        });
        Ok(())
    }
}

/// Terminal facts from one attempted Editor-window paint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WgpuEditorFrameOutcome {
    window: Option<WgpuEditorWindowPresentedReceipt>,
    composite: Option<WgpuEditorCompositeReceipt>,
}

impl WgpuEditorFrameOutcome {
    #[must_use]
    pub const fn window(self) -> Option<WgpuEditorWindowPresentedReceipt> {
        self.window
    }

    #[must_use]
    pub const fn composite(self) -> Option<WgpuEditorCompositeReceipt> {
        self.composite
    }
}

/// Private fixed-topology GPU owner used by the first-party Editor Host.
///
/// This type intentionally exposes no target registry or general render graph. Raw WGPU values are
/// borrowed only through scoped first-party callbacks.
#[doc(hidden)]
pub struct WgpuEditorExecution {
    backend: WgpuRenderBackend,
    handles: BackendWindowHandles,
    image_target: Option<WgpuEditorImageTarget>,
    last_target_generation: u64,
    last_presentation_generation: u64,
}

impl std::fmt::Debug for WgpuEditorExecution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorExecution")
            .field("backend_instance_id", &self.backend.instance_id())
            .field("device_epoch", &self.backend.device_epoch())
            .field("image_target_active", &self.image_target.is_some())
            .finish_non_exhaustive()
    }
}

impl WgpuEditorExecution {
    pub fn new(
        window_id: WindowId,
        provider: WindowHandleProvider,
    ) -> Result<Self, WgpuEditorExecutionError> {
        let handles = BackendWindowHandles::default();
        handles.insert(window_id, provider)?;
        Ok(Self {
            backend: WgpuRenderBackend::default(),
            handles,
            image_target: None,
            last_target_generation: 0,
            last_presentation_generation: 0,
        })
    }

    #[must_use]
    pub const fn backend_instance_id(&self) -> u64 {
        self.backend.instance_id()
    }

    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.backend.device_epoch()
    }

    #[must_use]
    pub const fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.backend.frame_transaction_stats()
    }

    pub fn with_device<R>(
        &mut self,
        window: RenderWindowPacket,
        operation: impl for<'frame> FnOnce(WgpuEditorDeviceScope<'frame>) -> R,
    ) -> Result<R, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        Ok(operation(WgpuEditorDeviceScope {
            device: &namespace.device,
            queue: &namespace.queue,
            surface_format: namespace.surface_format,
            surface_extent: namespace.surface_extent,
            backend_instance_id: namespace.backend_instance_id,
            device_epoch: namespace.device_epoch,
        }))
    }

    pub fn submit_image(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(WgpuEditorExecutionError::ExpectedImageTarget);
        };
        self.backend
            .begin_frame_transaction(packet.frame_index(), None);
        if let Err(error) = self
            .backend
            .admit_packet(packet.source_generation(), packet.frame_index())
        {
            self.backend.reject_captured_packet();
            return Err(error.into());
        }
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        self.ensure_image_target(&namespace, target)?;
        let image_target = self
            .image_target
            .as_ref()
            .expect("ensuring an Editor image target installs it");
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_editor_scene_encoder"),
                });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_editor_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &image_target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color_to_wgpu(packet.view().clear_color)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        namespace.queue.submit([encoder.finish()]);
        self.backend.record_editor_image_submission();
        Ok(WgpuSubmittedImageReceipt::new(
            target,
            image_target.generation,
            namespace.backend_instance_id,
            namespace.device_epoch,
            packet.frame_index(),
        ))
    }

    pub fn with_scene_texture<R>(
        &mut self,
        window: RenderWindowPacket,
        submission: WgpuSubmittedImageReceipt,
        operation: impl for<'frame> FnOnce(
            WgpuEditorDeviceScope<'frame>,
            WgpuEditorSceneTexture<'frame>,
        ) -> R,
    ) -> Result<R, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        let target = self
            .image_target
            .as_ref()
            .filter(|target| {
                target.matches_namespace(&namespace) && target.matches_receipt(submission)
            })
            .ok_or(WgpuEditorExecutionError::StaleImageSubmission)?;
        Ok(operation(
            WgpuEditorDeviceScope {
                device: &namespace.device,
                queue: &namespace.queue,
                surface_format: namespace.surface_format,
                surface_extent: namespace.surface_extent,
                backend_instance_id: namespace.backend_instance_id,
                device_epoch: namespace.device_epoch,
            },
            WgpuEditorSceneTexture {
                view: &target.view,
                receipt: submission,
            },
        ))
    }

    pub fn present_window(
        &mut self,
        window: RenderWindowPacket,
        editor_frame_index: u64,
        scene_submission: Option<WgpuSubmittedImageReceipt>,
        paint: impl for<'frame> FnOnce(
            &mut WgpuEditorWindowFrame<'frame>,
        ) -> Result<(), WgpuEditorExecutionError>,
        pre_present: impl FnOnce(),
    ) -> Result<WgpuEditorFrameOutcome, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        let Some(acquired) = self.backend.acquire_editor_surface(window.id)? else {
            return Ok(WgpuEditorFrameOutcome::default());
        };
        let scene = match scene_submission {
            Some(submission) => Some(self.scene_texture(&namespace, submission)?),
            None => None,
        };
        let WgpuEditorAcquiredSurface {
            texture,
            surface_generation,
            format,
            extent,
        } = acquired;
        let target_view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_editor_window_encoder"),
                });
        let mut leading_command_buffers = Vec::new();
        let mut sampled_scene = None;
        paint(&mut WgpuEditorWindowFrame {
            device: &namespace.device,
            queue: &namespace.queue,
            encoder: &mut encoder,
            target_view: &target_view,
            surface_format: format,
            surface_extent: extent,
            scene,
            leading_command_buffers: &mut leading_command_buffers,
            sampled_scene: &mut sampled_scene,
        })?;
        namespace.queue.submit(
            leading_command_buffers
                .into_iter()
                .chain([encoder.finish()]),
        );
        pre_present();
        namespace.queue.present(texture);
        self.backend.record_editor_window_presentation();
        let window_receipt = WgpuEditorWindowPresentedReceipt::new(
            window.id,
            surface_generation,
            namespace.backend_instance_id,
            namespace.device_epoch,
            editor_frame_index,
        );
        let composite = join_composite_receipt(
            &mut self.last_presentation_generation,
            scene_submission,
            sampled_scene,
            window_receipt,
        )?;
        Ok(WgpuEditorFrameOutcome {
            window: Some(window_receipt),
            composite,
        })
    }

    pub fn retire_image_target(&mut self) {
        self.image_target = None;
    }

    fn scene_texture<'frame>(
        &'frame self,
        namespace: &WgpuEditorDeviceNamespace,
        submission: WgpuSubmittedImageReceipt,
    ) -> Result<WgpuEditorSceneTexture<'frame>, WgpuEditorExecutionError> {
        let target = self
            .image_target
            .as_ref()
            .filter(|target| {
                target.matches_namespace(namespace) && target.matches_receipt(submission)
            })
            .ok_or(WgpuEditorExecutionError::StaleImageSubmission)?;
        Ok(WgpuEditorSceneTexture {
            view: &target.view,
            receipt: submission,
        })
    }

    fn ensure_image_target(
        &mut self,
        namespace: &WgpuEditorDeviceNamespace,
        packet: RenderImagePacket,
    ) -> Result<(), WgpuEditorExecutionError> {
        if self
            .image_target
            .as_ref()
            .is_some_and(|target| target.packet == packet && target.matches_namespace(namespace))
        {
            return Ok(());
        }
        let generation = self
            .last_target_generation
            .checked_add(1)
            .ok_or(WgpuEditorExecutionError::TargetGenerationExhausted)?;
        let extent = packet.extent();
        let texture = namespace.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nara_editor_scene_target"),
            size: wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EDITOR_IMAGE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[EDITOR_IMAGE_FORMAT],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(EDITOR_IMAGE_FORMAT),
            ..Default::default()
        });
        self.image_target = Some(WgpuEditorImageTarget {
            packet,
            generation,
            backend_instance_id: namespace.backend_instance_id,
            device_epoch: namespace.device_epoch,
            _texture: texture,
            view,
        });
        self.last_target_generation = generation;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::{AssetId, Handle};
    use nara_ecs::Entity;
    use nara_render::{
        __private::{EditorRenderGeneration, build_editor_render_frame_packet},
        Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, ViewportRect,
    };

    #[test]
    fn composite_receipt_requires_the_exact_submitted_sample_and_window_namespace() {
        let image = test_image_packet();
        let submitted = WgpuSubmittedImageReceipt::new(image, 4, 10, 2, 7);
        let window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            3,
            submitted.backend_instance_id(),
            submitted.device_epoch(),
            9,
        );
        let mut presentation_generation = 0;

        assert_eq!(
            join_composite_receipt(&mut presentation_generation, Some(submitted), None, window,)
                .unwrap(),
            None,
        );
        assert_eq!(presentation_generation, 0);

        let foreign_window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            3,
            submitted.backend_instance_id().saturating_add(1),
            submitted.device_epoch(),
            9,
        );
        let sample = WgpuEditorSceneSample {
            image: submitted,
            texture_registration_generation: 5,
        };
        assert_eq!(
            join_composite_receipt(
                &mut presentation_generation,
                Some(submitted),
                Some(sample),
                foreign_window,
            )
            .unwrap(),
            None,
        );
        assert_eq!(presentation_generation, 0);

        let composite = join_composite_receipt(
            &mut presentation_generation,
            Some(submitted),
            Some(sample),
            window,
        )
        .unwrap()
        .unwrap();
        assert_eq!(composite.presentation_generation(), 1);
        assert_eq!(composite.texture_registration_generation(), 5);
        assert_eq!(presentation_generation, 1);
    }

    fn test_image_packet() -> RenderImagePacket {
        let generation = EditorRenderGeneration::try_acquire().unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(Handle::<RenderImage2d>::new(AssetId::from_raw(17))),
            viewport: ViewportRect::new(0, 0, 64, 64).unwrap(),
            world_position: nara_core::Vec2::ZERO,
            viewport_height: 64.0,
            order: 0,
            clear_color: Color::BLACK,
        });
        let packet = build_editor_render_frame_packet(generation, 1, &views)
            .unwrap()
            .unwrap();
        match packet.target() {
            RenderFrameTarget::Image(image) => image,
            RenderFrameTarget::Window(_) => panic!("test packet must target an image"),
        }
    }
}
