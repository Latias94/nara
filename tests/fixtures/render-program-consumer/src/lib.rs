//! Independent consumer of the production render program and Feature API.
//!
//! Semantic texture ports remain distinct even when their native descriptors match:
//!
//! ```compile_fail,E0308
//! use nara_render_wgpu::WgpuProgramTexture;
//! struct SceneColor;
//! struct CompositeColor;
//! fn scene_input(_: &WgpuProgramTexture<SceneColor>) {}
//! fn wrong_input(composite: &WgpuProgramTexture<CompositeColor>) {
//!     scene_input(composite);
//! }
//! ```
//!
//! Raw integer statistics cannot satisfy a normalized frequency input:
//!
//! ```compile_fail,E0308
//! use nara_render_wgpu::WgpuProgramBuffer;
//! struct RawCounts;
//! struct Frequencies;
//! fn histogram_input(_: &WgpuProgramBuffer<Frequencies>) {}
//! fn wrong_input(raw: &WgpuProgramBuffer<RawCounts>) {
//!     histogram_input(raw);
//! }
//! ```
#![forbid(unsafe_code)]
