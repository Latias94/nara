use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use open_gpui::{
    App, AsyncApp, PlatformFrameDeliveryMode, WindowId,
    profiler::{
        FrameTiming, PresentTiming,
        journal::{ForegroundEvent, ForegroundJournalEntry, IntervalBoundary},
    },
};

use super::{NATIVE, OpenEditor, wait};
use crate::viewport::{PerformanceBoundary, PerformanceFrames, ViewportCallbackTiming};

const WARMUP_FRAMES: u32 = 16;

pub(super) fn start(
    editor: OpenEditor,
    count: u32,
    without_accessibility: bool,
    failure: Rc<RefCell<Option<anyhow::Error>>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        match measure(&editor, count, without_accessibility, cx).await {
            Ok(()) => println!("EDITOR_PERFORMANCE_MEASUREMENT_OK"),
            Err(error) => {
                eprintln!("Editor performance measurement failed: {error:#}");
                *failure.borrow_mut() = Some(error);
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

async fn measure(
    editor: &OpenEditor,
    count: u32,
    without_accessibility: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    wait(cx, NATIVE, "present the initial editor viewport", |cx| {
        let viewport = editor.viewport.read(cx);
        (viewport.presented_preview().is_some()
            && viewport.stats().extraction.extracted_sprites > 0)
            .then_some(())
    })
    .await?;

    let console = editor
        .dock
        .item_id_for_external_key("console")
        .context("The editor has no Console pane")?;
    wait(cx, NATIVE, "settle the initial dock layout", |_| {
        (!editor.dock.is_update_pending()).then_some(())
    })
    .await?;
    cx.update(|cx| {
        editor.dock.update(
            |frame| {
                frame.select_item_current(console)?;
                Ok(())
            },
            cx,
        )
    })?;
    wait(cx, NATIVE, "show the Console status in the dock", |_| {
        (!editor.dock.is_update_pending()
            && editor
                .dock
                .with_view(|view| view.item(console).is_some_and(|item| item.is_active())))
        .then_some(())
    })
    .await?;

    cx.update(|cx| {
        editor.window.update(cx, |_, window, cx| -> Result<()> {
            ensure!(
                window.frame_delivery_mode() == PlatformFrameDeliveryMode::Synchronous,
                "The native profiler does not time platform submission on this deferred window"
            );
            ensure!(
                !without_accessibility || !window.is_a11y_enabled(),
                "The no-accessibility measurement opened an accessibility-enabled window"
            );
            let refresh_hz = window
                .display(cx)
                .and_then(|display| display.refresh_interval())
                .filter(|interval| !interval.is_zero())
                .map_or_else(|| "unavailable".into(), |interval| format!("{:.2}", 1.0 / interval.as_secs_f64()));
            let gpu = window.gpu_specs().map(|specs| specs.device_name);
            let viewport_size = window.viewport_size();
            let retention = std::env::var("GPUI_VIEW_RETENTION").unwrap_or_else(|_| "default".into());
            println!(
                "editor-performance native_display_refresh_hz={refresh_hz} adapter={gpu:?} window_logical_width={} window_logical_height={} window_scale={} window_active={} accessibility_enabled={} accessibility_active={} retention={retention}",
                viewport_size.width.to_f64(), viewport_size.height.to_f64(), window.scale_factor(), window.is_window_active(), window.is_a11y_enabled(), window.is_a11y_active(),
            );
            Ok(())
        })?
    })?;

    println!(
        "editor-performance metric=ui_work_wall_ms sums the Nara viewport callbacks, accepted GPUI draw and synchronous platform present associated with each submission. Present time includes any platform wait. The sum excludes idle intervals and event dispatch outside those sections. Draw-only percentiles include only draws paired with a native submission. Rates use first-to-last completion and (submissions - 1) / elapsed."
    );
    for (case, publish_status) in [("static", false), ("status", true)] {
        cx.update(|cx| begin_frames(editor, WARMUP_FRAMES, publish_status, cx))?;
        wait(cx, NATIVE, "warm the editor performance case", |cx| {
            editor
                .viewport
                .read(cx)
                .completed_performance_frames()
                .map(|_| ())
        })
        .await?;
        capture(editor, case, count, publish_status, cx).await?;
    }
    Ok(())
}

fn begin_frames(
    editor: &OpenEditor,
    count: u32,
    publish_status: bool,
    cx: &mut App,
) -> Result<PerformanceBoundary> {
    editor.window.update(cx, |_, window, cx| {
        let start = PerformanceBoundary::from_window(window);
        editor.viewport.update(cx, |view, cx| {
            view.request_performance_frames(count, publish_status, window, cx)
        })?;
        Ok(start)
    })?
}

async fn capture(
    editor: &OpenEditor,
    case: &str,
    count: u32,
    publish_status: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (start, initial_viewport, mut collector) = cx.update(|cx| -> Result<_> {
        let collector = cx.foreground_journal().collector();
        let initial_viewport = editor.viewport.read(cx).stats();
        let start = begin_frames(editor, count, publish_status, cx)?;
        Ok((start, initial_viewport, collector))
    })?;
    let mut timing = GpuiTiming::default();
    let timeout = NATIVE + Duration::from_millis(u64::from(count) * 100);
    let (viewport, stats) = wait(cx, timeout, "submit the editor performance frames", |cx| {
        let batch = collector.collect_unseen();
        timing.lost += batch.lost;
        timing.collect(editor.window.window_id(), batch.entries);
        let view = editor.viewport.read(cx);
        view.completed_performance_frames()
            .map(|done| (done.clone(), view.stats()))
    })
    .await?;
    let end = viewport
        .settled
        .context("The measured viewport did not settle")?;
    ensure!(
        viewport.submissions == u64::from(count)
            && stats.rendered_frames - initial_viewport.rendered_frames == viewport.submissions,
        "Observed viewport submissions differ from the measured {count} frames"
    );
    ensure!(
        stats.image_imports == initial_viewport.image_imports,
        "The viewport imported a new texture during the measurement"
    );
    let expected_status = if publish_status {
        viewport.submissions
    } else {
        0
    };
    ensure!(
        viewport.status_updates == expected_status,
        "Unexpected Console status update count"
    );

    let nara = SubmissionRate::from_viewport(&viewport)?;
    let gpui = timing.finish(
        start,
        end,
        viewport.submissions,
        &viewport.viewport_callbacks,
    )?;
    ensure!(
        gpui.draws == expected_status,
        "Expected {expected_status} GPUI draws in the {case} case, observed {}",
        gpui.draws
    );
    let [draw_p50, draw_p95] = gpui.draw_only.map_or_else(
        || ["unavailable".into(), "unavailable".into()],
        |values| values.map(|value| format!("{value:.3}")),
    );
    println!(
        "editor-performance case={case} requested_nara_frames={count} nara_submissions={} nara_elapsed_ms={:.3} nara_submissions_per_s={:.2} gpui_submissions={} gpui_elapsed_ms={:.3} gpui_submissions_per_s={:.2} gpui_draws={} gpui_retained_only_submissions={} status_updates={} gpui_draw_submit_p50_ms={:.3} gpui_draw_submit_p95_ms={:.3} gpui_platform_present_p50_ms={:.3} gpui_platform_present_p95_ms={:.3} gpui_draw_only_p50_ms={draw_p50} gpui_draw_only_p95_ms={draw_p95} viewport_callbacks={} viewport_callback_p50_ms={:.3} viewport_callback_p95_ms={:.3} ui_work_p50_ms={:.3} ui_work_p95_ms={:.3}",
        nara.count,
        milliseconds(nara.elapsed),
        nara.per_second(),
        gpui.rate.count,
        milliseconds(gpui.rate.elapsed),
        gpui.rate.per_second(),
        gpui.draws,
        gpui.rate.count - gpui.draws,
        viewport.status_updates,
        gpui.work[0],
        gpui.work[1],
        gpui.present[0],
        gpui.present[1],
        viewport.viewport_callbacks.len(),
        gpui.viewport[0],
        gpui.viewport[1],
        gpui.ui_work[0],
        gpui.ui_work[1],
    );
    Ok(())
}

#[derive(Default)]
struct GpuiTiming {
    draws: Vec<FrameTiming>,
    presentations: Vec<(PresentTiming, Option<FrameTiming>)>,
    lost: u64,
}

impl GpuiTiming {
    fn collect(&mut self, window_id: WindowId, entries: Vec<ForegroundJournalEntry>) {
        for entry in entries {
            match entry {
                ForegroundJournalEntry::Event(ForegroundEvent::Draw(timing))
                    if timing.window_id == window_id =>
                {
                    self.draws.push(timing)
                }
                ForegroundJournalEntry::Event(ForegroundEvent::Present(timing))
                    if timing.window_id == window_id =>
                {
                    self.presentations.push((timing, None))
                }
                ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(presented))
                    if presented.presentation.window_id == window_id =>
                {
                    self.presentations
                        .push((presented.presentation, Some(presented.frame)));
                }
                _ => {}
            }
        }
    }

    fn finish(
        self,
        start: PerformanceBoundary,
        end: PerformanceBoundary,
        expected_submissions: u64,
        viewport_callbacks: &[ViewportCallbackTiming],
    ) -> Result<GpuiResult> {
        ensure!(
            self.lost == 0,
            "The GPUI foreground journal lost {} entries",
            self.lost
        );
        let included = |began, completed| began >= start.at && completed <= end.at;
        let mut submissions = self
            .presentations
            .into_iter()
            .filter(|(present, _)| included(present.present_start, present.present_end))
            .collect::<Vec<_>>();
        submissions.sort_unstable_by_key(|(present, _)| present.present_end);
        let count = end.gpui_submissions - start.gpui_submissions;
        ensure!(
            submissions.len() as u64 == count,
            "GPUI recorded {} presentation timings for {count} native submissions; cannot attribute retained or refused attempts",
            submissions.len()
        );
        ensure!(
            count == expected_submissions,
            "Observed {count} GPUI submissions for {expected_submissions} Nara submissions"
        );
        let first = submissions
            .first()
            .context("No measured GPUI presentations")?
            .0
            .present_end;
        let last = submissions
            .last()
            .context("No measured GPUI presentations")?
            .0
            .present_end;
        let rate = SubmissionRate::new(count, first, last)?;
        let draws = self
            .draws
            .iter()
            .filter(|draw| included(draw.draw_start, draw.draw_end))
            .count() as u64;
        ensure!(
            submissions
                .iter()
                .filter(|(_, draw)| draw.is_some())
                .count() as u64
                == draws,
            "The GPUI profiler could not pair every accepted draw with its submission"
        );
        let mut work = Vec::with_capacity(submissions.len());
        let mut present = Vec::with_capacity(submissions.len());
        let mut draw_only = Vec::with_capacity(draws as usize);
        let mut viewport_per_submission = vec![None; submissions.len()];
        for callback in viewport_callbacks {
            ensure!(
                (start.gpui_submissions..end.gpui_submissions)
                    .contains(&callback.preceding_gpui_submissions),
                "A viewport callback has no matching GPUI submission"
            );
            let index = (callback.preceding_gpui_submissions - start.gpui_submissions) as usize;
            *viewport_per_submission[index].get_or_insert(Duration::ZERO) += callback.elapsed;
        }
        let mut ui_work = Vec::with_capacity(submissions.len());
        for ((submission, draw), viewport_time) in
            submissions.into_iter().zip(viewport_per_submission)
        {
            let viewport_time =
                viewport_time.context("A GPUI submission lacks a viewport callback timing")?;
            let present_time = submission.present_duration();
            ensure!(
                !present_time.is_zero(),
                "A native submission has no platform timing"
            );
            let draw_time = if let Some(draw) = draw {
                ensure!(
                    included(draw.draw_start, draw.draw_end),
                    "A measured submission used a draw from before the capture"
                );
                let duration = draw.draw_duration();
                draw_only.push(duration);
                duration
            } else {
                Duration::ZERO
            };
            work.push(draw_time + present_time);
            present.push(present_time);
            ui_work.push(viewport_time + draw_time + present_time);
        }
        let mut viewport_samples = viewport_callbacks
            .iter()
            .map(|callback| callback.elapsed)
            .collect::<Vec<_>>();
        Ok(GpuiResult {
            rate,
            draws,
            draw_only: (!draw_only.is_empty()).then(|| percentiles(&mut draw_only)),
            work: percentiles(&mut work),
            present: percentiles(&mut present),
            viewport: percentiles(&mut viewport_samples),
            ui_work: percentiles(&mut ui_work),
        })
    }
}

struct GpuiResult {
    rate: SubmissionRate,
    draws: u64,
    draw_only: Option<[f64; 2]>,
    work: [f64; 2],
    present: [f64; 2],
    viewport: [f64; 2],
    ui_work: [f64; 2],
}

struct SubmissionRate {
    count: u64,
    elapsed: Duration,
}

impl SubmissionRate {
    fn new(count: u64, first: Instant, last: Instant) -> Result<Self> {
        let elapsed = last.saturating_duration_since(first);
        ensure!(
            count >= 2 && !elapsed.is_zero(),
            "Need two distinct submission times to measure throughput"
        );
        Ok(Self { count, elapsed })
    }

    fn from_viewport(frames: &PerformanceFrames) -> Result<Self> {
        Self::new(
            frames.submissions,
            frames
                .first_submission
                .context("No first Nara submission time")?,
            frames
                .last_submission
                .context("No last Nara submission time")?,
        )
    }

    fn per_second(&self) -> f64 {
        (self.count - 1) as f64 / self.elapsed.as_secs_f64()
    }
}

fn percentiles(samples: &mut [Duration]) -> [f64; 2] {
    samples.sort_unstable();
    [50, 95].map(|rank| milliseconds(samples[(samples.len() * rank).div_ceil(100) - 1]))
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_gpui::profiler::journal::PresentedFrame;

    #[test]
    fn successful_native_submissions_include_draw_and_retained_only_work() {
        let window_id = WindowId::from(1);
        let start = Instant::now();
        let ms = |value| start + Duration::from_millis(value);
        let draw = FrameTiming {
            window_id,
            draw_start: ms(1),
            draw_end: ms(4),
            dirty_at: None,
            invalidations: 0,
            refresh_interval: None,
            signal_at: None,
            phases: Default::default(),
        };
        let present = |begin, end| PresentTiming {
            window_id,
            present_start: ms(begin),
            present_end: ms(end),
            animation_interval: None,
        };
        let mut sample = GpuiTiming::default();
        sample.collect(
            window_id,
            vec![
                ForegroundJournalEntry::Event(ForegroundEvent::Draw(draw)),
                ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(PresentedFrame {
                    frame: draw,
                    presentation: present(4, 6),
                })),
                ForegroundJournalEntry::Event(ForegroundEvent::Present(present(10, 11))),
                ForegroundJournalEntry::Event(ForegroundEvent::Present(present(20, 21))),
            ],
        );
        let begin = PerformanceBoundary {
            at: start,
            gpui_submissions: 6,
        };
        let end = PerformanceBoundary {
            at: ms(12),
            gpui_submissions: 8,
        };
        let result = sample
            .finish(
                begin,
                end,
                2,
                &[
                    ViewportCallbackTiming {
                        preceding_gpui_submissions: 6,
                        elapsed: Duration::from_millis(1),
                    },
                    ViewportCallbackTiming {
                        preceding_gpui_submissions: 7,
                        elapsed: Duration::from_millis(2),
                    },
                    ViewportCallbackTiming {
                        preceding_gpui_submissions: 7,
                        elapsed: Duration::from_millis(3),
                    },
                ],
            )
            .unwrap();
        assert_eq!(result.rate.count, 2);
        assert_eq!(result.rate.elapsed, Duration::from_millis(5));
        assert_eq!(result.rate.per_second(), 200.0);
        assert_eq!(result.draws, 1);
        assert_eq!(result.draw_only, Some([3.0, 3.0]));
        assert_eq!(result.work, [1.0, 5.0]);
        assert_eq!(result.present, [1.0, 2.0]);
        assert_eq!(result.viewport, [2.0, 3.0]);
        assert_eq!(result.ui_work, [6.0, 6.0]);
    }

    #[test]
    fn retained_only_submissions_have_no_draw_percentiles() {
        let at = Instant::now();
        let ms = |value| at + Duration::from_millis(value);
        let present = |begin, end| PresentTiming {
            window_id: WindowId::from(1),
            present_start: ms(begin),
            present_end: ms(end),
            animation_interval: None,
        };
        let sample = || GpuiTiming {
            presentations: vec![(present(1, 3), None), (present(4, 8), None)],
            ..Default::default()
        };
        let begin = PerformanceBoundary {
            at,
            gpui_submissions: 6,
        };
        let end = PerformanceBoundary {
            at: ms(8),
            gpui_submissions: 8,
        };
        let callbacks = [6, 7].map(|preceding_gpui_submissions| ViewportCallbackTiming {
            preceding_gpui_submissions,
            elapsed: Duration::from_millis(1),
        });
        let result = sample().finish(begin, end, 2, &callbacks).unwrap();
        assert_eq!(result.draws, 0);
        assert_eq!(result.draw_only, None);
        assert_eq!(result.work, [2.0, 4.0]);
        assert_eq!(result.present, result.work);

        let extra = sample()
            .finish(begin, end, 1, &callbacks[..1])
            .err()
            .unwrap();
        assert!(extra.to_string().contains("2 GPUI submissions for 1 Nara"));
        let missing = sample()
            .finish(begin, end, 2, &callbacks[..1])
            .err()
            .unwrap();
        assert!(
            missing
                .to_string()
                .contains("lacks a viewport callback timing")
        );
    }

    #[test]
    fn refused_attempts_cannot_be_reported_as_retained_submissions() {
        let at = Instant::now();
        let end = at + Duration::from_millis(2);
        let present = PresentTiming {
            window_id: WindowId::from(1),
            present_start: at,
            present_end: end,
            animation_interval: None,
        };
        let sample = GpuiTiming {
            presentations: vec![(present, None)],
            ..Default::default()
        };
        let begin = PerformanceBoundary {
            at,
            gpui_submissions: 7,
        };
        let end = PerformanceBoundary {
            at: end,
            gpui_submissions: 7,
        };
        let error = sample.finish(begin, end, 0, &[]).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("cannot attribute retained or refused attempts")
        );
    }
}
