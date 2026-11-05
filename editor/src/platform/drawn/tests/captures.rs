use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{self, BufWriter, Write},
    path::PathBuf,
};

use nara::render::{DisplayMapping2d, DisplayScale};
use nara_render_wgpu::WgpuCompositeColor;
use nara_stock_composition_consumer::oracle::Image;
use nara_ui_kernel::{UiFrame, UiPaintPrimitive};

use super::{TestResult, Workflow};

impl Workflow<'_> {
    pub(super) fn capture_workspace(&mut self, label: &str) -> TestResult {
        let Some(directory) =
            std::env::var_os("NARA_DRAWN_CAPTURE_DIR").filter(|directory| !directory.is_empty())
        else {
            return Ok(());
        };
        (|| -> io::Result<()> {
            if label.is_empty()
                || label.len() > 64
                || !label.as_bytes()[0].is_ascii_alphanumeric()
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(io::Error::other(
                    "Capture labels require 1..=64 ASCII letters, digits, hyphens or underscores, starting with a letter or digit",
                ));
            }
            let size = self.native.window.inner_size();
            let scale = self.native.window.scale_factor();
            if size.width == 0
                || size.height == 0
                || size.width > 2048
                || size.height > 2048
                || !scale.is_finite()
                || scale <= 0.0
            {
                return Err(io::Error::other(
                    "Capture requires a nonempty window of at most 2048 by 2048 physical pixels",
                ));
            }
            let logical = size.to_logical::<f32>(scale);
            if self.closed
                || self.native.closing
                || self.native.recovery.is_some()
                || self.native.presented.is_empty()
                || self.native.presented.len() > 16
                || self.native.presented.len() != self.native.surfaces.len()
            {
                return Err(io::Error::other(
                    "Capture requires 1..=16 current presented panels in the original live workspace",
                ));
            }
            let mut workspace = Image {
                width: size.width,
                height: size.height,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                pixels: vec![[0, 0, 0, 255]; (size.width * size.height) as usize],
            };
            let mut metadata = Vec::new();
            writeln!(metadata, "label={label}")?;
            writeln!(metadata, "display_frame={}", self.native.display_frame)?;
            writeln!(
                metadata,
                "window_physical_size={:?}",
                [size.width, size.height],
            )?;
            writeln!(metadata, "window_scale_factor={scale:?}")?;
            writeln!(metadata, "panel_count={}", self.native.presented.len())?;
            writeln!(metadata, "rgba_format=Rgba8UnormSrgb")?;
            writeln!(
                metadata,
                "composition=original GPU panel texels at original fitted bounds; nearest sampling; black window clear",
            )?;
            writeln!(
                metadata,
                "ppm=P6 RGB; premultiplied RGB retained; alpha discarded",
            )?;
            writeln!(
                metadata,
                "scope=panel output readback; not swapchain or OS capture; no performance evidence",
            )?;
            writeln!(
                metadata,
                "appearance=resolved original frame paint, geometry and typography",
            )?;
            writeln!(metadata, "appearance_recipe_inputs={:?}", self.native.appearance)?;
            for (index, font) in self.native.fonts.iter().enumerate() {
                writeln!(
                    metadata,
                    "font[{index}]=family:{:?} face_index:{} id:{:?} bytes:{}",
                    font.family_name(),
                    font.face_index(),
                    font.id(),
                    font.byte_len(),
                )?;
            }
            let graphics = self
                .native
                .graphics
                .as_mut()
                .ok_or_else(|| io::Error::other("The original GPU Host is closed"))?;
            if graphics.frame != self.native.display_frame {
                return Err(io::Error::other("The original display frame is inconsistent"));
            }
            for (index, (slot, surface)) in self.native.surfaces.iter().enumerate() {
                let frame = self
                    .native
                    .presented
                    .get(slot)
                    .ok_or_else(|| io::Error::other("The original panel is not presented"))?;
                if !surface.token.is_current(&self.native.application)
                    || frame.environment().surface_id() != surface.surface
                    || frame.environment().scale_factor() != surface.scale
                    || surface.scale != scale as f32
                    || surface.bounds
                        != super::super::panel_bounds(
                            &surface.kind,
                            logical.width,
                            logical.height,
                        )
                {
                    return Err(io::Error::other("The original panel placement is stale"));
                }
                let (display, pixels) = graphics
                    .capture_panel_pixels_for_test(*slot, frame)
                    .map_err(io::Error::other)?;
                let mapping = DisplayMapping2d::fit(
                    display.output().extent(),
                    surface.bounds,
                    surface.scale,
                    DisplayScale::Fit,
                )
                .ok_or_else(|| io::Error::other("The original panel placement is invalid"))?;
                let accepted = graphics
                    .journal(*slot)
                    .ok_or_else(|| io::Error::other("The original UI journal is absent"))?
                    .latest(frame.environment().surface_id())
                    .map_err(io::Error::other)?;
                writeln!(metadata, "\n[panel {index}]")?;
                writeln!(metadata, "instance={slot:?}")?;
                writeln!(metadata, "kind={:?}", surface.kind)?;
                writeln!(metadata, "logical_bounds={:?}", surface.bounds)?;
                writeln!(metadata, "surface_scale_factor={:?}", surface.scale)?;
                writeln!(metadata, "display_mapping={mapping:?}")?;
                writeln!(
                    metadata,
                    "physical_content_bounds={:?}",
                    [
                        mapping.content_rect().min.map(|value| value * surface.scale),
                        mapping.content_rect().max.map(|value| value * surface.scale),
                    ],
                )?;
                writeln!(
                    metadata,
                    "presented_pixel_size={:?}",
                    [pixels.width, pixels.height],
                )?;
                writeln!(metadata, "output_format={:?}", pixels.format)?;
                writeln!(metadata, "display_occurrence={}", display.occurrence().get())?;
                writeln!(metadata, "submitted_image_receipt={:?}", display.receipt())?;
                writeln!(metadata, "ui_presentation_receipt={:?}", accepted.receipt())?;
                writeln!(
                    metadata,
                    "output_semantic={}",
                    if display.output().texture::<WgpuCompositeColor>().is_ok() {
                        "WgpuCompositeColor"
                    } else {
                        "WgpuWorldColor"
                    },
                )?;
                write_frame_metadata(&mut metadata, frame)?;
                let pixels = if pixels.format.is_srgb() {
                    pixels
                } else {
                    pixels.convert(wgpu::TextureFormat::Rgba8UnormSrgb)
                };
                compose_panel(&mut workspace, &pixels, mapping);
            }
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory)?;
            let image_path = directory.join(format!("drawn-{label}.ppm"));
            let metadata_path = directory.join(format!("drawn-{label}.txt"));
            let mut image = BufWriter::new(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&image_path)?,
            );
            write!(image, "P6\n{} {}\n255\n", workspace.width, workspace.height)?;
            let mut row = Vec::with_capacity(workspace.width as usize * 3);
            for pixels in workspace.pixels.chunks_exact(workspace.width as usize) {
                row.clear();
                for pixel in pixels {
                    row.extend_from_slice(&pixel[..3]);
                }
                image.write_all(&row)?;
            }
            image.flush()?;
            let mut text = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&metadata_path)?;
            text.write_all(&metadata)?;
            eprintln!("drawn native capture: {}", image_path.display());
            Ok(())
        })()
        .map_err(|error| format!("Drawn workspace capture {label:?}: {error}"))
    }
}

fn compose_panel(workspace: &mut Image, panel: &Image, mapping: DisplayMapping2d) {
    let bounds = mapping.panel_rect();
    let scale = mapping.pixels_per_point();
    let min = bounds
        .min
        .map(|value| (value * scale).floor().max(0.0) as u32);
    let max = [
        (bounds.max[0] * scale)
            .ceil()
            .clamp(0.0, workspace.width as f32) as u32,
        (bounds.max[1] * scale)
            .ceil()
            .clamp(0.0, workspace.height as f32) as u32,
    ];
    for y in min[1]..max[1] {
        for x in min[0]..max[0] {
            let point = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
            let Some(target) = mapping.panel_to_target(point) else {
                continue;
            };
            let source_x = (target[0].floor() as u32).min(panel.width - 1);
            let source_y = (target[1].floor() as u32).min(panel.height - 1);
            workspace.pixels[(y * workspace.width + x) as usize] =
                panel.pixels[(source_y * panel.width + source_x) as usize];
        }
    }
}

fn write_frame_metadata(metadata: &mut impl Write, frame: &UiFrame) -> io::Result<()> {
    writeln!(metadata, "ui_frame_revision={}", frame.revision().get())?;
    writeln!(
        metadata,
        "ui_source_revision={}",
        frame.source_revision().get(),
    )?;
    writeln!(metadata, "environment={:?}", frame.environment())?;
    for (index, primitive) in frame.paint().iter().enumerate() {
        write!(
            metadata,
            "paint[{index}]=key:{:?} rect:{:?} clip:{:?} ",
            primitive.key(),
            primitive.rect(),
            primitive.clip_rect(),
        )?;
        match primitive {
            UiPaintPrimitive::Rect {
                brush,
                border,
                radii,
                ..
            } => {
                writeln!(
                    metadata,
                    "rect brush:{brush:?} border:{border:?} radii:{radii:?}",
                )?;
            }
            UiPaintPrimitive::Text { color, layout, .. } => {
                let faces: BTreeSet<_> = layout
                    .glyphs()
                    .iter()
                    .map(|glyph| glyph.key().face_id())
                    .collect();
                writeln!(
                    metadata,
                    "text color:{color:?} typography:{:?} size:{:?} faces:{faces:?} lines:{:?}",
                    layout.typography(),
                    [layout.width(), layout.height()],
                    layout.lines(),
                )?;
            }
            UiPaintPrimitive::Mesh { mesh, .. } => {
                let colors: BTreeSet<_> = mesh
                    .vertices()
                    .iter()
                    .map(|vertex| vertex.color().components().map(f32::to_bits))
                    .collect();
                writeln!(
                    metadata,
                    "mesh vertices:{} indices:{} rgba_f32_bits:{colors:?}",
                    mesh.vertices().len(),
                    mesh.indices().len(),
                )?;
            }
        }
    }
    Ok(())
}
