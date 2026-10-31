use engine::{Presentation2dPlugins, Runtime2dPlugins};

pub fn product_groups() -> (Runtime2dPlugins, Presentation2dPlugins) {
    (Runtime2dPlugins, Presentation2dPlugins)
}

/// The code-first native owner does not require source serialization or a project file.
#[cfg(feature = "desktop")]
pub fn desktop_run(runtime: engine::app::RuntimeInstance) -> engine::project_host::DesktopRun {
    engine::project_host::DesktopRun::from_runtime(
        runtime,
        engine::project_host::DesktopRunOptions::new(),
    )
}
