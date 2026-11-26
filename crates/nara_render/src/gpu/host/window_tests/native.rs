use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window as NativeWindow, WindowId as NativeWindowId},
};

use super::*;

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn configured_primary_retires_before_first_present() {
    with_native_event_loop(|event_loop| {
        let (_window, packet, mut host) = native_host(event_loop);
        host.begin_window_frame(packet, 0).unwrap();
        let extent = Extent2d::new(
            packet.resolution.physical_width,
            packet.resolution.physical_height,
        )
        .unwrap();
        assert_eq!(
            host.backend.configured_surface_extent(packet.id),
            Some(extent)
        );
        host.wait_for_idle().unwrap();
        let stats = host.frame_transaction_stats();
        assert_eq!(stats.surface_acquire_attempts(), 0);
        assert_eq!(stats.presents(), 0);
        drop(host);
    });
}

fn with_native_event_loop(check: fn(&ActiveEventLoop)) {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut app = WindowTestApp {
        check,
        result: None,
    };
    event_loop.run_app(&mut app).unwrap();
    if let Err(failure) = app.result.expect("the native window test must run") {
        resume_unwind(failure);
    }
}

struct WindowTestApp {
    check: fn(&ActiveEventLoop),
    result: Option<std::thread::Result<()>>,
}

impl ApplicationHandler for WindowTestApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return;
        }
        self.result = Some(catch_unwind(AssertUnwindSafe(|| (self.check)(event_loop))));
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: NativeWindowId,
        _event: WindowEvent,
    ) {
    }
}

fn native_host(
    event_loop: &ActiveEventLoop,
) -> (Arc<NativeWindow>, RenderWindowPacket, WgpuRenderHost) {
    let window = Arc::new(
        event_loop
            .create_window(
                NativeWindow::default_attributes()
                    .with_title("Nara native configured surface retirement")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(320, 240)),
            )
            .unwrap(),
    );
    let packet = RenderWindowPacket {
        id: WindowId::PRIMARY,
        resolution: nara_window::WindowResolution::new(
            window.inner_size().width,
            window.inner_size().height,
        )
        .with_scale_factor(window.scale_factor()),
        present_mode: nara_window::PresentMode::AutoNoVsync,
    };
    let host =
        WgpuRenderHost::new(packet.id, WindowHandleProvider::new(Arc::clone(&window))).unwrap();
    (window, packet, host)
}
