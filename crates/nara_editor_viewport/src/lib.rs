//! Native editor viewport backed by Nara's scene renderer and Open-GPUI's imported images.

mod scene;
#[cfg(test)]
mod tests;

use std::{io, sync::Arc};

use nara_app::{AddPluginsError, AppRunError, PluginError, RuntimeDriveError, RuntimeScopeError};
use nara_asset::{AssetError, AssetStateError, image::ImageDecodeError};
use nara_core::{Affine2, EntityUuid, Vec2, vfs::DirAssetSource};
use nara_editor_core::viewport::{PickCandidate2D, ViewportCamera2D};
use nara_editor_gizmo::{EditorOverlayPass, OverlayFrame, OverlayState, OverlayStroke};
use nara_render::{
    Extent2d, RenderFramePacketError,
    gpu::{
        GpuContext, GpuDeviceErrors, Renderer, TextureCacheStats, WgpuImageOutput,
        WgpuNativeWorkStats, WgpuRenderError,
    },
    sprite_render::SpriteRenderStats,
};
use nara_scene::authoring::{SceneComponentError, SceneDocument, SceneDocumentError};
use open_gpui::{
    Bounds, DevicePixels, ImageTexture, ImageTextureAlpha, ImageTextureSampling,
    ImageTextureTransfer, Pixels, Window, WindowId, point, size,
};
use open_gpui_wgpu::{DeviceErrorState, SharedWgpuContext, WgpuAtlas, WgpuImportedImageDescriptor};

use scene::PreviewScene;

#[derive(Debug, thiserror::Error)]
pub enum ViewportError {
    #[error(transparent)]
    Document(#[from] SceneDocumentError),
    #[error("entity {entity} has an invalid {component} component: {source}")]
    Component {
        entity: EntityUuid,
        component: &'static str,
        #[source]
        source: SceneComponentError,
    },
    #[error("could not read preview PNG '{path}': {source}")]
    ReadImage {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("could not decode preview PNG '{path}': {source}")]
    DecodeImage {
        path: String,
        #[source]
        source: ImageDecodeError,
    },
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error(transparent)]
    AssetState(#[from] AssetStateError),
    #[error(transparent)]
    AddPlugins(#[from] AddPluginsError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    Start(#[from] AppRunError),
    #[error(transparent)]
    Scope(#[from] RuntimeScopeError),
    #[error("preview extraction failed: {:?}", .0.fault())]
    Drive(#[from] RuntimeDriveError),
    #[error("the logical panel and display scale do not yield a valid physical extent")]
    InvalidExtent,
    #[error("the viewport camera cannot be projected into the physical target")]
    InvalidCamera,
    #[error("the preview scene did not extract its camera")]
    MissingCamera,
    #[error(transparent)]
    Packet(#[from] RenderFramePacketError),
    #[error(transparent)]
    Render(#[from] WgpuRenderError),
    #[error("the window has no native WGPU atlas")]
    MissingAtlas,
    #[error("the renderer did not export a usable sRGB viewport texture")]
    InvalidOutput,
    #[error("Open-GPUI could not import or paint the viewport: {0:#}")]
    Gpui(#[source] anyhow::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOutcome {
    /// A new registration requests a GPUI draw so its scene can paint the handle.
    Imported,
    /// The existing registration requests a retained presentation.
    Updated,
    /// The previous image remains available. Retry from the next frame callback.
    TargetsBusy,
}

/// Cumulative work for one viewport, plus facts from its last successful Nara submission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewportStats {
    pub rendered_frames: u64,
    pub targets_busy: u64,
    pub image_imports: u64,
    pub image_updates: u64,
    pub retained_presentations: u64,
    pub created_texture_calls: u64,
    pub created_buffer_calls: u64,
    pub last_work: WgpuNativeWorkStats,
    pub texture_cache: TextureCacheStats,
    pub extraction: SpriteRenderStats,
}

/// One scene pane and its current window-local image registration.
///
/// Create every destination window with the same shared context used here. Call [`Self::render`]
/// from the shell's frame callback, and sample the returned handle during ordinary GPUI paint.
pub struct EditorViewport {
    scene: PreviewScene,
    renderer: Renderer,
    overlay: OverlayState,
    image: Option<ImportedImage>,
    picks: Vec<PickCandidate2D>,
    stats: ViewportStats,
}

impl EditorViewport {
    pub fn new(shared: &SharedWgpuContext) -> Result<Self, ViewportError> {
        let mut renderer = Renderer::new(GpuContext::from_shared(
            shared.instance_handle().clone(),
            shared.adapter().clone(),
            shared.device().clone(),
            shared.queue().clone(),
            Arc::new(NaraDeviceErrors(Arc::clone(shared.errors()))),
        ));
        let (pass, overlay) = EditorOverlayPass::new();
        renderer.set_editor_overlay(Some(Box::new(pass)))?;
        Ok(Self {
            scene: PreviewScene::new()?,
            renderer,
            overlay,
            image: None,
            picks: Vec::new(),
            stats: ViewportStats::default(),
        })
    }

    /// Applies an authored edit and updates world geometry for the shell's next overlay frame.
    /// Without a project asset source, textured sprites preview as their solid tint.
    /// An invalid document or unreadable PNG leaves the current preview in place.
    pub fn set_scene(
        &mut self,
        document: &SceneDocument,
        assets: Option<&DirAssetSource>,
    ) -> Result<(), ViewportError> {
        self.scene.set_document(document, assets)
    }

    /// Submits the scene and overlay before the window samples the output on the shared queue.
    pub fn render(
        &mut self,
        window: &mut Window,
        camera: &ViewportCamera2D,
        overlay: &OverlayFrame,
    ) -> Result<RenderOutcome, ViewportError> {
        let atlas = WgpuAtlas::for_window(window).ok_or(ViewportError::MissingAtlas)?;
        self.observe_with_atlas(window, &atlas);
        let extent = physical_extent(camera, window.scale_factor())?;
        let input = self.scene.render_input(*camera, extent)?;
        self.overlay.update(overlay.clone());
        let (output, report) = match self.renderer.render_to_image(&input) {
            Err(WgpuRenderError::TargetsBusy) => {
                self.stats.targets_busy += 1;
                return Ok(RenderOutcome::TargetsBusy);
            }
            result => result?,
        };
        self.stats.rendered_frames += 1;
        self.stats.last_work = report.work;
        self.stats.created_texture_calls += report.work.created_texture_calls;
        self.stats.created_buffer_calls += report.work.created_buffer_calls;
        self.stats.texture_cache = self.renderer.texture_cache_stats();
        self.stats.extraction = self.scene.extraction();

        let descriptor = import_descriptor(output)?;
        let window_id = window.window_handle().window_id();
        let outcome = if let Some(image) = &mut self.image
            && image.matches(window_id, &atlas)
            && image.extent == extent
        {
            atlas
                .update_image(&image.texture, descriptor)
                .map_err(ViewportError::Gpui)?;
            image.pending_replay = Some(PresentationSample::from_window(window));
            window.request_retained_presentation();
            self.stats.image_updates += 1;
            RenderOutcome::Updated
        } else {
            let texture = atlas
                .import_image(descriptor)
                .map_err(ViewportError::Gpui)?;
            self.image = Some(ImportedImage {
                texture,
                atlas,
                window: window_id,
                extent,
                pending_replay: None,
            });
            window.refresh();
            self.stats.image_imports += 1;
            RenderOutcome::Imported
        };
        self.scene.collect_picks(&mut self.picks);
        Ok(outcome)
    }

    /// Returns the current handle for its owning window and atlas epoch.
    pub fn texture(&self, window: &Window) -> Option<&ImageTexture> {
        let image = self.image.as_ref()?;
        let atlas = WgpuAtlas::for_window(window)?;
        image
            .matches(window.window_handle().window_id(), &atlas)
            .then_some(&image.texture)
    }

    /// Paints the latest available image during the GPUI paint phase.
    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) -> Result<(), ViewportError> {
        if let Some(image) = self.texture(window) {
            window
                .paint_image_texture(bounds, Default::default(), image)
                .map_err(ViewportError::Gpui)?;
        }
        Ok(())
    }

    /// Samples retained presentation after a native frame, including when the pane becomes idle.
    pub fn observe_presentation(&mut self, window: &Window) {
        if let Some(atlas) = WgpuAtlas::for_window(window) {
            self.observe_with_atlas(window, &atlas);
        }
    }

    fn observe_with_atlas(&mut self, window: &Window, atlas: &Arc<WgpuAtlas>) {
        if let Some(image) = &mut self.image
            && image.matches(window.window_handle().window_id(), atlas)
            && let Some(previous) = image.pending_replay
        {
            let current = PresentationSample::from_window(window);
            if current.revision != previous.revision || current.submissions > previous.submissions {
                image.pending_replay = None;
                if current.revision == previous.revision {
                    self.stats.retained_presentations += 1;
                }
            }
        }
    }

    #[must_use]
    pub const fn stats(&self) -> ViewportStats {
        self.stats
    }

    /// Bounds of visible authored sprites, independent of camera culling or texture publication.
    /// An empty selection includes the whole scene.
    pub fn scene_bounds(&self, selection: &[EntityUuid]) -> Option<[Vec2; 2]> {
        self.scene.scene_bounds(selection)
    }

    /// The last installed image's visible sprite quads, in exact back-to-front draw order.
    pub fn pick_candidates(&self) -> &[PickCandidate2D] {
        &self.picks
    }

    /// The current scene's visible sprite quad, including while its image awaits submission.
    pub fn selection_quad(&self, entity: EntityUuid) -> Option<PickCandidate2D> {
        self.scene.selection_quad(entity)
    }

    pub fn world_affine(&self, entity: EntityUuid) -> Option<Affine2> {
        self.scene.world_affine(entity)
    }

    /// Returns the parent frame, or identity for a known root entity.
    pub fn parent_affine(&self, entity: EntityUuid) -> Option<Affine2> {
        self.scene.parent_affine(entity)
    }

    /// Appends the visible authored collider outlines in world coordinates.
    pub fn append_colliders(&self, frame: &mut OverlayFrame, stroke: OverlayStroke) {
        self.scene.append_colliders(frame, stroke);
    }
}

struct ImportedImage {
    texture: ImageTexture,
    atlas: Arc<WgpuAtlas>,
    window: WindowId,
    extent: Extent2d,
    pending_replay: Option<PresentationSample>,
}

impl ImportedImage {
    fn matches(&self, window: WindowId, atlas: &Arc<WgpuAtlas>) -> bool {
        self.window == window && Arc::ptr_eq(&self.atlas, atlas) && self.texture.validate().is_ok()
    }
}

#[derive(Clone, Copy)]
struct PresentationSample {
    revision: u64,
    submissions: u64,
}

impl PresentationSample {
    fn from_window(window: &Window) -> Self {
        Self {
            revision: window.rendered_frame_revision(),
            submissions: window.present_submission_count(),
        }
    }
}

struct NaraDeviceErrors(Arc<DeviceErrorState>);

impl GpuDeviceErrors for NaraDeviceErrors {
    fn observe_error(&self, cursor: &mut u64) -> Option<String> {
        self.0.observe_error(cursor)
    }

    fn on_device_lost(&self, observer: &Arc<dyn Fn() + Send + Sync>) {
        self.0.on_device_lost(observer);
    }
}

fn physical_extent(camera: &ViewportCamera2D, scale: f32) -> Result<Extent2d, ViewportError> {
    let physical = (camera.panel_size().as_vec2() * scale).round();
    if physical.min_element() < 1.0
        || !physical.is_finite()
        || f64::from(physical.max_element()) > f64::from(i32::MAX)
    {
        return Err(ViewportError::InvalidExtent);
    }
    Extent2d::new(physical.x as u32, physical.y as u32).ok_or(ViewportError::InvalidExtent)
}

fn import_descriptor(
    output: WgpuImageOutput,
) -> Result<WgpuImportedImageDescriptor, ViewportError> {
    let owner = Arc::new(output);
    let texture = owner.native_texture().ok_or(ViewportError::InvalidOutput)?;
    if texture.format() != WgpuImageOutput::FORMAT
        || !texture
            .usage()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(ViewportError::InvalidOutput);
    }
    let extent = owner.extent();
    let width = i32::try_from(extent.width).map_err(|_| ViewportError::InvalidOutput)?;
    let height = i32::try_from(extent.height).map_err(|_| ViewportError::InvalidOutput)?;
    Ok(WgpuImportedImageDescriptor {
        texture: texture.clone(),
        mip_level: 0,
        array_layer: 0,
        region: Bounds::new(
            point(DevicePixels(0), DevicePixels(0)),
            size(DevicePixels(width), DevicePixels(height)),
        ),
        transfer: ImageTextureTransfer::Srgb,
        alpha: ImageTextureAlpha::Premultiplied,
        sampling: ImageTextureSampling::Linear,
        owner: Some(owner),
    })
}
