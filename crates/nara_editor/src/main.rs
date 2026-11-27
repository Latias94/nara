mod docking;
mod inspector;
mod jobs;
mod panels;
#[cfg(feature = "native-smoke")]
mod smoke;
mod theme;
mod viewport;
mod workspace;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::{Context as _, Result, bail, ensure};
use nara_project::workflow::ProjectTrust;
use open_gpui::{
    App, Bounds, Context, IntoElement, PathPromptOptions, Render, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use open_gpui_wgpu::SharedWgpuContext;

fn main() -> Result<()> {
    let mut project: Option<PathBuf> = None;
    let mut trust = ProjectTrust::Untrusted;
    #[cfg(feature = "native-smoke")]
    let mut probe = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--trusted" {
            trust = ProjectTrust::Trusted;
        } else if argument == "--help" || argument == "-h" {
            const USAGE: &str = "Usage: nara_editor [--project DIRECTORY] [--trusted]";
            #[cfg(feature = "native-smoke")]
            println!("{USAGE} {}", smoke::Probe::USAGE);
            #[cfg(not(feature = "native-smoke"))]
            println!("{USAGE}");
            return Ok(());
        } else if argument == "--project" && project.is_none() {
            project = Some(
                arguments
                    .next()
                    .context("--project requires a directory")?
                    .into(),
            );
        } else {
            #[cfg(feature = "native-smoke")]
            if let Some(parsed) = smoke::Probe::parse(&argument, &mut arguments)? {
                ensure!(
                    probe.replace(parsed).is_none(),
                    "Choose one native smoke probe"
                );
                continue;
            }
            bail!("Unknown argument: {argument:?}. Use --help for usage.");
        }
    }
    #[cfg(feature = "native-smoke")]
    let _measurement_copy = match probe {
        Some(probe) => probe.prepare(&mut project, trust)?,
        None => None,
    };
    #[cfg(feature = "native-smoke")]
    let persist = probe.is_none();
    #[cfg(not(feature = "native-smoke"))]
    let persist = true;
    let shared = pollster::block_on(create_shared_gpu())?;
    let failure = Rc::new(RefCell::new(None));
    let on_open_failure = failure.clone();
    open_gpui_platform::application_with_shared_wgpu(shared.clone())?.run(move |cx| {
        let result = if let Some(directory) = project {
            workspace::Workspace::open(&directory, trust).and_then(|workspace| {
                let editor = docking::open(workspace, &shared, persist, cx)?;
                #[cfg(feature = "native-smoke")]
                if let Some(probe) = probe {
                    probe.start(editor, on_open_failure.clone(), cx);
                }
                #[cfg(not(feature = "native-smoke"))]
                let _ = editor;
                Ok(())
            })
        } else {
            open_project_manager(shared, cx)
        };
        if let Err(error) = result {
            eprintln!("Could not open the editor: {error:#}");
            *on_open_failure.borrow_mut() = Some(error);
            cx.quit();
        }
    });
    failure.borrow_mut().take().map_or(Ok(()), Err)
}

async fn create_shared_gpu() -> Result<SharedWgpuContext> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends &= if cfg!(windows) {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    } else if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::VULKAN
    };
    #[cfg(windows)]
    {
        descriptor.backend_options.dx12.latency_waitable_object =
            wgpu::Dx12UseFrameLatencyWaitableObject::DontWait.with_env();
    }
    ensure!(
        !descriptor.backends.is_empty(),
        "WGPU_BACKEND excludes the native editor backends"
    );
    let backends = descriptor.backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter_name = std::env::var("WGPU_ADAPTER_NAME")
        .ok()
        .map(|name| name.to_lowercase());
    let adapter = instance
        .enumerate_adapters(backends)
        .await
        .into_iter()
        .filter(|adapter| {
            adapter_name
                .as_ref()
                .is_none_or(|name| adapter.get_info().name.to_lowercase().contains(name))
        })
        .min_by_key(|adapter| {
            let info = adapter.get_info();
            let kind = match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 0,
                wgpu::DeviceType::IntegratedGpu => 1,
                wgpu::DeviceType::VirtualGpu => 2,
                wgpu::DeviceType::Other => 3,
                wgpu::DeviceType::Cpu => 4,
            };
            (kind, info.backend != wgpu::Backend::Dx12)
        })
        .context("No WGPU adapter matched the native editor backends")?;
    Ok(SharedWgpuContext::request_device(
        instance,
        adapter,
        wgpu::DeviceDescriptor {
            label: Some("Nara Editor"),
            ..Default::default()
        },
    )
    .await?)
}

struct ProjectManager {
    shared: SharedWgpuContext,
    error: Option<String>,
}

fn open_project_manager(shared: SharedWgpuContext, cx: &mut App) -> Result<()> {
    let bounds = Bounds::centered(None, size(px(540.0), px(300.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Nara · Project Manager".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |_, cx| {
            cx.new(|_| ProjectManager {
                shared,
                error: None,
            })
        },
    )?;
    cx.activate(true);
    Ok(())
}

impl Render for ProjectManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        theme::panel()
            .p_6()
            .gap_3()
            .justify_center()
            .child(
                div()
                    .text_size(px(28.0))
                    .text_color(rgb(theme::ACCENT))
                    .child("Nara"),
            )
            .child(
                div()
                    .text_color(rgb(theme::MUTED))
                    .child("Open a project folder containing nara.toml."),
            )
            .child(
                div()
                    .flex()
                    .child(
                        theme::button("open-project", "Open Project…").on_click(cx.listener(
                            |manager, _, window, cx| {
                                let prompt = cx.prompt_for_paths(PathPromptOptions {
                                    files: false,
                                    directories: true,
                                    multiple: false,
                                    prompt: Some("Open Nara project".into()),
                                });
                                let window = window.window_handle();
                                let shared = manager.shared.clone();
                                cx.spawn(async move |manager, cx| {
                                    let Ok(result) = prompt.await else {
                                        return;
                                    };
                                    let _ = manager.update(cx, move |manager, cx| {
                                        let result = result.and_then(|paths| {
                                            let Some(directory) =
                                                paths.and_then(|paths| paths.into_iter().next())
                                            else {
                                                return Ok(None);
                                            };
                                            let workspace = workspace::Workspace::open(
                                                &directory,
                                                ProjectTrust::Untrusted,
                                            )?;
                                            docking::open(workspace, &shared, true, cx).map(Some)
                                        });
                                        match result {
                                            Ok(Some(_)) => {
                                                let _ = window.update(cx, |_, window, cx| {
                                                    window.remove_window(cx)
                                                });
                                            }
                                            Ok(None) => {}
                                            Err(error) => {
                                                manager.error = Some(error.to_string());
                                                cx.notify();
                                            }
                                        }
                                    });
                                })
                                .detach();
                            },
                        )),
                    ),
            )
            .when_some(self.error.clone(), |div, error| div.child(div_label(error)))
    }
}

fn div_label(error: String) -> impl IntoElement {
    div().text_color(rgb(theme::ERROR)).child(error)
}
