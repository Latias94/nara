mod hot_reload;
mod performance;

use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    num::NonZeroU32,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail, ensure};
use nara_core::{Vec2, vfs::AssetPath};
use nara_editor_core::{
    EditorCommand, EditorDocument, InspectorModel, InspectorValue, play::PlayControl,
};
use nara_project::workflow::ProjectTrust;
use nara_reflect::{ComponentFieldId, ComponentTypeId, ComponentValue};
use open_gpui::{App, AsyncApp, px, size};
use open_gpui_docking::{
    dockspace::{model::DockMoveSource, policy::DockPolicy},
    persistence::DockWorkspaceDocument,
};

use crate::{
    docking::{self, OpenEditor, ROOT},
    workspace::{JobStatus, Workspace},
};

const NATIVE: Duration = Duration::from_secs(20);
const BUILD: Duration = Duration::from_secs(240);

#[derive(Clone, Copy)]
pub(crate) enum Probe {
    Verify,
    CaptureFrames(NonZeroU32),
    MeasureHotReload(u32),
    MeasureEditorPerformance {
        count: u32,
        without_accessibility: bool,
    },
}

impl Probe {
    pub(crate) const USAGE: &str = "[--verify | --capture-frames N | --measure-hot-reload N | \
        --measure-editor-performance N | --measure-editor-performance-no-a11y N]";

    pub(crate) fn parse(
        argument: &OsStr,
        arguments: &mut impl Iterator<Item = OsString>,
    ) -> Result<Option<Self>> {
        let probe = match argument.to_str() {
            Some("--verify") => Self::Verify,
            Some("--capture-frames") => Self::CaptureFrames(
                arguments
                    .next()
                    .and_then(|count| count.to_str()?.parse().ok())
                    .context("--capture-frames requires a positive frame count")?,
            ),
            Some("--measure-hot-reload") => Self::MeasureHotReload(
                arguments
                    .next()
                    .and_then(|count| count.to_str()?.parse().ok())
                    .filter(|count| (1..=100).contains(count))
                    .context("--measure-hot-reload requires a sample count from 1 to 100")?,
            ),
            Some("--measure-editor-performance" | "--measure-editor-performance-no-a11y") => {
                let count = arguments
                    .next()
                    .and_then(|count| count.to_str()?.parse().ok())
                    .filter(|count| (2..=600).contains(count))
                    .context("The editor performance probe requires 2 to 600 frames per case")?;
                Self::MeasureEditorPerformance {
                    count,
                    without_accessibility: argument == "--measure-editor-performance-no-a11y",
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(probe))
    }

    pub(crate) fn without_accessibility(self) -> bool {
        matches!(
            self,
            Self::MeasureEditorPerformance {
                without_accessibility: true,
                ..
            }
        )
    }

    pub(crate) fn prepare(
        self,
        project: &mut Option<PathBuf>,
        trust: ProjectTrust,
    ) -> Result<Option<tempfile::TempDir>> {
        match self {
            Self::Verify => ensure!(
                project.is_some() && trust == ProjectTrust::Trusted,
                "--verify requires --project and --trusted"
            ),
            Self::CaptureFrames(_) => {
                ensure!(project.is_some(), "--capture-frames requires --project")
            }
            Self::MeasureHotReload(_) => {
                let original = project
                    .as_deref()
                    .context("--measure-hot-reload requires a Night Relay --project")?;
                let copy = hot_reload::copy_fixture(original)?;
                *project = Some(copy.path().to_path_buf());
                return Ok(Some(copy));
            }
            Self::MeasureEditorPerformance { .. } => {
                ensure!(
                    !cfg!(debug_assertions),
                    "--measure-editor-performance requires a release build"
                );
                let original = project
                    .as_deref()
                    .context("--measure-editor-performance requires --project")?;
                let copy = hot_reload::copy_fixture(original)?;
                *project = Some(copy.path().to_path_buf());
                return Ok(Some(copy));
            }
        }
        Ok(None)
    }

    pub(crate) fn start(
        self,
        editor: OpenEditor,
        failure: Rc<RefCell<Option<anyhow::Error>>>,
        cx: &mut App,
    ) {
        match self {
            Self::Verify => start(editor, failure, cx),
            Self::CaptureFrames(count) => capture_frames(editor, count.get(), failure, cx),
            Self::MeasureHotReload(count) => hot_reload::start(editor, count, failure, cx),
            Self::MeasureEditorPerformance { count, .. } => {
                performance::start(editor, count, failure, cx)
            }
        }
    }
}

fn start(editor: OpenEditor, failure: Rc<RefCell<Option<anyhow::Error>>>, cx: &mut App) {
    cx.spawn(async move |cx| {
        match exercise(&editor, cx).await {
            Ok(()) => println!("Night Relay editor verification passed"),
            Err(error) => {
                eprintln!("Night Relay editor verification failed: {error:#}");
                *failure.borrow_mut() = Some(error);
            }
        }
        let _ = cx.update(|cx| {
            editor.workspace.update(cx, Workspace::stop);
            cx.quit();
        });
    })
    .detach();
}

fn capture_frames(
    editor: OpenEditor,
    count: u32,
    failure: Rc<RefCell<Option<anyhow::Error>>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        if let Err(error) = capture_viewport(&editor, count, cx).await {
            eprintln!("Viewport capture failed: {error:#}");
            *failure.borrow_mut() = Some(error);
        }
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

async fn capture_viewport(editor: &OpenEditor, count: u32, cx: &mut AsyncApp) -> Result<()> {
    const WARMUP: u32 = 16;
    let initial = frames(editor, cx, 1).await?;
    cx.update(|cx| {
        editor
            .viewport
            .update(cx, |view, cx| view.request_frames(WARMUP, cx))
    });
    let baseline = frames(editor, cx, initial.rendered_frames + u64::from(WARMUP)).await?;
    let presents = |cx: &mut App| {
        editor
            .window
            .update(cx, |_, window, _| window.present_submission_count())
            .ok()
    };
    let initial_presents = cx.update(presents).context("The capture window closed")?;
    println!("Viewport warmed up. Requesting {count} frames");
    cx.update(|cx| {
        editor
            .viewport
            .update(cx, |view, cx| view.request_frames(count, cx))
    });
    let count = u64::from(count);
    let timeout = NATIVE + Duration::from_millis(count * 100);
    let (stats, submissions) = wait(cx, timeout, "present the captured viewport frames", |cx| {
        let stats = editor.viewport.read(cx).stats();
        let submissions = presents(cx)?.saturating_sub(initial_presents);
        (stats.rendered_frames >= baseline.rendered_frames + count && submissions >= count)
            .then_some((stats, submissions))
    })
    .await?;
    println!(
        "Viewport capture submitted {} Nara frames and {submissions} GPUI presents, with {} new textures and {} new buffers",
        stats.rendered_frames - baseline.rendered_frames,
        stats.created_texture_calls - baseline.created_texture_calls,
        stats.created_buffer_calls - baseline.created_buffer_calls,
    );
    Ok(())
}

async fn exercise(editor: &OpenEditor, cx: &mut AsyncApp) -> Result<()> {
    let initial = wait(cx, NATIVE, "present the shared viewport", |cx| {
        let stats = editor.viewport.read(cx).stats();
        let presented = editor
            .window
            .update(cx, |_, window, _| window.present_submission_count())
            .ok()?;
        (presented > 0 && stats.rendered_frames > 0 && stats.extraction.extracted_sprites > 16)
            .then_some(stats)
    })
    .await?;
    println!(
        "Scene View rendered {} sprites",
        initial.extraction.extracted_sprites
    );
    let source = cx
        .update(|cx| {
            editor
                .workspace
                .read(cx)
                .session
                .document()
                .path()
                .map(ToOwned::to_owned)
        })
        .context("Night Relay must have an authored source scene")?;
    let source_contents = std::fs::read(&source)?;

    cx.update(|cx| editor.workspace.update(cx, Workspace::refresh_schema));
    let component = ComponentTypeId::new("night-relay.RunRules");
    let field = ComponentFieldId::new("shield_charges");
    wait(cx, BUILD, "refresh the Night Relay Schema", |cx| {
        let workspace = editor.workspace.read(cx);
        (workspace.status == JobStatus::Idle).then(|| {
            ensure!(
                workspace.session.registry().schema(&component).is_some()
                    && workspace
                        .console
                        .entries()
                        .iter()
                        .any(|line| line.message == "Schema refreshed"),
                "Night Relay Schema was not refreshed"
            );
            Ok(())
        })
    })
    .await?
    .with_context(|| recent_console(editor, cx))?;

    let scratch = tempfile::tempdir()?;
    let authored_copy = scratch.path().join("verified.scene.ron");
    let edited_shields = cx.update(|cx| {
        editor.workspace.update(cx, |workspace, cx| -> Result<u64> {
            let (entity, previous) = workspace
                .session
                .document()
                .scene()
                .entities
                .iter()
                .find_map(|row| {
                    row.components
                        .get(&component)
                        .and_then(|record| record.value().cloned().map(|value| (row.id, value)))
                })
                .context("Night Relay has no editable rules entity")?;
            let path = workspace
                .session
                .registry()
                .schema(&component)
                .and_then(|schema| {
                    schema
                        .fields()
                        .iter()
                        .find(|candidate| candidate.id() == &field)
                })
                .context("Run Rules has no Shield charges field")?
                .path()
                .clone();
            let next = if previous.get_path(&path)?.as_u64() == Some(4) {
                3
            } else {
                4
            };
            workspace.select([entity], cx);
            ensure!(
                workspace.apply(
                    EditorCommand::SetField {
                        entities: vec![entity],
                        component: component.clone(),
                        path,
                        value: ComponentValue::U64(next)
                    },
                    cx
                ),
                "Inspector edit was rejected"
            );
            workspace.undo(cx);
            ensure!(
                workspace
                    .session
                    .document()
                    .scene()
                    .entities
                    .iter()
                    .find(|row| row.id == entity)
                    .unwrap()
                    .components[&component]
                    .value()
                    == Some(&previous),
                "Undo did not restore the original Rust component"
            );
            workspace.redo(cx);
            let inspector = InspectorModel::for_session(&workspace.session);
            let value = inspector
                .components
                .iter()
                .find(|entry| entry.id == component)
                .and_then(|entry| {
                    entry
                        .fields
                        .iter()
                        .find(|candidate| candidate.schema.id() == &field)
                })
                .context("Inspector lost the authored field")?;
            ensure!(
                value.value == InspectorValue::Shared(ComponentValue::U64(next)),
                "Redo did not update the Inspector"
            );
            workspace.session.save_as(&authored_copy)?;
            let reopened = EditorDocument::open_with(
                &authored_copy,
                workspace.session.registry().catalog().components(),
            )?;
            ensure!(
                reopened.scene() == workspace.session.document().scene(),
                "Scene round trip changed the document"
            );
            let before = workspace.session.document().scene().entities.len();
            workspace.create(
                Vec2::new(15.0, 44.0),
                Some(AssetPath::new("power-cell.png")?),
                None,
                cx,
            );
            ensure!(
                workspace.session.document().scene().entities.len() == before + 1,
                "Asset placement did not create a sprite"
            );
            workspace.undo(cx);
            Ok(next)
        })
    })?;
    println!("Schema edit, undo, redo, save, reload and sprite placement passed");

    let old_imports = cx.update(|cx| editor.viewport.read(cx).stats().image_imports);
    editor.window.update(cx, |_, window, _| {
        let _ = window.resize(size(px(1280.0), px(820.0)));
    })?;
    let after_resize = wait(cx, NATIVE, "resize the native viewport", |cx| {
        let stats = editor.viewport.read(cx).stats();
        (stats.image_imports > old_imports).then_some(stats)
    })
    .await?;
    cx.update(|cx| {
        editor
            .viewport
            .update(cx, |view, cx| view.request_frames(12, cx))
    });
    let baseline = frames(editor, cx, after_resize.rendered_frames + 12).await?;
    cx.update(|cx| {
        editor
            .viewport
            .update(cx, |view, cx| view.request_frames(60, cx))
    });
    let sample = frames(editor, cx, baseline.rendered_frames + 60).await?;
    ensure!(
        sample.created_buffer_calls == baseline.created_buffer_calls
            && sample.created_texture_calls == baseline.created_texture_calls,
        "Viewport allocated after warmup: {baseline:?} → {sample:?}"
    );
    ensure!(
        sample.retained_presentations > baseline.retained_presentations
            && sample.image_imports == baseline.image_imports,
        "Viewport did not reuse its registered image"
    );
    let quiet = sample.rendered_frames;
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    ensure!(
        cx.update(|cx| editor.viewport.read(cx).stats().rendered_frames) == quiet,
        "The viewport rendered continuously while idle"
    );
    cx.update(|cx| {
        editor
            .viewport
            .update(cx, |view, cx| view.request_frames(1, cx))
    });
    let refresh = frames(editor, cx, quiet + 1).await?;
    ensure!(
        refresh.rendered_frames == quiet + 1,
        "Idle refresh advanced more than one frame"
    );
    println!(
        "60 retained viewport frames with 0 new textures, 0 new buffers, and one idle refresh"
    );

    let inspector = editor
        .dock
        .item_id_for_external_key("inspector")
        .context("Inspector pane missing")?;
    cx.update(|cx| {
        editor.dock.update(
            |frame| {
                frame.move_to_new_window_default_current(DockMoveSource::Item(inspector))?;
                Ok(())
            },
            cx,
        )
    })?;
    let floating = wait(cx, NATIVE, "float the Inspector onto a peer window", |_| {
        let surface = editor
            .dock
            .with_view(|view| view.item(inspector).map(|item| item.surface()))?;
        (surface != ROOT).then_some(surface)
    })
    .await?;
    let layout = scratch.path().join("editor-layout.json");
    cx.update(|cx| docking::save_layout(&editor.dock, &layout, cx))?;
    let mut policy = DockPolicy::default();
    policy.set_allow_native_surfaces(true);
    let restored =
        DockWorkspaceDocument::restore_json(&std::fs::read(layout)?, policy, |_, _| true)?;
    ensure!(
        restored
            .session
            .view()
            .item(inspector)
            .is_some_and(|item| item.surface() == floating),
        "Layout did not preserve the floating Inspector"
    );
    println!("Inspector floated into a peer window and docking layout round-tripped");

    let edited_scene = cx.update(|cx| editor.workspace.read(cx).session.document().scene().clone());
    cx.update(|cx| editor.workspace.update(cx, Workspace::play));
    transport(editor, cx, JobStatus::Running, 0, BUILD).await?;
    let expected_game_output = format!("Night Relay started with {edited_shields} shields");
    ensure!(
        cx.update(|cx| editor
            .workspace
            .read(cx)
            .console
            .entries()
            .iter()
            .any(|line| line.message == expected_game_output)),
        "The running game did not report the edited Shield charges value"
    );
    cx.update(|cx| {
        editor.workspace.update(cx, |workspace, cx| {
            workspace.control(PlayControl::Pause, cx)
        })
    });
    transport(editor, cx, JobStatus::Paused, 0, NATIVE).await?;
    cx.update(|cx| {
        editor
            .workspace
            .update(cx, |workspace, cx| workspace.control(PlayControl::Step, cx))
    });
    transport(editor, cx, JobStatus::Paused, 1, NATIVE).await?;
    cx.update(|cx| {
        editor.workspace.update(cx, |workspace, cx| {
            workspace.control(PlayControl::Resume, cx)
        })
    });
    transport(editor, cx, JobStatus::Running, 1, NATIVE).await?;
    cx.update(|cx| editor.workspace.update(cx, Workspace::stop));
    transport(editor, cx, JobStatus::Idle, 1, NATIVE).await?;
    ensure!(
        std::fs::read(source)? == source_contents,
        "Play changed the original project scene"
    );
    ensure!(
        cx.update(|cx| editor.workspace.read(cx).session.document().scene() == &edited_scene),
        "Play changed the editor document"
    );
    ensure!(
        cx.update(|cx| editor
            .workspace
            .read(cx)
            .console
            .entries()
            .iter()
            .any(|line| line.message == "Stopped the project process")),
        "No completed Stop receipt"
    );
    println!(
        "External game started with {edited_shields} edited shields and acknowledged Pause, single Step, Resume and contained Stop"
    );
    Ok(())
}

async fn frames(
    editor: &OpenEditor,
    cx: &mut AsyncApp,
    count: u64,
) -> Result<nara_editor_viewport::ViewportStats> {
    wait(cx, NATIVE, "submit the requested viewport frames", |cx| {
        let stats = editor.viewport.read(cx).stats();
        (stats.rendered_frames >= count).then_some(stats)
    })
    .await
}

async fn transport(
    editor: &OpenEditor,
    cx: &mut AsyncApp,
    target: JobStatus,
    steps: u64,
    timeout: Duration,
) -> Result<()> {
    wait(cx, timeout, target.label(), |cx| {
        let workspace = editor.workspace.read(cx);
        if workspace.status == target && !workspace.control_pending() && workspace.steps == steps {
            Some(Ok(()))
        } else if workspace.status == JobStatus::Idle {
            Some(Err(anyhow::anyhow!("Play ended before {}", target.label())))
        } else {
            None
        }
    })
    .await
    .and_then(|result| result)
    .with_context(|| recent_console(editor, cx))
}

fn recent_console(editor: &OpenEditor, cx: &mut AsyncApp) -> String {
    cx.update(|cx| {
        let mut messages = editor.workspace.read(cx).console.entries().iter().rev();
        let mut recent: Vec<_> = messages
            .by_ref()
            .take(6)
            .map(|line| line.message.as_str())
            .collect();
        recent.reverse();
        recent.join(" | ")
    })
}

async fn wait<T>(
    cx: &mut AsyncApp,
    timeout: Duration,
    label: &str,
    mut probe: impl FnMut(&mut App) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = cx.update(&mut probe) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            bail!("Timed out waiting to {label}");
        }
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
    }
}
