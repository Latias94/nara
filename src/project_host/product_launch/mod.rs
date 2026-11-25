#[cfg(all(feature = "runtime-2d", feature = "serde"))]
mod headless;
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
mod ordinary;
mod root;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
const PRODUCT_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use headless::HeadlessProductRun;
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use ordinary::DesktopProductLaunch;
#[doc(hidden)]
pub use root::{open_explicit_product_project_root, open_product_project_root};
