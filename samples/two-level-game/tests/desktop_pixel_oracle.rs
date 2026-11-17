//! CPU-only controls registered by the shared desktop oracle, without a native window or GPU.

#![cfg(feature = "desktop")]
#![forbid(unsafe_code)]

#[path = "support/desktop_capture.rs"]
#[allow(dead_code, unused_imports)]
mod desktop_capture;
