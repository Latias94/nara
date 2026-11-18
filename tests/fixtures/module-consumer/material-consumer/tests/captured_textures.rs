//! Independent F4 regression through stock capture, the original GPU Host and image readback.
//!
//! Default tests exercise only the exact pixel oracle. Native tests require the existing
//! `native-source-watch-gpu` feature, `WGPU_BACKEND=dx12`, explicit
//! `NARA_WGPU_FORCE_FALLBACK=0` for hardware or `1` for the software control,
//! and explicit ignored-test execution. No filesystem watcher is installed.

#[path = "captured_textures/expected.rs"]
mod expected;

#[cfg(feature = "native-source-watch-gpu")]
#[allow(dead_code)]
#[path = "persistent_workflow/project.rs"]
mod project;

#[cfg(feature = "native-source-watch-gpu")]
#[path = "captured_textures/native.rs"]
mod native;
