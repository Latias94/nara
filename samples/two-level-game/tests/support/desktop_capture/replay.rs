//! Independent finite GPU replay of the retained game input, after original source retirement.

use std::{
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    render::{Extent2d, RenderImage2d, RenderImagePacket},
    render_wgpu::{
        TextureAlpha, TextureTransfer, WgpuImageOutput, WgpuRenderHost, WgpuSubmittedImageReceipt,
    },
};
use two_level_game::resume::ResumeState;

use super::{PixelChecks, PresentedGameFrame, Result, WINDOW_EXTENT, oracle, require};

const GPU_TIMEOUT: Duration = Duration::from_secs(10);

/// Linear RGBA8 pixels from an independent oracle Host, not the original swapchain or scanout.
pub struct ReplayReadback {
    pub receipt: WgpuSubmittedImageReceipt,
    pub extent: Extent2d,
    pub rgba8: Vec<u8>,
    pub checks: PixelChecks,
}

/// Replays the exact stock capture with only an equal-sized execution-target adaptation.
/// Caller must finish original DesktopRun retirement first; this creates no production owner.
/// All polling is finite. Synchronous Host/device calls still require a parent process timeout.
pub fn replay_and_check(
    frame: &PresentedGameFrame,
    expected: &ResumeState,
) -> Result<ReplayReadback> {
    oracle::assert_frame(&frame.sealed, expected)?;
    require(
        frame.sealed.source_view.with_document(|_| ()).is_none(),
        "retire the original Scene/Runtime before independent GPU replay",
    )?;
    let mut host = WgpuRenderHost::offscreen();
    let result = replay(&mut host, frame, expected);
    let retired = retire(&mut host);
    if retired.is_err() {
        std::mem::forget(host);
    }
    match (result, retired) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(io::Error::other(format!(
            "game replay failed: {error}; original oracle Host retirement also failed: {cleanup}",
        ))
        .into()),
    }
}

fn replay(
    host: &mut WgpuRenderHost,
    frame: &PresentedGameFrame,
    expected: &ResumeState,
) -> Result<ReplayReadback> {
    let producer = host.create_producer()?;
    host.begin_frame(1)?;
    require(
        host.backend_instance_id() != frame.presentation.backend_instance,
        "the replay Host must be independent of the original desktop Host",
    )?;
    let target = RenderImagePacket::new(RenderImage2d::allocate_handle()?, WINDOW_EXTENT)?;
    let original = frame.input();
    let input = original.with_image_target(target)?;
    require(
        input.packet().source_generation() == original.packet().source_generation()
            && input.packet().frame_index() == original.packet().frame_index()
            && input.installation() == original.installation()
            && input.packet().views() == original.packet().views()
            && input.packet().world_canvas() == original.packet().world_canvas(),
        "image adaptation changed original source, frame, policy, View or canvas meaning",
    )?;
    let reservation =
        host.reserve_image_target(&producer, input.packet().source_generation(), target)?;
    host.activate_image_target(&producer, reservation)?;
    let candidate = host.prepare(&producer, &input)?;
    let published = host
        .submit_candidate(&producer, candidate)
        .map_err(|failure| failure.error)?;
    let output = published.image();
    let receipt = output.receipt();
    require(
        receipt.producer_id() == producer.id()
            && receipt.target() == target
            && receipt.source_generation() == original.packet().source_generation()
            && receipt.render_transaction_frame_index() == frame.presentation.render_frame
            && receipt.backend_instance_id() == host.backend_instance_id()
            && receipt.device_epoch() == host.device_epoch()
            && receipt.output_version() > 0
            && receipt.target_generation() > 0,
        "replayed image receipt lost the original input or independent Host/producer identity",
    )?;
    let rgba8 = readback(host, &output)?;
    let checks = oracle::check_pixels(&rgba8, expected)?;
    let prefix = host.retire_producer(&producer).ok_or_else(|| {
        io::Error::other("submitted game replay lost its original producer prefix")
    })?;
    require(
        prefix.poll()?,
        "readback did not complete the replay producer's original prefix",
    )?;
    Ok(ReplayReadback {
        receipt,
        extent: output.extent(),
        rgba8,
        checks,
    })
}

fn readback(host: &mut WgpuRenderHost, output: &WgpuImageOutput) -> Result<Vec<u8>> {
    let descriptor = output.descriptor();
    require(
        output.extent() == WINDOW_EXTENT
            && output.format() == wgpu::TextureFormat::Rgba8Unorm
            && descriptor.transfer == TextureTransfer::Linear
            && descriptor.alpha == TextureAlpha::Premultiplied,
        "game replay readback requires the original same-sized linear associated RGBA8 output",
    )?;
    let row_bytes = WINDOW_EXTENT.width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let (buffer, prefix) =
        host.submit_image_use(std::slice::from_ref(output), |device, _, encoder, views| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("two-level-game-original-input-replay-readback"),
                size: u64::from(padded_row) * u64::from(WINDOW_EXTENT.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
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
        })?;
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + GPU_TIMEOUT;
    let mut mapped = false;
    loop {
        let complete = prefix.poll()?;
        host.with_image(output, |device, _, _| device.poll(wgpu::PollType::Poll))??;
        if !mapped {
            match receiver.try_recv() {
                Ok(result) => {
                    result?;
                    mapped = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(io::Error::other("game replay map callback disappeared").into());
                }
            }
        }
        if mapped && complete {
            break;
        }
        require(
            Instant::now() < deadline,
            "game GPU replay/readback exceeded ten seconds",
        )?;
        thread::sleep(Duration::from_millis(1));
    }
    let result = (|| {
        let mapped = buffer.get_mapped_range(..)?;
        require(
            mapped.len() == padded_row as usize * WINDOW_EXTENT.height as usize,
            "game replay mapped a different buffer extent",
        )?;
        let mut rgba8 = Vec::with_capacity(row_bytes as usize * WINDOW_EXTENT.height as usize);
        for row in mapped.chunks_exact(padded_row as usize) {
            rgba8.extend_from_slice(&row[..row_bytes as usize]);
        }
        Ok(rgba8)
    })();
    buffer.unmap();
    result
}

fn retire(host: &mut WgpuRenderHost) -> Result<()> {
    let deadline = Instant::now() + GPU_TIMEOUT;
    while !host.poll_retirement()? {
        require(
            Instant::now() < deadline,
            "independent game oracle Host retirement exceeded ten seconds",
        )?;
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
