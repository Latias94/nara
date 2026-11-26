use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail, ensure};
use open_gpui::{
    App, AsyncApp, Window, WindowHandle, WindowInitialPresentationStatus, WindowMutationDispatch,
    WindowPresentationFacts, px, size,
};
use open_gpui_wgpu::SharedWgpuContext;

use crate::{Failure, Inspector, Viewport, ViewportGeometry, fail, scene::SceneFacts};

const TIMEOUT: Duration = Duration::from_secs(15);
const SAMPLE_TIME: Duration = Duration::from_secs(2);
const WARMUP_FRAMES: u64 = 30;

pub(crate) fn start(
    viewport: WindowHandle<Viewport>,
    inspector: WindowHandle<Inspector>,
    shared: SharedWgpuContext,
    failure: Failure,
    cx: &App,
) {
    cx.spawn(async move |cx| {
        let result = run(viewport, inspector, &shared, cx).await;
        cx.update(|cx| match result {
            Ok(()) if failure.borrow().is_none() => {
                println!("SHARED_VIEWPORT_SMOKE_OK");
                cx.quit();
            }
            Ok(()) => {}
            Err(error) => fail(&failure, error, cx),
        });
    })
    .detach();
}

#[derive(Clone, Copy, Debug)]
struct NativeFrame {
    facts: WindowPresentationFacts,
    revision: u64,
    submissions: u64,
}

impl NativeFrame {
    fn sample(window: &Window) -> Self {
        Self {
            facts: window.presentation_facts(),
            revision: window.rendered_frame_revision(),
            submissions: window.present_submission_count(),
        }
    }

    fn is_presented(self) -> bool {
        self.facts.native_created
            && self.facts.native_visible
            && self.facts.initial_presentation == WindowInitialPresentationStatus::Completed
            && self.facts.non_empty_presented_generation == Some(self.revision)
            && self.submissions > 0
    }

    fn advanced_from(self, previous: Self) -> bool {
        self.is_presented() && self.submissions > previous.submissions
    }
}

#[derive(Clone, Copy, Debug)]
struct ViewportSample {
    native: NativeFrame,
    window_geometry: Option<ViewportGeometry>,
    image_geometry: Option<ViewportGeometry>,
    scene: SceneFacts,
    retained: u64,
    refresh_interval: Option<Duration>,
}

fn viewport_sample(viewport: WindowHandle<Viewport>, cx: &mut App) -> Result<ViewportSample> {
    viewport.update(cx, |view, window, cx| ViewportSample {
        native: NativeFrame::sample(window),
        window_geometry: ViewportGeometry::from_window(window),
        image_geometry: view.image.as_ref().map(|image| image.geometry),
        scene: view.scene.facts(),
        retained: view.retained_presentations,
        refresh_interval: window.refresh_interval(cx),
    })
}

async fn run(
    viewport: WindowHandle<Viewport>,
    inspector: WindowHandle<Inspector>,
    shared: &SharedWgpuContext,
    cx: &AsyncApp,
) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    let (before, inspector_before) =
        wait_for(cx, deadline, "two native presents and warmup", |cx| {
            let view = viewport_sample(viewport, cx)?;
            let panel = inspector.update(cx, |_, window, _| NativeFrame::sample(window))?;
            Ok((view.image_geometry.is_some()
                && view.native.is_presented()
                && panel.is_presented()
                && view.scene.frames >= WARMUP_FRAMES)
                .then_some((view, panel)))
        })
        .await?;
    let start = Instant::now();
    cx.background_executor().timer(SAMPLE_TIME).await;
    let (after, inspector_after) = cx.update(|cx| -> Result<_> {
        Ok((
            viewport_sample(viewport, cx)?,
            inspector.update(cx, |_, window, _| NativeFrame::sample(window))?,
        ))
    })?;
    let elapsed = start.elapsed().as_secs_f64();
    ensure!(
        after.image_geometry == before.image_geometry,
        "viewport geometry changed during the sample"
    );
    ensure!(
        after.native.advanced_from(before.native),
        "viewport stopped presenting: {after:?}"
    );
    ensure!(
        after.retained > before.retained,
        "no retained scene presentation was observed"
    );
    ensure!(
        inspector_after.is_presented(),
        "the inspector lost its native presentation"
    );
    ensure!(
        after.scene.created_textures == before.scene.created_textures
            && after.scene.created_buffers == before.scene.created_buffers,
        "steady-state Nara rendering allocated GPU resources: {before:?} -> {after:?}"
    );
    println!(
        "Native windows: viewport_revision={} inspector_revision={}",
        before.native.revision, inspector_before.revision
    );
    print_steady_state(
        before,
        after,
        inspector_after.submissions - inspector_before.submissions,
        elapsed,
    );

    cx.update(|cx| -> Result<()> {
        let resize = viewport.update(cx, |view, window, _| {
            view.live = false;
            window.resize(size(px(576.0), px(384.0)))
        })?;
        ensure!(
            matches!(resize, WindowMutationDispatch::Queued(_)),
            "resize dispatch: {resize:?}"
        );
        inspector.update(cx, |_, window, _| window.refresh())?;
        Ok(())
    })?;
    let (resized, refreshed_inspector) =
        wait_for(cx, deadline, "resize and inspector refresh", |cx| {
            let view = viewport_sample(viewport, cx)?;
            let panel = inspector.update(cx, |_, window, _| NativeFrame::sample(window))?;
            Ok((view.image_geometry != after.image_geometry
                && view.image_geometry.is_some()
                && view.image_geometry == view.window_geometry
                && view.native.revision > after.native.revision
                && view.native.advanced_from(after.native)
                && panel.revision > inspector_after.revision
                && panel.advanced_from(inspector_after))
            .then_some((view, panel)))
        })
        .await?;
    println!(
        "Resize and refresh: viewport_revision={} inspector_revision={}",
        resized.native.revision, refreshed_inspector.revision
    );

    cx.update(|cx| viewport.update(cx, Viewport::refresh_once))?;
    let refreshed = wait_for(cx, deadline, "paused viewport refresh", |cx| {
        let view = viewport_sample(viewport, cx)?;
        Ok((view.scene.frames == resized.scene.frames + 1
            && view.native.revision == resized.native.revision
            && view.native.advanced_from(resized.native))
        .then_some(view))
    })
    .await?;
    println!(
        "Paused viewport refresh: viewport_submissions={}",
        refreshed.native.submissions
    );

    let work_done = Arc::new(AtomicBool::new(false));
    shared.queue().on_submitted_work_done({
        let work_done = Arc::clone(&work_done);
        move || work_done.store(true, Ordering::Release)
    });
    wait_for(cx, deadline, "shared GPU completion", |_| {
        shared.device().poll(wgpu::PollType::Poll)?;
        Ok(work_done.load(Ordering::Acquire).then_some(()))
    })
    .await?;
    let errors = shared.errors();
    ensure!(!errors.device_lost(), "shared WGPU device was lost");
    let mut cursor = 0;
    if let Some(error) = errors.observe_error(&mut cursor) {
        bail!("shared WGPU device reported an uncaptured error: {error}");
    }
    Ok(())
}

fn print_steady_state(
    before: ViewportSample,
    after: ViewportSample,
    inspector_submissions: u64,
    elapsed: f64,
) {
    let submissions = after.native.submissions - before.native.submissions;
    let refresh_hz = after
        .refresh_interval
        .filter(|interval| !interval.is_zero())
        .map_or("unknown".to_owned(), |interval| {
            format!("{:.1}", interval.as_secs_f64().recip())
        });
    println!(
        "Steady state: seconds={elapsed:.3} nara_frames={} viewport_submissions={submissions} \
         viewport_fps={:.1} display_refresh_hz={refresh_hz} \
         retained_submissions={} inspector_submissions={inspector_submissions} \
         sprites={} tile_cells={} nara_textures_created={} nara_textures_reused={} \
         nara_textures_idle={} nara_buffers_created={}",
        after.scene.frames - before.scene.frames,
        submissions as f64 / elapsed,
        after.retained - before.retained,
        after.scene.extraction.extracted_sprites,
        after.scene.extraction.extracted_tile_cells,
        after.scene.created_textures - before.scene.created_textures,
        after.scene.textures.reused - before.scene.textures.reused,
        after.scene.textures.idle,
        after.scene.created_buffers - before.scene.created_buffers,
    );
}

async fn wait_for<T>(
    cx: &AsyncApp,
    deadline: Instant,
    phase: &str,
    mut sample: impl FnMut(&mut App) -> Result<Option<T>>,
) -> Result<T> {
    while Instant::now() < deadline {
        if let Some(result) = cx
            .update(&mut sample)
            .with_context(|| format!("observe {phase}"))?
        {
            return Ok(result);
        }
        cx.background_executor()
            .timer(Duration::from_millis(16))
            .await;
    }
    bail!("timed out waiting for {phase}")
}
