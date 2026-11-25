# ADR-003 Shared GPU, rendering, and color

**Accepted on 2026-11-24**

Consolidate the WGPU backend, materials, images, sprites, sprite animation, and tilemaps under
`nara_render`, with a separate material derive crate. Keep Naga validation, `encase`, and immutable
extracted frames. Declare `RenderPass` attachments so a future graph can order them by dependency.

## Device and frames

- The editor startup requests the union of renderer features and limits. `SharedWgpuContext`
  carries the shared `wgpu::Device` and `Queue` and one `Arc<DeviceErrorState>`. Install the device
  loss and uncaptured error callbacks once when creating the device. Both renderers observe that
  error state. Nara accepts the application device and observes its callbacks through a WGPU-only
  interface, so the standalone game runtime keeps its own device lifecycle and dependency graph.
- Reuse transient render targets with `TextureCache` and frame buffers with `FrameRingBuffer`.
  Rebuild `DirtyTileChunk` GPU data only for affected chunks. Keep synchronous pipeline validation
  out of frame submission and return device failures through the normal surface lifecycle.
- Draw the game scene directly into its swapchain image. Clear it in the scene pass and use
  `LoadOp::Load` for the game UI. Present after both passes, with zero full-screen copies or blits.
  An explicit virtual-resolution `WorldCanvas` or a custom policy may require an intermediate
  image. The stock surface path requires a clear followed by an optional in-place HUD pass.
- Render editor viewports offscreen with the `EditorOverlayPass` in the same frame. Submit Nara's
  work before GPUI's sampling pass on the same queue. Pass the pooled target lease through
  `WgpuAtlas::import_image` as `owner: Arc<_>` and release it after the last GPU submission using it
  completes via `on_submitted_work_done`.
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
