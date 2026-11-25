//! One independent public offscreen Host for at most three exact original inputs.
//!
//! The caller retains its copied Project through this operation. Original Runtime/World/Scene
//! retirement is independently checked before acquiring the oracle device. This is image replay,
//! not a swapchain/display capture, GPU execution timing, hardware-busy measurement or leak proof.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    render::{RenderImage2d, RenderImagePacket},
    render_wgpu::{
        TextureAlpha, TextureInterpretation, TextureTransfer, WgpuImageOutput, WgpuRenderHost,
        WgpuRenderProducer, WgpuSubmissionPrefix,
    },
};
use serde_json::{Value, json};
use two_level_game::levels::LevelId;

use super::{MAX_REPLAYS, MAX_SEALS, PresentedTileFrame, WINDOW_EXTENT, oracle, require};

const TIMEOUT: Duration = Duration::from_secs(10);
const RESOURCE_LIMIT: u64 = 64 * 1024 * 1024;
const CANDIDATE_METADATA_LIMIT: u64 = 128 * 1024 * 1024;

/// Replays Meadow/Cavern (and optionally a final Meadow) after original World retirement.
/// Input order is unrestricted: each original frame gets a new producer on this one Host.
/// Polls are finite; the native child process still needs the caller's wall-clock watchdog.
pub fn replay(frames: &[PresentedTileFrame], population: usize) -> Result<Value, String> {
    require(
        (2..=MAX_REPLAYS).contains(&frames.len()),
        "tile replay requires two or three retained original Meadow/Cavern presentations",
    )?;
    oracle::tile_side(population)?;
    let first = &frames[0].presentation;
    let mut meadow = false;
    let mut cavern = false;
    for (index, frame) in frames.iter().enumerate() {
        require(
            frame.original_source_retired(),
            "retire the ORIGINAL Runtime World and original Scene owner before tile replay",
        )?;
        oracle::assert_ground(frame, population)?;
        let original = frame.presentation;
        require(
            original.runtime == first.runtime
                && original.window == first.window
                && original.backend_instance == first.backend_instance
                && original.device_epoch == first.device_epoch
                && frames[..index].iter().all(|other| {
                    other.presentation.app_frame != original.app_frame
                        && other.presentation.render_frame != original.render_frame
                }),
            "tile replay received mixed original owners or duplicate App/render frames",
        )?;
        match frame
            .snapshot()
            .level
            .ok_or("retained original frame has no level")?
            .level()
        {
            LevelId::Meadow => meadow = true,
            LevelId::Cavern => cavern = true,
        }
    }
    require(
        meadow && cavern,
        "tile replay requires actual original presentations of both levels",
    )?;

    let mut owner = ReplayOwner {
        host: WgpuRenderHost::offscreen()
            .with_resource_byte_limit(RESOURCE_LIMIT)
            .with_candidate_metadata_byte_limit(CANDIDATE_METADATA_LIMIT),
        producers: Vec::with_capacity(MAX_REPLAYS),
        outputs: Vec::with_capacity(MAX_REPLAYS),
        prefixes: Vec::with_capacity(16),
        readback_buffers: Vec::with_capacity(1),
    };
    let result = guarded("tile image replay", || run(&mut owner, frames, population));
    let cleanup = guarded("tile oracle cleanup", || owner.cleanup());
    match (result, cleanup) {
        (Ok(mut report), Ok(cleanup)) => {
            report["cleanup"] = cleanup;
            Ok(report)
        }
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; original oracle cleanup also failed: {cleanup}"
        )),
    }
}

struct ReplayOwner {
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    outputs: Vec<WgpuImageOutput>,
    prefixes: Vec<WgpuSubmissionPrefix>,
    readback_buffers: Vec<wgpu::Buffer>,
}

impl ReplayOwner {
    fn retire_producers(&mut self) {
        for producer in &self.producers {
            if let Some(prefix) = self.host.retire_producer(producer) {
                self.prefixes.push(prefix);
            }
        }
    }

    fn cleanup(&mut self) -> Result<Value, String> {
        self.retire_producers();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let mut prefixes_complete = true;
            for prefix in &self.prefixes {
                let complete = if prefix.callback_abandoned() {
                    prefix.recover_abandoned()
                } else {
                    prefix.poll()
                }
                .map_err(|error| error.to_string())?;
                prefixes_complete &= complete;
            }
            if prefixes_complete {
                break;
            }
            require(
                Instant::now() < deadline,
                "tile oracle original prefixes did not retire within ten seconds",
            )?;
            thread::sleep(Duration::from_millis(1));
        }
        let prefixes = self.prefixes.len();
        let abandoned = self
            .prefixes
            .iter()
            .any(WgpuSubmissionPrefix::callback_abandoned);
        self.prefixes.clear();
        self.outputs.clear();
        self.producers.clear();
        while !self
            .host
            .poll_retirement()
            .map_err(|error| error.to_string())?
        {
            require(
                Instant::now() < deadline,
                "original tile oracle Host did not retire within ten seconds",
            )?;
            thread::sleep(Duration::from_millis(1));
        }
        for buffer in &self.readback_buffers {
            buffer.unmap();
        }
        self.readback_buffers.clear();
        let bytes = self.host.retained_resource_bytes();
        let metadata = self.host.retained_candidate_metadata_bytes();
        let faults = self.host.cleanup_fault_count();
        require(
            bytes == 0 && metadata == 0,
            "retired tile oracle Host still accounts retained resource/candidate bytes",
        )?;
        require(
            faults == 0 && !abandoned,
            "tile oracle encountered cleanup faults or abandoned original callbacks",
        )?;
        Ok(json!({
            "original_producers_retired": true,
            "original_submission_prefixes_complete": true,
            "observed_prefix_tokens": prefixes,
            "original_host_retired": true,
            "retained_resource_bytes": bytes,
            "retained_candidate_metadata_bytes": metadata,
            "cleanup_fault_count": faults,
            "callback_abandoned": abandoned,
        }))
    }
}

fn run(
    owner: &mut ReplayOwner,
    frames: &[PresentedTileFrame],
    population: usize,
) -> Result<Value, String> {
    let mut reports = Vec::with_capacity(frames.len());
    let mut originals = Vec::with_capacity(frames.len());
    let mut peak_observed_bytes = 0;
    let mut replay_adapter = None;
    for (index, frame) in frames.iter().enumerate() {
        owner.producers.push(
            owner
                .host
                .create_producer()
                .map_err(|error| error.to_string())?,
        );
        owner
            .host
            .begin_frame(index as u64 + 1)
            .map_err(|error| error.to_string())?;
        require(
            owner.host.backend_instance_id() != frame.presentation.backend_instance,
            "tile replay must use an independent Host, never the original desktop owner",
        )?;
        let adapter = owner
            .host
            .adapter_summary()
            .ok_or("tile oracle Host has no actual adapter observation")?;
        require(
            matches!(adapter.backend(), "dx12" | "vulkan" | "metal")
                && matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu")
                && adapter.backend() == frame.adapter.backend()
                && adapter.vendor() == frame.adapter.vendor()
                && adapter.device() == frame.adapter.device()
                && adapter.name() == frame.adapter.name(),
            "independent tile replay selected a different or software adapter",
        )?;
        let adapter = json!({
            "name": adapter.name(), "backend": adapter.backend(),
            "device_type": adapter.device_type(), "vendor": adapter.vendor(), "device": adapter.device(),
        });
        if let Some(previous) = &replay_adapter {
            require(
                previous == &adapter,
                "tile replay Host changed adapter during retained-output checks",
            )?;
        }
        replay_adapter.get_or_insert(adapter);

        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().map_err(|error| error.to_string())?,
            WINDOW_EXTENT,
        )
        .map_err(|error| error.to_string())?;
        let original = &frame.input;
        let input = original
            .with_image_target(target)
            .map_err(|error| error.to_string())?;
        require(
            input.packet().source_generation() == original.packet().source_generation()
                && input.packet().frame_index() == original.packet().frame_index()
                && input.installation() == original.installation()
                && input.packet().views() == original.packet().views()
                && input.packet().world_canvas() == original.packet().world_canvas()
                && input.packet().captured_time() == original.packet().captured_time()
                && input.world_input_mapping() == original.world_input_mapping()
                && std::ptr::eq(
                    input
                        .captured::<nara::render_wgpu::WgpuStockPolicy>()
                        .ok_or("adapted tile input lost its stock capture")?,
                    original
                        .captured::<nara::render_wgpu::WgpuStockPolicy>()
                        .ok_or("original tile input lost its stock capture")?,
                ),
            "image adaptation changed original source/views/canvas/time/policy/capture meaning",
        )?;
        let producer = &owner.producers[index];
        let reservation = owner
            .host
            .reserve_image_target(producer, input.packet().source_generation(), target)
            .map_err(|error| error.to_string())?;
        owner
            .host
            .activate_image_target(producer, reservation)
            .map_err(|error| error.to_string())?;
        let candidate = owner
            .host
            .prepare(producer, &input)
            .map_err(|error| error.to_string())?;
        let published = owner
            .host
            .submit_candidate(producer, candidate)
            .map_err(|failure| failure.error.to_string())?;
        owner
            .prefixes
            .push(published.outputs().submitted_prefix().clone());
        owner.outputs.push(published.into_image());
        let output = &owner.outputs[index];
        let receipt = output.receipt();
        require(
            receipt.producer_id() == producer.id()
                && receipt.target() == target
                && receipt.source_generation() == original.packet().source_generation()
                && receipt.render_transaction_frame_index() == frame.presentation.render_frame
                && receipt.backend_instance_id() == owner.host.backend_instance_id()
                && receipt.device_epoch() == owner.host.device_epoch()
                && receipt.target_generation() > 0
                && receipt.output_version() > 0
                && owner.outputs[..index].iter().all(|previous| {
                    previous.receipt().producer_id() != receipt.producer_id()
                        && previous.receipt().output_version() != receipt.output_version()
                }),
            "tile image receipt does not name the exact original source and new independent producer/output",
        )?;
        let bytes = readback(
            &mut owner.host,
            output,
            &mut owner.prefixes,
            &mut owner.readback_buffers,
        )?;
        let level = frame
            .snapshot()
            .level
            .ok_or("original captured tile frame has no level")?
            .level();
        let checks = oracle::check_pixels(&bytes, population, level)?;
        reports.push(json!({
            "level": level,
            "original_runtime_generation": frame.presentation.runtime.get(),
            "original_app_frame": frame.presentation.app_frame,
            "original_render_frame": frame.presentation.render_frame,
            "original_window": frame.presentation.window.id.get(),
            "original_surface_generation": frame.presentation.surface_generation,
            "original_backend_instance": frame.presentation.backend_instance,
            "original_device_epoch": frame.presentation.device_epoch,
            "original_scene_instance": format!("{:?}", frame.game.instance.instance_id()),
            "original_source": format!("{:?}", frame.game.source),
            "original_source_stamp": {
                "schema": format!("{:?}", frame.game.stamp.schema_fingerprint()),
                "manifest": format!("{:?}", frame.game.stamp.manifest_digest()),
                "content": format!("{:?}", frame.game.stamp.content_digest()),
            },
            "original_source_and_world_retired": frame.original_source_retired(),
            "original_source_runtime_matched": true,
            "original_capture_retained_by_arc": true,
            "image_target_only_adaptation": true,
            "replay_producer": receipt.producer_id().get(),
            "replay_backend_instance": receipt.backend_instance_id(),
            "replay_device_epoch": receipt.device_epoch(),
            "replay_output_version": receipt.output_version(),
            "replay_target_generation": receipt.target_generation(),
            "replay_render_frame": receipt.render_transaction_frame_index(),
            "replay_source_matches_original": receipt.source_generation() == original.packet().source_generation(),
            "submitted_prefix_complete": owner.prefixes.last().is_some_and(WgpuSubmissionPrefix::is_complete),
            "rgba8_digest": format!("{:?}", ContentDigest::of_bytes(&bytes)),
            "pixels": checks,
        }));
        originals.push(bytes);
        peak_observed_bytes = peak_observed_bytes.max(owner.host.retained_resource_bytes());
    }

    for producer in &owner.producers {
        let prefix = owner.host.retire_producer(producer).ok_or(
            "published tile producer lost its original submission prefix during retirement",
        )?;
        owner.prefixes.push(prefix);
    }
    for (index, output) in owner.outputs.iter().enumerate() {
        require(
            output.is_current(),
            "retiring a tile producer invalidated its retained immutable output",
        )?;
        let reread = readback(
            &mut owner.host,
            output,
            &mut owner.prefixes,
            &mut owner.readback_buffers,
        )?;
        require(
            reread == originals[index],
            "retained tile output bytes changed after later publication or producer retirement",
        )?;
        reports[index]["retained_after_later_publications_and_all_producer_retirements"] =
            json!(true);
        reports[index]["reread_exact_bytes_equal"] = json!(true);
        reports[index]["reread_prefix_complete"] = json!(true);
    }
    require(
        owner.prefixes.iter().all(|prefix| {
            prefix.is_complete()
                && !prefix.callback_abandoned()
                && prefix.cleanup_fault_count() == 0
        }),
        "tile replay has incomplete/faulted original render, readback or retirement prefixes",
    )?;
    let retained_bytes = owner.host.retained_resource_bytes();
    peak_observed_bytes = peak_observed_bytes.max(retained_bytes);
    Ok(json!({
        "format": "two-level-tile-original-input-replay/v1",
        "accepted": true,
        "population": population,
        "frames": reports,
        "adapter": replay_adapter,
        "original_world_and_sources_retired_before_replay": true,
        "offscreen_host_count": 1,
        "producer_per_original_frame": true,
        "input_order_may_be_nonmonotonic": true,
        "physical_extent": [960, 540],
        "canvas_extent": [320, 180],
        "format_and_interpretation": "Rgba8Unorm / linear / premultiplied",
        "fixed_original_png_digest": format!("{:?}", oracle::PNG_DIGEST),
        "same_encoder_image_copy_and_original_prefix_wait": true,
        "retained_outputs_exact_after_later_publication_and_producer_retirement": true,
        "memory": {
            "maximum_capture_ledger_frames": MAX_SEALS,
            "retained_selected_frames": frames.len(),
            "maximum_selected_frames": MAX_REPLAYS,
            "host_logical_resource_byte_limit": RESOURCE_LIMIT,
            "candidate_metadata_byte_limit": CANDIDATE_METADATA_LIMIT,
            "observed_host_bytes_with_retained_outputs": retained_bytes,
            "peak_observed_host_bytes_between_operations": peak_observed_bytes,
            "retained_baseline_rgba8_bytes": originals.iter().map(Vec::len).sum::<usize>(),
            "maximum_single_external_readback_buffer_bytes": 960 * 540 * 4,
            "scope": "Host-accounted logical native payload plus separately stated CPU/readback buffers; excludes driver/pipeline overhead, physical residency and process RSS; point observations are not an allocation peak or leak bound",
        },
        "scope": "independent replay of original native-presented inputs; finite selected ground pixel centers and level clears; not full rasterization, swapchain/display capture, GPU busy time or a production performance claim",
        "gpu_poll_deadline_ms": TIMEOUT.as_millis(),
    }))
}

fn readback(
    host: &mut WgpuRenderHost,
    output: &WgpuImageOutput,
    prefixes: &mut Vec<WgpuSubmissionPrefix>,
    buffers: &mut Vec<wgpu::Buffer>,
) -> Result<Vec<u8>, String> {
    let descriptor = output.descriptor();
    require(
        output.extent() == WINDOW_EXTENT
            && output.format() == wgpu::TextureFormat::Rgba8Unorm
            && matches!(
                descriptor.interpretation,
                TextureInterpretation::Color {
                    transfer: TextureTransfer::Linear,
                    alpha: TextureAlpha::Premultiplied,
                }
            ),
        "tile readback requires the same physical linear premultiplied RGBA8 image",
    )?;
    let row = WINDOW_EXTENT.width * 4;
    let padded =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    require(
        buffers.is_empty(),
        "a previous original tile readback buffer is still pending",
    )?;
    let (buffer, prefix) = host
        .submit_image_use(std::slice::from_ref(output), |device, _, encoder, views| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("two-level-original-tile-input-readback"),
                size: u64::from(padded) * u64::from(WINDOW_EXTENT.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            buffers.push(buffer.clone());
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(WINDOW_EXTENT.height),
                    },
                },
                wgpu::Extent3d {
                    width: WINDOW_EXTENT.width,
                    height: WINDOW_EXTENT.height,
                    depth_or_array_layers: 1,
                },
            );
            Ok(buffer)
        })
        .map_err(|error| error.to_string())?;
    prefixes.push(prefix.clone());
    let result = (|| {
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        let deadline = Instant::now() + TIMEOUT;
        let mut mapped = false;
        loop {
            let complete = prefix.poll().map_err(|error| error.to_string())?;
            host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            if !mapped {
                match receiver.try_recv() {
                    Ok(result) => {
                        result.map_err(|error| error.to_string())?;
                        mapped = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("original tile readback map callback disappeared".into());
                    }
                }
            }
            if mapped && complete {
                break;
            }
            require(
                Instant::now() < deadline,
                "original tile image copy/map exceeded ten seconds",
            )?;
            thread::sleep(Duration::from_millis(1));
        }
        let mapped = buffer
            .get_mapped_range(..)
            .map_err(|error| error.to_string())?;
        require(
            mapped.len() == padded as usize * WINDOW_EXTENT.height as usize,
            "tile readback mapped an unexpected buffer size",
        )?;
        let mut bytes = Vec::with_capacity(row as usize * WINDOW_EXTENT.height as usize);
        for scanline in mapped.chunks_exact(padded as usize) {
            bytes.extend_from_slice(&scanline[..row as usize]);
        }
        Ok(bytes)
    })();
    buffer.unmap();
    if result.is_ok() {
        buffers.clear();
    }
    result
}

fn guarded<T>(label: &str, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => {
            let detail = if let Some(message) = payload.downcast_ref::<String>() {
                message.clone()
            } else if let Some(message) = payload.downcast_ref::<&str>() {
                (*message).to_owned()
            } else {
                "non-string panic payload".into()
            };
            Err(format!("{label} panicked: {detail}"))
        }
    }
}
