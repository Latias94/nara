#[cfg(feature = "project-source")]
mod headless;
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
mod ordinary;
mod root;

#[cfg(feature = "project-source")]
const PRODUCT_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(feature = "project-source")]
pub use headless::HeadlessProductRun;
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use ordinary::DesktopProductLaunch;
#[doc(hidden)]
pub use root::{open_explicit_product_project_root, open_product_project_root};
