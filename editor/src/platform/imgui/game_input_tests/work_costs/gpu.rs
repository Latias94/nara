//! Bounded diagnostics around the actual primary Dear paint in its original window encoder.
//!
//! WGPU 30 permits encoder commands to reorder across timestamp writes, depending on backend
//! and driver. These markers are not strict isolated execution or a proof of drawing/presenting;
//! the existing image/composite/window receipts independently establish those associations.

use std::{sync::mpsc, time::Instant};

use dear_imgui_wgpu::wgpu;
use nara::{
    render::RenderWindowPacket,
    render_wgpu::{
        __private::{WgpuEditorImguiFrame, WgpuRuntimeHostFacts},
        WgpuDeviceRequirements, WgpuRenderError, WgpuRenderHostError, WgpuSubmissionPrefix,
        WgpuWindowDeviceScope, WgpuWindowFrame, WgpuWindowPresentedReceipt,
    },
    window::WindowId as NaraWindowId,
};

use super::{CostSample, GameInputWorkflow, SAMPLE_FRAMES, elapsed_ns};
use crate::platform::imgui::{ImguiRenderer, NativeImgui, editor_window_packet};

const QUERIES_PER_SAMPLE: u32 = 2;
const QUERY_COUNT: u32 = SAMPLE_FRAMES as u32 * QUERIES_PER_SAMPLE;
// Every pair is resolved by its own sampled window encoder. Each destination must be aligned,
// even though a pair contains only two u64 values. No slot or query is reused during this run.
const RESOLVE_STRIDE: u64 = wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
const BUFFER_BYTES: u64 = SAMPLE_FRAMES as u64 * RESOLVE_STRIDE;
const REQUIRED_FEATURES: wgpu::Features =
    wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);

pub(super) fn device_requirements() -> WgpuDeviceRequirements {
    WgpuDeviceRequirements::new(REQUIRED_FEATURES, wgpu::Limits::default())
}

fn host_refusal(native: &mut NativeImgui, operation: &str, error: WgpuRenderHostError) -> String {
    if matches!(
        &error,
        WgpuRenderHostError::Backend(WgpuRenderError::DeviceLost)
    ) {
        native.close.device_lost = true;
    }
    format!(
        "primary GPU timestamp {operation}: {error}; host frame={}",
        native.frame_index
    )
}

#[derive(Clone, Copy)]
enum PendingSample {
    Armed,
    Encoded { display_frame_index: u64 },
}

struct SampleAssociation {
    window: WgpuWindowPresentedReceipt,
    runtime_frame: u64,
}

/// Test-only dependencies of the original Native owner, released after its real GPU drain.
pub(in crate::platform::imgui) struct PrimaryGpuTimestamps {
    backend_instance_id: u64,
    device_epoch: u64,
    timestamp_period_ns: f32,
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    samples: Vec<SampleAssociation>,
    pending: Option<PendingSample>,
    copy_prefix: Option<WgpuSubmissionPrefix>,
    map_callback: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    map_requested: bool,
}

pub(super) struct PrimaryGpuObservation {
    timestamp_period_ns: f32,
    intervals_ns: Vec<f64>,
    readback_ns: u64,
}

pub(super) fn enable_primary_gpu_timestamps(native: &mut NativeImgui) -> Result<(), String> {
    if native.primary_gpu_timestamps.is_some() {
        return Err("primary GPU timestamp diagnostics were already initialized".into());
    }
    let (backend_instance_id, device_epoch, timestamp_period_ns) = native
        .gpu
        .with_device(editor_window_packet(&native.window), |scope| {
            let missing = REQUIRED_FEATURES - scope.device().features();
            if !missing.is_empty() {
                return Err(format!(
                    "primary GPU timestamp prerequisites were not enabled before native initialization: {missing:?}"
                ));
            }
            let period = scope.queue().get_timestamp_period();
            if !period.is_finite() || period <= 0.0 {
                return Err(format!("the original queue has no valid timestamp period: {period:?}"));
            }
            Ok((scope.backend_instance_id(), scope.device_epoch(), period))
        })
        .map_err(|error| host_refusal(native, "initialization", error))??;
    let timestamps = &mut native.primary_gpu_timestamps;
    // Resource construction uses the Host's native error scopes. The empty setup submission is
    // outside every warmup/sample; no additional submission or pass is added to a sampled frame.
    // Store resources in Native before this boundary can report a post-callback error.
    let (_, setup_prefix) = native
        .gpu
        .submit_image_use(&[], |device, _, _, _| {
            *timestamps = Some(PrimaryGpuTimestamps {
                backend_instance_id,
                device_epoch,
                timestamp_period_ns,
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("nara_native_primary_paint_timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: QUERY_COUNT,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("nara_native_primary_paint_timestamp_resolve"),
                    size: BUFFER_BYTES,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("nara_native_primary_paint_timestamp_readback"),
                    size: BUFFER_BYTES,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                samples: Vec::with_capacity(SAMPLE_FRAMES),
                pending: None,
                copy_prefix: None,
                map_callback: None,
                map_requested: false,
            });
            Ok(())
        })
        .map_err(|error| host_refusal(native, "resource construction", error))?;
    // The Host retains this empty setup prefix; no diagnostic resource has a GPU use yet.
    // Native already holds original queue-drain custody from renderer initialization.
    drop(setup_prefix);
    Ok(())
}

impl PrimaryGpuTimestamps {
    pub(super) fn arm_sample(&mut self, sample: usize) -> Result<(), String> {
        if sample != self.samples.len()
            || sample >= SAMPLE_FRAMES
            || self.pending.is_some()
            || self.copy_prefix.is_some()
        {
            return Err(format!(
                "primary GPU timestamp sample cannot be armed: sample={sample}, confirmed={}, pending={}, readback_started={}",
                self.samples.len(),
                self.pending.is_some(),
                self.copy_prefix.is_some()
            ));
        }
        self.pending = Some(PendingSample::Armed);
        Ok(())
    }

    pub(in crate::platform::imgui) fn paint(
        &mut self,
        renderer: &mut ImguiRenderer,
        prepared: WgpuEditorImguiFrame<'_>,
        frame: &mut WgpuWindowFrame<'_>,
        window: RenderWindowPacket,
        display_frame_index: u64,
    ) -> Result<(), WgpuRenderHostError> {
        match self.pending {
            None => return renderer.paint(prepared, frame),
            Some(PendingSample::Armed) => {}
            Some(PendingSample::Encoded { .. }) => {
                return Err(WgpuRenderHostError::window_paint(
                    "the preceding primary GPU timestamp pair has no matching window receipt",
                ));
            }
        }
        if window.id != NaraWindowId::PRIMARY || self.samples.len() >= SAMPLE_FRAMES {
            return Err(WgpuRenderHostError::window_paint(
                "primary GPU timestamp encoding escaped its bounded primary-window samples",
            ));
        }
        let sample = self.samples.len() as u32;
        let first = sample * QUERIES_PER_SAMPLE;
        frame.encode(|_, _, encoder, _| encoder.write_timestamp(&self.queries, first));
        // This is the original paint, pass, target and encoder. Do not add a proxy draw/pass,
        // submit, fence or serialization to manufacture stronger timestamp ordering guarantees.
        renderer.paint(prepared, frame)?;
        frame.encode(|_, _, encoder, _| {
            encoder.write_timestamp(&self.queries, first + 1);
            encoder.resolve_query_set(
                &self.queries,
                first..first + QUERIES_PER_SAMPLE,
                &self.resolve,
                u64::from(sample) * RESOLVE_STRIDE,
            );
        });
        self.pending = Some(PendingSample::Encoded {
            display_frame_index,
        });
        Ok(())
    }

    pub(super) fn confirm_sample(
        &mut self,
        sample: usize,
        window: WgpuWindowPresentedReceipt,
        runtime_frame: u64,
    ) -> Result<(), String> {
        if sample != self.samples.len()
            || sample >= SAMPLE_FRAMES
            || !matches!(
                self.pending,
                Some(PendingSample::Encoded { display_frame_index })
                    if display_frame_index == window.display_frame_index()
            )
            || window.window_id() != NaraWindowId::PRIMARY
            || window.surface_generation().is_none()
            || window.backend_instance_id() != self.backend_instance_id
            || window.device_epoch() != self.device_epoch
            || self.samples.last().is_some_and(|last| {
                last.window.display_frame_index().checked_add(1)
                    != Some(window.display_frame_index())
                    || runtime_frame <= last.runtime_frame
            })
        {
            return Err(format!(
                "primary GPU timestamp pair did not match its actual sampled window receipt: sample={sample}, confirmed={}, window={window:?}, runtime_frame={runtime_frame}",
                self.samples.len()
            ));
        }
        self.samples.push(SampleAssociation {
            window,
            runtime_frame,
        });
        self.pending = None;
        Ok(())
    }

    fn validate_samples(
        &self,
        samples: &[CostSample],
        original: &WgpuRuntimeHostFacts,
    ) -> Result<(), String> {
        if self.pending.is_some()
            || self.copy_prefix.is_some()
            || self.map_requested
            || samples.len() != SAMPLE_FRAMES
            || self.samples.len() != SAMPLE_FRAMES
            || self.backend_instance_id != original.backend_instance_id
            || self.device_epoch != original.device_epoch
            || self.samples.iter().zip(samples).any(|(gpu, cpu)| {
                gpu.window != cpu.window || gpu.runtime_frame != cpu.runtime_frame
            })
        {
            return Err(
                "primary GPU timestamp readback lost the complete original sample association"
                    .into(),
            );
        }
        Ok(())
    }

    fn check_scope(&self, scope: &WgpuWindowDeviceScope<'_>) -> Result<(), String> {
        if scope.backend_instance_id() != self.backend_instance_id
            || scope.device_epoch() != self.device_epoch
            || scope.queue().get_timestamp_period() != self.timestamp_period_ns
        {
            return Err(
                "primary GPU timestamp readback reached a different device namespace or period"
                    .into(),
            );
        }
        Ok(())
    }

    fn begin_mapping(&mut self, scope: WgpuWindowDeviceScope<'_>) -> Result<(), String> {
        self.check_scope(&scope)?;
        if self.map_requested || self.copy_prefix.is_none() {
            return Err("primary GPU timestamp map must follow its one original Host copy".into());
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        self.map_callback = Some(receiver);
        self.map_requested = true;
        self.readback
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.try_send(result);
            });
        Ok(())
    }

    fn read_intervals(&mut self) -> Result<Vec<f64>, String> {
        let result = (|| {
            let mapped = self
                .readback
                .get_mapped_range(..)
                .map_err(|error| format!("primary GPU timestamp mapped range refused: {error}"))?;
            if mapped.len() != BUFFER_BYTES as usize {
                return Err(format!(
                    "primary GPU timestamp mapped byte count is {} instead of {BUFFER_BYTES}",
                    mapped.len()
                ));
            }
            let mut intervals = Vec::with_capacity(SAMPLE_FRAMES);
            for (index, slot) in mapped.chunks_exact(RESOLVE_STRIDE as usize).enumerate() {
                let first = u64::from_le_bytes(slot[..8].try_into().map_err(|_| {
                    format!("truncated primary GPU timestamp start at sample {index}")
                })?);
                let last = u64::from_le_bytes(slot[8..16].try_into().map_err(|_| {
                    format!("truncated primary GPU timestamp end at sample {index}")
                })?);
                let ticks = last.checked_sub(first).filter(|ticks| *ticks > 0)
                    .ok_or_else(|| format!(
                        "primary GPU timestamp pair is non-increasing or wrapped; refusing the complete run without dropping a sample: sample={index}, first={first}, last={last}"
                    ))?;
                intervals.push(ticks as f64 * f64::from(self.timestamp_period_ns));
            }
            Ok(intervals)
        })();
        // A mapped view is gone before unmapping. Ownership of both buffers and the query set
        // stays with Native through the later final-health/loss controls and real cleanup.
        self.readback.unmap();
        self.map_requested = false;
        result
    }

    pub(in crate::platform::imgui) fn retire_after_drain(&mut self) {
        if self.map_requested {
            // Includes timeout/error: cancellation is not interpreted as a successful map.
            self.readback.unmap();
            self.map_requested = false;
        }
        self.map_callback = None;
    }
}

impl GameInputWorkflow<'_> {
    pub(super) fn read_primary_gpu_timestamps(
        &mut self,
        samples: &[CostSample],
        original: &WgpuRuntimeHostFacts,
        host_completions: &mut Vec<nara::render_wgpu::WgpuFrameCompletionSample>,
    ) -> Result<Option<PrimaryGpuObservation>, String> {
        let Some(timestamps) = self.native.primary_gpu_timestamps.as_ref() else {
            return Ok(None);
        };
        timestamps.validate_samples(samples, original)?;
        self.stage = "primary GPU timestamp readback outside sampled CPU/completion timers";
        let readback_started = Instant::now();
        if !self
            .work_original_device_is_healthy(original)
            .map_err(|error| self.frame_error("pre-readback original-device health", error))?
        {
            return Err("primary GPU timestamp copy refused a replacement device".into());
        }
        let copied = {
            let timestamps = self
                .native
                .primary_gpu_timestamps
                .as_ref()
                .ok_or("primary GPU timestamp custody disappeared before copy")?;
            self.native.gpu.submit_image_use(&[], |_, _, encoder, _| {
                encoder.copy_buffer_to_buffer(
                    &timestamps.resolve,
                    0,
                    &timestamps.readback,
                    0,
                    BUFFER_BYTES,
                );
                Ok(())
            })
        };
        let (_, prefix) = copied
            .map_err(|error| host_refusal(self.native, "original Host readback copy", error))?;
        let mapping = {
            let timestamps = self
                .native
                .primary_gpu_timestamps
                .as_mut()
                .ok_or("primary GPU timestamp custody disappeared after copy")?;
            timestamps.copy_prefix = Some(prefix);
            self.native
                .gpu
                .with_device(editor_window_packet(&self.native.window), |scope| {
                    timestamps.begin_mapping(scope)
                })
        };
        mapping
            .map_err(|error| host_refusal(self.native, "original Host map request", error))??;
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let mut map_succeeded = false;
        loop {
            self.work_maintenance(host_completions)?;
            let polled = {
                let timestamps = self
                    .native
                    .primary_gpu_timestamps
                    .as_ref()
                    .ok_or("primary GPU timestamp custody disappeared during polling")?;
                self.native
                    .gpu
                    .with_device(editor_window_packet(&self.native.window), |scope| {
                        timestamps.check_scope(&scope)?;
                        scope.device().poll(wgpu::PollType::Poll).map_err(|error| {
                            format!("original timestamp device poll refused: {error}")
                        })?;
                        Ok::<_, String>(())
                    })
            };
            polled
                .map_err(|error| host_refusal(self.native, "original Host map poll", error))??;
            let timestamps = self
                .native
                .primary_gpu_timestamps
                .as_ref()
                .ok_or("primary GPU timestamp custody disappeared after polling")?;
            let prefix = timestamps
                .copy_prefix
                .as_ref()
                .ok_or("primary GPU timestamp copy has no original submission prefix")?;
            if prefix.callback_abandoned() {
                return Err("primary GPU timestamp copy completion callback was abandoned".into());
            }
            let copy_complete = prefix.is_complete();
            if !map_succeeded {
                match timestamps
                    .map_callback
                    .as_ref()
                    .ok_or("primary GPU timestamp map has no callback receiver")?
                    .try_recv()
                {
                    Ok(Ok(())) => map_succeeded = true,
                    Ok(Err(error)) => {
                        return Err(format!(
                            "primary GPU timestamp map callback refused: {error}"
                        ));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("primary GPU timestamp map callback was abandoned".into());
                    }
                }
            }
            if copy_complete && map_succeeded {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "primary GPU timestamp readback deadline expired: original_copy_complete={copy_complete}, actual_map_callback_succeeded={map_succeeded}; resources retained for original native cleanup"
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let read = {
            let timestamps = self
                .native
                .primary_gpu_timestamps
                .as_mut()
                .ok_or("primary GPU timestamp custody disappeared before mapped read")?;
            self.native
                .gpu
                .with_device(editor_window_packet(&self.native.window), |scope| {
                    timestamps.check_scope(&scope)?;
                    let intervals_ns = timestamps.read_intervals()?;
                    Ok::<_, String>(PrimaryGpuObservation {
                        timestamp_period_ns: timestamps.timestamp_period_ns,
                        intervals_ns,
                        readback_ns: elapsed_ns(readback_started),
                    })
                })
        };
        let observed =
            read.map_err(|error| host_refusal(self.native, "original Host mapped read", error))??;
        Ok(Some(observed))
    }
}

impl PrimaryGpuObservation {
    pub(super) fn report(&self) {
        let mut sorted = self.intervals_ns.clone();
        sorted.sort_unstable_by(f64::total_cmp);
        let percentiles = [50, 95, 99, 100]
            .map(|percent| sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]);
        println!(
            "NATIVE_GPU primary_paint_markers requested_features=TIMESTAMP_QUERY|TIMESTAMP_QUERY_INSIDE_ENCODERS timestamp_period_ns={:?} pairs={}/{SAMPLE_FRAMES} queries={QUERY_COUNT} interval_ns_p50_p95_p99_max={percentiles:?} actual_map_callback_succeeded=true original_copy_prefix_complete=true readback_outside_cpu_and_completion_timers_ns={}",
            self.timestamp_period_ns,
            self.intervals_ns.len(),
            self.readback_ns
        );
        println!(
            "NATIVE_GPU scope=primary_native_UI_composition_paint_encoder_markers excluded=Game_and_image_producer_GPU_work,acquire,present,scanout,other_windows full_R11a_GPU_attribution=false exact_draw_and_presentation_proof=existing_independent_receipts Dear_private_upload_bytes=unmeasured"
        );
        println!(
            "NATIVE_GPU limitation=wgpu30_encoder_commands_may_reorder_across_timestamp_writes_backend_and_driver_dependent strict_isolated_execution=false extra_sampled_passes=0 extra_sampled_submissions=0 serialization_added=false cpu_and_completion_lane=instrumented_not_ordinary_baseline"
        );
        println!(
            "NATIVE_GPU bounded_pairs={SAMPLE_FRAMES} resolve_stride_bytes={RESOLVE_STRIDE} diagnostic_resolve_buffer_bytes={BUFFER_BYTES} diagnostic_readback_buffer_bytes={BUFFER_BYTES} diagnostic_buffers_outside_backend_payload_accounting=true setup_submissions_outside_samples=1 end_readback_copy_submissions=1 ownership=original_Native_until_actual_graphics_drain"
        );
    }
}
