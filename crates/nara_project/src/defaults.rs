pub(crate) fn default_assets_path() -> String {
    "assets".to_owned()
}

pub(crate) fn default_scenes_path() -> String {
    "scenes".to_owned()
}

pub(crate) fn default_prefabs_path() -> String {
    "prefabs".to_owned()
}

pub(crate) fn default_scripts_path() -> String {
    "scripts".to_owned()
}

pub(crate) fn default_import_cache_path() -> String {
    ".nara/import-cache".to_owned()
}

pub(crate) const fn default_time_scale() -> f32 {
    1.0
}

pub(crate) const fn default_max_delta_seconds() -> f64 {
    0.25
}

pub(crate) const fn default_fixed_timestep_seconds() -> f64 {
    1.0 / 60.0
}

pub(crate) const fn default_max_fixed_steps_per_frame() -> u32 {
    5
}

pub(crate) const fn default_io_threads() -> usize {
    2
}

pub(crate) fn default_compute_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

pub(crate) fn default_async_compute_threads() -> usize {
    (default_compute_threads() / 2).max(1)
}

pub(crate) const fn default_window_enabled() -> bool {
    true
}

pub(crate) fn default_window_title() -> String {
    "nara".to_owned()
}

pub(crate) const fn default_window_width() -> u32 {
    1280
}

pub(crate) const fn default_window_height() -> u32 {
    720
}

pub(crate) const fn default_window_scale_factor() -> f64 {
    1.0
}

pub(crate) const fn default_window_resizable() -> bool {
    true
}

pub(crate) const fn default_diagnostics_capacity() -> usize {
    256
}
