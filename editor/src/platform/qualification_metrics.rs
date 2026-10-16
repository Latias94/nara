use std::{collections::VecDeque, time::Instant};

use nara::{render::RenderWindowPacket, render_wgpu::__private::WgpuEditorExecution};

pub(super) struct FrontendMetrics {
    frontend: &'static str,
    started: Instant,
    first_present_us: Option<u128>,
    redraws: u64,
    presents: u64,
    redraw_us: VecDeque<u64>,
}

impl FrontendMetrics {
    pub(super) fn requested(frontend: &'static str) -> Option<Self> {
        (std::env::var_os("NARA_EDITOR_FRONTEND_METRICS").as_deref()
            == Some(std::ffi::OsStr::new("1")))
        .then(|| Self {
            frontend,
            started: Instant::now(),
            first_present_us: None,
            redraws: 0,
            presents: 0,
            redraw_us: VecDeque::new(),
        })
    }

    pub(super) fn observe(&mut self, started: Instant, presented: bool) {
        self.redraws += 1;
        if presented {
            self.presents += 1;
            self.first_present_us
                .get_or_insert_with(|| self.started.elapsed().as_micros());
        }
        if self.redraw_us.len() == 2048 {
            self.redraw_us.pop_front();
        }
        self.redraw_us
            .push_back(started.elapsed().as_micros() as u64);
    }

    pub(super) fn finish(self, gpu: Option<(&mut WgpuEditorExecution, RenderWindowPacket)>) {
        let resources = gpu.and_then(|(gpu, window)| {
            gpu.with_device(window, |scope| {
                scope.instance().generate_report().map(|report| {
                    serde_json::json!({
                        "surfaces": report.surfaces.num_kept_from_user,
                        "devices": report.hub.devices.num_kept_from_user,
                        "buffers": report.hub.buffers.num_kept_from_user,
                        "textures": report.hub.textures.num_kept_from_user,
                        "texture_views": report.hub.texture_views.num_kept_from_user,
                        "bind_groups": report.hub.bind_groups.num_kept_from_user,
                        "render_pipelines": report.hub.render_pipelines.num_kept_from_user,
                    })
                })
            })
            .ok()
            .flatten()
        });
        let mut samples = self.redraw_us.into_iter().collect::<Vec<_>>();
        samples.sort_unstable();
        let percentile = |percent: usize| {
            samples
                .get(samples.len().saturating_sub(1) * percent / 100)
                .copied()
        };
        println!(
            "{}",
            serde_json::json!({
                "schema": "nara.editor-frontend-metrics-v1",
                "frontend": self.frontend,
                "elapsed_ms": self.started.elapsed().as_millis(),
                "first_present_us": self.first_present_us,
                "redraws": self.redraws,
                "presents": self.presents,
                "redraw_samples": samples.len(),
                "redraw_p50_us": percentile(50),
                "redraw_p95_us": percentile(95),
                "retained_wgpu_handles": resources,
            })
        );
    }
}
