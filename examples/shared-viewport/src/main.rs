mod ime;
mod scene;
mod smoke;

use std::{cell::RefCell, env, rc::Rc, sync::Arc};

use anyhow::{Context as _, Result, bail, ensure};
use nara_render::Extent2d;
use nara_render::gpu::{
    GpuContext, GpuDeviceErrors, TextureAlpha, TextureInterpretation, TextureTransfer,
    WgpuImageOutput, WgpuRenderHost, WgpuRenderHostError, WgpuRenderPolicy, WgpuStockPolicy,
};
use open_gpui::{
    App, Bounds, ClickEvent, Context, DevicePixels, Entity, ImageTexture, ImageTextureAlpha,
    ImageTextureSampling, ImageTextureTransfer, Pixels, QuitMode, TargetedEvent, TitlebarOptions,
    Window, WindowBounds, WindowHandle, WindowInitialPresentationStatus, WindowKind, WindowOptions,
    canvas, div, point, prelude::*, px, relative, rgb, size,
};
use open_gpui_wgpu::{DeviceErrorState, SharedWgpuContext, WgpuAtlas, WgpuImportedImageDescriptor};

use crate::{ime::ImeField, scene::Scene};

type Failure = Rc<RefCell<Option<anyhow::Error>>>;

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let smoke = match args.as_slice() {
        [] => false,
        [flag] if flag == "--smoke" => true,
        [flag] if flag == "--help" || flag == "-h" => {
            println!("Usage: nara-shared-viewport [--smoke]");
            return Ok(());
        }
        _ => bail!("Usage: nara-shared-viewport [--smoke]"),
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let shared = pollster::block_on(create_shared_gpu())?;
    let host = WgpuRenderHost::with_gpu_context(GpuContext::from_shared(
        shared.instance_handle().clone(),
        shared.adapter().clone(),
        shared.device().clone(),
        shared.queue().clone(),
        Arc::new(NaraDeviceErrors(Arc::clone(shared.errors()))),
    ))?;
    let scene = Scene::new(host)?;
    let application = open_gpui_platform::application_with_shared_wgpu(shared.clone())?;

    let failure = Failure::default();
    application
        .with_quit_mode(if smoke {
            QuitMode::Explicit
        } else {
            QuitMode::LastWindowClosed
        })
        .run({
            let failure = Rc::clone(&failure);
            move |cx| match open_windows(scene, Rc::clone(&failure), cx) {
                Ok((viewport, inspector)) if smoke => {
                    smoke::start(viewport, inspector, shared, failure, cx);
                }
                Ok(_) => {}
                Err(error) => fail(&failure, error, cx),
            }
        });
    match failure.borrow_mut().take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

async fn create_shared_gpu() -> Result<SharedWgpuContext> {
    let native_backends = if cfg!(target_os = "windows") {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    } else if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::VULKAN
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends &= native_backends;
    #[cfg(target_os = "windows")]
    {
        // GPUI's Windows frame scheduler paces the viewport before surface acquisition.
        descriptor.backend_options.dx12.latency_waitable_object =
            wgpu::Dx12UseFrameLatencyWaitableObject::DontWait.with_env();
    }
    ensure!(
        !descriptor.backends.is_empty(),
        "WGPU_BACKEND excludes this platform's native backends"
    );
    let backends = descriptor.backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter_name = env::var("WGPU_ADAPTER_NAME")
        .ok()
        .map(|name| name.to_lowercase());
    let (info, adapter) = instance
        .enumerate_adapters(backends)
        .await
        .into_iter()
        .map(|adapter| (adapter.get_info(), adapter))
        .filter(|(info, _)| {
            info.device_type != wgpu::DeviceType::Cpu
                && adapter_name
                    .as_ref()
                    .is_none_or(|name| info.name.to_lowercase().contains(name.as_str()))
        })
        .min_by_key(|(info, _)| {
            let preference = match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 0,
                wgpu::DeviceType::IntegratedGpu => 1,
                _ => 2,
            };
            (preference, info.backend != wgpu::Backend::Dx12)
        })
        .context("no native hardware WGPU adapter matched the backend and adapter filters")?;
    let requirements = WgpuStockPolicy::new().device_requirements();
    let shared = SharedWgpuContext::request_device(
        instance,
        adapter,
        wgpu::DeviceDescriptor {
            label: Some("Nara + Open-GPUI Mode A"),
            required_features: requirements.required_features(),
            required_limits: requirements.required_limits().clone(),
            ..Default::default()
        },
    )
    .await
    .context("request the shared Nara and Open-GPUI device")?;
    println!(
        "Shared GPU: {} ({:?}, driver {} {})",
        info.name, info.backend, info.driver, info.driver_info
    );
    Ok(shared)
}

struct NaraDeviceErrors(Arc<DeviceErrorState>);

impl GpuDeviceErrors for NaraDeviceErrors {
    fn observe_error(&self, cursor: &mut u64) -> Option<String> {
        self.0.observe_error(cursor)
    }

    fn on_device_lost(&self, observer: &Arc<dyn Fn() + Send + Sync>) {
        self.0.on_device_lost(observer);
    }
}

fn open_windows(
    scene: Scene,
    failure: Failure,
    cx: &mut App,
) -> Result<(WindowHandle<Viewport>, WindowHandle<Inspector>)> {
    let layout = Bounds::centered(None, size(px(1000.0), px(480.0)), cx);
    let viewport = cx
        .open_window(
            WindowOptions {
                inactive_frame_interval: None,
                ..window_options(
                    "Nara Scene View",
                    Bounds::new(layout.origin, size(px(640.0), px(480.0))),
                )
            },
            |window, cx| cx.new(|cx| Viewport::new(scene, failure, window, cx)),
        )
        .context("open the native viewport on the shared GPU")?;
    let inspector = cx
        .open_window(
            WindowOptions {
                focus_on_appearing: false,
                ..window_options(
                    "Nara Inspector",
                    Bounds::new(
                        point(layout.origin.x + px(656.0), layout.origin.y),
                        size(px(344.0), px(226.0)),
                    ),
                )
            },
            |_, cx| {
                let input = cx.new(ImeField::new);
                cx.new(|_| Inspector { viewport, input })
            },
        )
        .context("open the native inspector on the shared GPU")?;
    inspector.update(cx, |_, window, _| {
        WgpuAtlas::for_window(window).context("inspector has no native WGPU atlas")
    })??;
    viewport.update(cx, |view, window, cx| view.schedule_frame(window, cx))?;
    cx.activate(true);
    Ok((viewport, inspector))
}

fn window_options(title: &'static str, bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        kind: WindowKind::Normal,
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewportGeometry {
    extent: Extent2d,
    scale: f32,
}

impl ViewportGeometry {
    fn from_window(window: &Window) -> Option<Self> {
        let size = window.viewport_size();
        let scale = window.scale_factor();
        let physical = |length: Pixels| (f32::from(length) * scale).round() as u32;
        let extent = Extent2d::new(physical(size.width), physical(size.height))?;
        Some(Self { extent, scale })
    }
}

struct ViewportImage {
    texture: ImageTexture,
    geometry: ViewportGeometry,
}

struct Viewport {
    image: Option<ViewportImage>,
    scene: Scene,
    failure: Failure,
    live: bool,
    frame_scheduled: bool,
    refresh_requested: bool,
    pending_replay: Option<(u64, u64)>,
    retained_presentations: u64,
}

impl Viewport {
    fn new(scene: Scene, failure: Failure, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_window_bounds(window, Self::schedule_frame)
            .detach();
        cx.observe_window_display(window, Self::schedule_frame)
            .detach();
        cx.observe_window_visibility(window, |view, visibility, window, cx| {
            if visibility.is_visible() {
                view.schedule_frame(window, cx);
            }
        })
        .detach();
        Self {
            image: None,
            scene,
            failure,
            live: true,
            frame_scheduled: false,
            refresh_requested: false,
            pending_replay: None,
            retained_presentations: 0,
        }
    }

    fn schedule_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_scheduled {
            return;
        }
        self.frame_scheduled = true;
        let view = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.frame_scheduled = false;
                if let Err(error) = view.update_image(window) {
                    fail(&view.failure, error, cx);
                } else if (view.live || view.refresh_requested) && window.is_visible() {
                    view.schedule_frame(window, cx);
                }
            });
        });
    }

    fn refresh_once(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_requested = true;
        self.schedule_frame(window, cx);
    }

    fn update_image(&mut self, window: &mut Window) -> Result<()> {
        let revision = window.rendered_frame_revision();
        let submissions = window.present_submission_count();
        if let Some((prior_revision, prior_submissions)) = self.pending_replay.take()
            && prior_revision == revision
            && prior_submissions < submissions
        {
            self.retained_presentations += 1;
        }
        if !window.is_visible()
            && window.presentation_facts().initial_presentation
                == WindowInitialPresentationStatus::Completed
        {
            return Ok(());
        }
        let Some(geometry) = ViewportGeometry::from_window(window) else {
            return Ok(());
        };
        let needs_image = !self
            .image
            .as_ref()
            .is_some_and(|image| image.geometry == geometry && image.texture.validate().is_ok());
        let refresh_requested = std::mem::take(&mut self.refresh_requested);
        if !self.live && !refresh_requested && !needs_image {
            return Ok(());
        }

        let output = match self.scene.render(geometry.extent, geometry.scale) {
            Ok(output) => output,
            Err(error)
                if error
                    .downcast_ref::<WgpuRenderHostError>()
                    .is_some_and(WgpuRenderHostError::is_deferred) =>
            {
                // Keep the last image visible until the shared queue releases enough storage.
                self.refresh_requested = true;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let descriptor = import_descriptor(output)?;
        let atlas = WgpuAtlas::for_window(window).context("viewport has no native WGPU atlas")?;
        if let Some(image) = &mut self.image
            && image.geometry.extent == geometry.extent
            && image.texture.validate().is_ok()
        {
            atlas.update_image(&image.texture, descriptor)?;
            image.geometry = geometry;
            self.pending_replay = Some((revision, submissions));
            window.request_retained_presentation();
        } else {
            self.image = Some(ViewportImage {
                texture: atlas.import_image(descriptor)?,
                geometry,
            });
            println!(
                "Viewport image: {}x{} physical px at {:.2}x",
                geometry.extent.width, geometry.extent.height, geometry.scale
            );
            window.refresh();
        }
        Ok(())
    }
}

impl Render for Viewport {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.as_ref().map(|image| image.texture.clone());
        let failure = Rc::clone(&self.failure);
        div()
            .relative()
            .size_full()
            .bg(rgb(0x243249))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(relative(0.5))
                    .h_full()
                    .bg(rgb(0x4e6081)),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        if let Some(image) = &image
                            && let Err(error) =
                                window.paint_image_texture(bounds, Default::default(), image)
                        {
                            fail(&failure, error.context("paint the retained viewport"), cx);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}

fn import_descriptor(output: WgpuImageOutput) -> Result<WgpuImportedImageDescriptor> {
    let owner = Arc::new(output);
    let texture = owner
        .native_texture()
        .context("Nara viewport texture retired")?;
    ensure!(
        texture.format() == wgpu::TextureFormat::Rgba8UnormSrgb
            && texture.usage().contains(
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            )
            && owner.descriptor().interpretation
                == (TextureInterpretation::Color {
                    transfer: TextureTransfer::Srgb,
                    alpha: TextureAlpha::Premultiplied,
                }),
        "Nara must export an sRGB premultiplied render target"
    );
    let extent = owner.extent();
    Ok(WgpuImportedImageDescriptor {
        texture: texture.clone(),
        mip_level: 0,
        array_layer: 0,
        region: Bounds::new(
            point(DevicePixels(0), DevicePixels(0)),
            size(
                DevicePixels(i32::try_from(extent.width)?),
                DevicePixels(i32::try_from(extent.height)?),
            ),
        ),
        transfer: ImageTextureTransfer::Srgb,
        alpha: ImageTextureAlpha::Premultiplied,
        sampling: ImageTextureSampling::Linear,
        owner: Some(owner),
    })
}

struct Inspector {
    viewport: WindowHandle<Viewport>,
    input: Entity<ImeField>,
}

impl Render for Inspector {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let live = self.viewport.read(cx).is_ok_and(|view| view.live);
        let toggle = if live {
            "Pause viewport"
        } else {
            "Resume viewport"
        };
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x1a2333))
            .text_color(rgb(0xecf3ff))
            .child("Scene name (native IME)")
            .child(self.input.clone())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .child(button(
                        toggle,
                        cx.listener(|view, _, _, cx| {
                            let _ = view.viewport.update(cx, |viewport, window, cx| {
                                viewport.live = !viewport.live;
                                if viewport.live {
                                    viewport.schedule_frame(window, cx);
                                }
                            });
                            cx.notify();
                        }),
                    ))
                    .child(button(
                        "Refresh once",
                        cx.listener(|view, _, _, cx| {
                            let _ = view.viewport.update(cx, Viewport::refresh_once);
                        }),
                    )),
            )
    }
}

fn button(
    label: &'static str,
    click: impl Fn(&TargetedEvent<ClickEvent>, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(rgb(0x31435f))
        .hover(|button| button.bg(rgb(0x40597f)))
        .cursor_pointer()
        .child(label)
        .on_click(click)
}

fn fail(failure: &Failure, error: anyhow::Error, cx: &App) {
    let mut failure = failure.borrow_mut();
    if failure.is_none() {
        eprintln!("Shared viewport failed: {error:#}");
        *failure = Some(error);
        cx.quit();
    }
}
