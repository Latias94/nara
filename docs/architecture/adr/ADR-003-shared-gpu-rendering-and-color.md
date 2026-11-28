# ADR-003 Shared GPU, rendering, and color

**Accepted on 2026-11-24**

Consolidate the WGPU backend, materials, images, sprites, sprite animation, and tilemaps under
`nara_render`, with a separate material derive crate. Keep Naga validation, `encase`, and immutable
extracted frames. A `RenderPass` declares its attachment reads and writes and records into the
frame's WGPU encoder.

## Device and frames

- The editor startup requests the union of renderer features and limits. `SharedWgpuContext`
  carries the shared `wgpu::Device` and `Queue` and one `Arc<DeviceErrorState>`. Install the device
  loss and uncaptured error callbacks once when creating the device. Both renderers observe that
  error state. Nara accepts the application device and observes its callbacks through a WGPU-only
  interface, so the standalone game runtime keeps its own device lifecycle and dependency graph.
- Reuse transient render targets with `TextureCache` and frame buffers with `FrameRingBuffer`.
  Rebuild `DirtyTileChunk` GPU data only for affected chunks. Keep synchronous pipeline validation
  out of frame submission and return device failures through the normal surface lifecycle.
- The window and offscreen targets use the same renderer. `SpriteTilePass` clears and draws the
  scene. Optional `PostProcess2d` effects use pooled linear textures and composite into the final
  attachment before `HudPass` loads it in place. The window driver publishes HUD hit geometry
  after presenting the matching swapchain image. An explicit virtual-resolution `WorldCanvas`
  retains its nearest-sampled composite and letterbox when effects are enabled.
- `Renderer::set_editor_overlay` accepts the Phase 4 `EditorOverlayPass` after the scene and HUD.
  The overlay loads and writes the final color attachment before submission. Submit Nara's work
  before GPUI's sampling pass on the same queue. Pass the pooled `WgpuImageOutput` through
  `WgpuAtlas::import_image` as `owner: Arc<_>`. The producer and the sampler keep the image alive
  through their respective `on_submitted_work_done` callbacks. When all pooled targets are in use,
  the viewport keeps its current image and retries when it can render another frame.
- Reuse the imported image while the viewport is unchanged. When only the image changes, replay
  retained UI geometry with the updated texture view. Schedule real-time viewports independently
  of otherwise idle editor chrome.

## Color

Nara renders in linear space and exports `Rgba8UnormSrgb` viewport textures with
`RENDER_ATTACHMENT | TEXTURE_BINDING` usage. HDR can use a linear `Rgba16Float` intermediate and
tone-map into the same import format.

The atlas import declares sRGB transfer and premultiplied alpha for translucent content. An opaque
viewport may use straight alpha with alpha of one. Editor chrome composites in
`WgpuSceneColorSpace::SrgbEncoded`, matching the Design Kit's colors, one-pixel borders, and glyphs.
Qualify the import and present conversions together to avoid a second gamma conversion.
