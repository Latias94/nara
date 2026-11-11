use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    thread::{ThreadId, current},
};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window as NativeWindow, WindowId as NativeWindowId},
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attempt {
    Success,
    PaintRefusal,
    PaintPanic,
    WriteValidation,
    EncoderValidation,
    FinalizePanic,
    SubmitValidation,
    SubmitPanic,
    PrePresentPanic,
}

struct LocalRecording {
    _buffer: wgpu::Buffer,
    drops: Rc<Cell<usize>>,
    owner: ThreadId,
}

impl Drop for LocalRecording {
    fn drop(&mut self) {
        assert_eq!(current().id(), self.owner);
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_window_attempt_observation_keeps_local_custody_across_every_terminal_path() {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut app = CustodyApp { result: None };
    event_loop.run_app(&mut app).unwrap();
    if let Err(failure) = app.result.expect("the native custody trajectory must run") {
        resume_unwind(failure);
    }
}

struct CustodyApp {
    result: Option<std::thread::Result<()>>,
}

impl ApplicationHandler for CustodyApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_some() {
            return;
        }
        self.result = Some(catch_unwind(AssertUnwindSafe(|| {
            verify_custody(event_loop)
        })));
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

fn verify_custody(event_loop: &ActiveEventLoop) {
    let window = Arc::new(
        event_loop
            .create_window(
                NativeWindow::default_attributes()
                    .with_title("Nara native window attempt custody")
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
    let mut host =
        WgpuRenderHost::new(packet.id, WindowHandleProvider::new(Arc::clone(&window))).unwrap();
    for (frame_index, attempt) in [
        Attempt::Success,
        Attempt::PaintRefusal,
        Attempt::PaintPanic,
        Attempt::WriteValidation,
        Attempt::EncoderValidation,
        Attempt::FinalizePanic,
        Attempt::SubmitValidation,
        Attempt::SubmitPanic,
        Attempt::PrePresentPanic,
    ]
    .into_iter()
    .enumerate()
    {
        let frame_index = frame_index as u64 * 4 + 1;
        host.begin_frame(frame_index).unwrap();
        let namespace = host.prepare_namespace(Some(packet)).unwrap();
        let buffer = namespace.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("native-window-local-recording-upload"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let drops = Rc::new(Cell::new(0));
        let recording = LocalRecording {
            _buffer: buffer.clone(),
            drops: Rc::clone(&drops),
            owner: current().id(),
        };
        let mut custody = None;
        let presents = host.frame_transaction_stats().presents();
        let outcome = host.present_window(
            packet,
            frame_index,
            &[],
            |frame| {
                let original = frame.submission_prefix();
                assert!(original.submission_index().is_none());
                assert!(!original.poll().unwrap());
                assert!(!original.recover_abandoned().unwrap());
                custody = Some((original, recording));
                frame.encode(|device, queue, encoder, target| {
                    queue.write_buffer(&buffer, 0, &[37; 16]);
                    {
                        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("native-window-custody-clear"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: target,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                    }
                    match attempt {
                        Attempt::WriteValidation => queue.write_buffer(&buffer, 16, &[0; 4]),
                        Attempt::EncoderValidation | Attempt::SubmitValidation => {
                            let invalid = device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("native-window-custody-invalid-command"),
                                size: 16,
                                usage: if attempt == Attempt::EncoderValidation {
                                    wgpu::BufferUsages::COPY_SRC
                                } else {
                                    wgpu::BufferUsages::COPY_DST
                                },
                                mapped_at_creation: false,
                            });
                            encoder.clear_buffer(&invalid, 0, None);
                            if attempt == Attempt::SubmitValidation {
                                invalid.destroy();
                            }
                        }
                        _ => {}
                    }
                });
                match attempt {
                    Attempt::PaintRefusal => {
                        return Err(WgpuRenderHostError::window_paint("injected paint refusal"));
                    }
                    Attempt::PaintPanic => panic!("injected paint panic after upload"),
                    Attempt::FinalizePanic => WINDOW_SUBMISSION_FAILURE
                        .with(|failure| failure.set(Some(WindowSubmissionStage::Finalize))),
                    Attempt::SubmitPanic => WINDOW_SUBMISSION_FAILURE
                        .with(|failure| failure.set(Some(WindowSubmissionStage::Submit))),
                    _ => {}
                }
                Ok(())
            },
            || {
                if attempt == Attempt::PrePresentPanic {
                    panic!("injected pre-present panic");
                }
                window.pre_present_notify();
            },
        );
        match attempt {
            Attempt::Success => assert!(outcome.unwrap().window().is_some()),
            Attempt::PaintRefusal => assert!(matches!(
                outcome,
                Err(WgpuRenderHostError::WindowPaint { .. })
            )),
            Attempt::PaintPanic => assert!(matches!(
                outcome,
                Err(WgpuRenderHostError::NativeCallbackPanicked {
                    stage: "window paint"
                })
            )),
            Attempt::PrePresentPanic => assert!(matches!(
                outcome,
                Err(WgpuRenderHostError::NativeCallbackPanicked {
                    stage: "pre-present"
                })
            )),
            Attempt::FinalizePanic | Attempt::SubmitPanic => assert!(matches!(
                outcome,
                Err(WgpuRenderHostError::NativeExecution { .. })
            )),
            Attempt::WriteValidation | Attempt::EncoderValidation | Attempt::SubmitValidation => {
                assert!(matches!(
                    outcome,
                    Err(WgpuRenderHostError::Backend(
                        WgpuRenderError::ContentRejected { .. }
                    ))
                ));
            }
        }
        assert_eq!(
            host.frame_transaction_stats().presents(),
            presents + u64::from(attempt == Attempt::Success),
            "only an actual successful presentation publishes a receipt: {attempt:?}"
        );
        assert_eq!(drops.get(), 0, "local custody survived {attempt:?}");
        let (original, recording) = custody.take().unwrap();
        let bound = host.native_prefix.as_ref().unwrap();
        if matches!(attempt, Attempt::FinalizePanic | Attempt::SubmitPanic) {
            assert!(original.submission_index().is_none());
            assert!(bound.submission_index().is_none());
        } else {
            assert!(std::ptr::eq(
                original.submission_index().unwrap(),
                bound.submission_index().unwrap(),
            ));
        }
        let frozen_index = original.submission_index().map(std::ptr::from_ref);
        let (_, successor) = host.submit_image_use(&[], |_, _, _, _| Ok(())).unwrap();
        assert_eq!(
            original.submission_index().map(std::ptr::from_ref),
            frozen_index
        );
        assert!(successor.submission_index().is_some());
        let polling = original.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !polling.poll().unwrap() {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        })
        .join()
        .unwrap();
        assert!(original.is_complete());
        assert!(!original.callback_abandoned());
        assert_eq!(
            drops.get(),
            0,
            "native completion never drops a local recording"
        );
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            namespace.device.poll(wgpu::PollType::Poll).unwrap();
            match receiver.try_recv() {
                Ok(result) => {
                    result.unwrap();
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                Err(error) => panic!("native upload readback disappeared: {error}"),
            }
        }
        assert_eq!(&*buffer.get_mapped_range(..).unwrap(), &[37; 16]);
        buffer.unmap();
        drop(recording);
        assert_eq!(drops.get(), 1);
        host.maintain_producers().unwrap();
        assert_eq!(host.cleanup_fault_count(), 0);
        for recovery in 1..=3 {
            host.begin_frame(frame_index + recovery).unwrap();
            let outcome = host
                .present_window(
                    packet,
                    frame_index + recovery,
                    &[],
                    |frame| {
                        frame.encode(|_, _, encoder, target| {
                            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("native-window-success-after-refused-acquisition"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: target,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                ..Default::default()
                            });
                        });
                        Ok(())
                    },
                    || window.pre_present_notify(),
                )
                .unwrap();
            assert!(
                outcome.window().is_some(),
                "surface did not recover after {attempt:?}"
            );
        }
    }
}
