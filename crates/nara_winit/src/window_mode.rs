use nara_app::AppRunError;
use nara_window::{Window, WindowMode};
use winit::{dpi::PhysicalSize, monitor::MonitorHandle, window::Fullscreen};

pub(super) fn initial_fullscreen(
    window: &Window,
    primary_monitor: Option<MonitorHandle>,
) -> Result<Option<Fullscreen>, AppRunError> {
    match window.mode {
        WindowMode::Windowed => Ok(None),
        WindowMode::BorderlessFullscreen => Ok(Some(Fullscreen::Borderless(primary_monitor))),
        WindowMode::Fullscreen => {
            let monitor = primary_monitor.ok_or_else(|| {
                AppRunError::runner("exclusive fullscreen requires an available primary monitor")
            })?;
            let extent = PhysicalSize::new(
                window.resolution.physical_width,
                window.resolution.physical_height,
            );
            let modes: Vec<_> = monitor.video_modes().collect();
            let index = select_exclusive_mode(
                extent,
                modes.iter().map(|mode| VideoModeDescription {
                    extent: mode.size(),
                    refresh_rate_millihertz: mode.refresh_rate_millihertz(),
                    bit_depth: mode.bit_depth(),
                }),
            )?;
            Ok(Some(Fullscreen::Exclusive(modes[index].clone())))
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VideoModeDescription {
    extent: PhysicalSize<u32>,
    refresh_rate_millihertz: u32,
    bit_depth: u16,
}

fn select_exclusive_mode(
    extent: PhysicalSize<u32>,
    modes: impl IntoIterator<Item = VideoModeDescription>,
) -> Result<usize, AppRunError> {
    modes
        .into_iter()
        .enumerate()
        .filter(|(_, mode)| mode.extent == extent)
        .max_by_key(|(_, mode)| (mode.refresh_rate_millihertz, mode.bit_depth))
        .map(|(index, _)| index)
        .ok_or_else(|| {
            AppRunError::runner(format!(
                "primary monitor has no exclusive fullscreen video mode matching {}x{} physical pixels",
                extent.width, extent.height,
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_window::WindowResolution;

    fn mode(width: u32, height: u32, refresh: u32, depth: u16) -> VideoModeDescription {
        VideoModeDescription {
            extent: PhysicalSize::new(width, height),
            refresh_rate_millihertz: refresh,
            bit_depth: depth,
        }
    }

    #[test]
    fn exclusive_selection_requires_exact_extent_before_comparing_quality() {
        let modes = [
            mode(1920, 1080, 240_000, 32),
            mode(1280, 800, 144_000, 32),
            mode(1280, 720, 60_000, 24),
        ];

        assert_eq!(
            select_exclusive_mode(PhysicalSize::new(1280, 720), modes).unwrap(),
            2,
        );
    }

    #[test]
    fn exclusive_selection_prefers_refresh_then_bit_depth() {
        let modes = [
            mode(1280, 720, 60_000, 32),
            mode(1280, 720, 144_000, 24),
            mode(1280, 720, 120_000, 48),
            mode(1280, 720, 144_000, 32),
        ];

        assert_eq!(
            select_exclusive_mode(PhysicalSize::new(1280, 720), modes).unwrap(),
            3,
        );
    }

    #[test]
    fn exclusive_selection_rejects_missing_or_nonmatching_modes() {
        let extent = PhysicalSize::new(1280, 720);
        for modes in [Vec::new(), vec![mode(1920, 1080, 60_000, 32)]] {
            let error = select_exclusive_mode(extent, modes).unwrap_err();
            assert!(matches!(error, AppRunError::Runner { .. }));
            assert!(error.to_string().contains("1280x720 physical pixels"));
        }
    }

    #[test]
    fn exclusive_requires_a_primary_monitor_without_downgrading() {
        let mut window = Window::new("exclusive", WindowResolution::new(1280, 720));
        window.mode = WindowMode::Fullscreen;

        let error = initial_fullscreen(&window, None).unwrap_err();
        assert!(matches!(error, AppRunError::Runner { .. }));
        assert!(error.to_string().contains("primary monitor"));
    }

    #[test]
    fn windowed_and_borderless_do_not_require_a_primary_monitor() {
        let mut window = Window::default();
        assert_eq!(initial_fullscreen(&window, None).unwrap(), None);

        window.mode = WindowMode::BorderlessFullscreen;
        assert_eq!(
            initial_fullscreen(&window, None).unwrap(),
            Some(Fullscreen::Borderless(None)),
        );
    }
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;
