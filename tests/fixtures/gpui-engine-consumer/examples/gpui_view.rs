//! Nara-owned GPU composition with a hosted GPUI root and the public Editor preview workflow.

#[cfg(windows)]
#[path = "gpui_view/native.rs"]
mod native;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    return native::run();
    #[cfg(not(windows))]
    anyhow::bail!("the GPUI engine-view qualification currently requires Windows");
}
