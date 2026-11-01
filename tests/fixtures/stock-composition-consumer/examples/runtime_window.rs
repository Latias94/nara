#![forbid(unsafe_code)]

//! Ordinary Source DesktopRun, original UI sidecar, then replay on an independent offscreen Host.

use std::{
    cell::RefCell,
    io,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::AppExit,
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunIntent, DesktopRunOptions,
        DesktopRunOutcome, open_explicit_product_project_root,
    },
    render::{RenderImage2d, RenderImagePacket},
    render_wgpu::{WgpuCompositeColor, WgpuRenderHost},
    ui_render::advanced::{UiPacketFill, UiPacketPrimitive, UiPacketRect},
};
use nara_stock_composition_consumer::{
    CompositionConfiguration, CompositionPolicy, RetainedInputs,
    oracle::{EFFECT_TOLERANCE, Image},
    readback, recipe,
};

#[path = "../tests/support/scene.rs"]
#[allow(dead_code)]
mod scene;

fn main() -> scene::Result<()> {
    let backend = std::env::var("WGPU_BACKEND")?;
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let recipe = recipe(CompositionConfiguration::default())?.add_plugin::<scene::HudPlugin>()?;
    let fixture = scene::SourceFixture::new(recipe.clone())?;
    let retained = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&retained);
    let deadline = Instant::now() + Duration::from_secs(30);
    let options = DesktopRunOptions::new().with_presentation_observer(move |observation| {
        if observation.now() >= deadline {
            return DesktopPresentationControl::Exit(AppExit::Requested);
        }
        if let Some(receipt) = observation.presented_frame() {
            let world = observation
                .world()
                .expect("presented Runtime's original World");
            let Some(input) = world.resource::<RetainedInputs>().0.iter().find(|input| {
                input.packet().frame_index() == receipt.render_transaction_frame_index()
            }) else {
                return DesktopPresentationControl::Continue;
            };
            assert_eq!(
                input.packet().source_generation().runtime_generation(),
                Some(observation.runtime_generation())
            );
            let capture = input
                .captured::<CompositionPolicy>()
                .expect("the recipe-selected external policy");
            if capture.stock.ui_packets().is_empty() {
                return DesktopPresentationControl::Continue;
            }
            let ui = nara::ui_render::advanced::latest_presented_ui_observation(world)
                .expect("the original HUD sidecar must complete through the product bridge");
            let facts = ui
                .wgpu()
                .expect("the actual original UI Feature's submitted facts");
            assert_eq!(
                facts.render_transaction_frame_index(),
                receipt.render_transaction_frame_index()
            );
            assert!(
                capture
                    .stock
                    .ui_packets()
                    .iter()
                    .any(|packet| packet.identity() == ui.packet().identity())
            );
            assert_eq!(facts.submitted_quad_count(), 1);
            let adapter = observation
                .adapter_summary()
                .expect("actual native adapter");
            assert_eq!(adapter.backend(), backend);
            assert_ne!(adapter.device_type().to_ascii_lowercase(), "cpu");
            *observed.borrow_mut() = Some((input.clone(), receipt.backend_instance_id()));
            return DesktopPresentationControl::Exit(AppExit::Success);
        }
        DesktopPresentationControl::Continue
    });
    let mut run = DesktopRun::new(
        open_explicit_product_project_root(fixture.root())?,
        DesktopRunIntent::new()
            .with_recipe(recipe)
            .with_execution(options),
    );
    let mut report = run.execute();
    let deadline = Instant::now() + Duration::from_secs(10);
    while report.outcome() == DesktopRunOutcome::CleanupIncomplete && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
        report = run.execute();
    }
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Success)
        || report.diagnostics().has_errors()
    {
        return Err(io::Error::other(format!(
            "ordinary stock composition Runtime failed: {report:?}"
        ))
        .into());
    }
    let (original, native_host) = retained
        .borrow_mut()
        .take()
        .expect("one exact native presentation");
    drop(run);
    drop(fixture);
    // Replay the accepted original input, including its exact stock Sprite and HUD packets,
    // after its source root, Runtime, installation and native Host have retired.
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1)?;
    let producer = host.create_producer()?;
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        original.packet().target().extent(),
    )?;
    let input = original.with_image_target(target)?;
    assert_eq!(input.installation(), original.installation());
    assert_eq!(input.packet().views(), original.packet().views());
    assert_eq!(
        input.packet().source_generation(),
        original.packet().source_generation()
    );
    let reservation =
        host.reserve_image_target(&producer, input.packet().source_generation(), target)?;
    host.activate_image_target(&producer, reservation)?;
    let candidate = host.prepare(&producer, &input)?;
    let frame = host
        .submit_candidate(&producer, candidate)
        .map_err(|failure| failure.error)?;
    assert_eq!(
        frame.image().receipt().render_transaction_frame_index(),
        original.packet().frame_index()
    );
    assert_ne!(frame.image().receipt().backend_instance_id(), native_host);
    let capture = original
        .captured::<CompositionPolicy>()
        .expect("the original Runtime policy");
    assert!(original.packet().world_canvas().is_none());
    let composite = frame.final_texture::<WgpuCompositeColor>()?;
    let extent = original.packet().target().extent();
    let mut source = Image::solid(
        extent.width,
        extent.height,
        composite.descriptor().native.format,
        [0.0, 0.0, 1.0, 1.0],
    );
    // The authored Camera height is six world units; the Sprite is centered and four by two.
    // Compute coverage from pixel centers before observing GPU output, including native resize.
    // Rasterization includes top/left edges and excludes bottom/right edges at an exact tie.
    let half_sprite_width = extent.height as f64 / 3.0;
    let half_sprite_height = extent.height as f64 / 6.0;
    for y in 0..extent.height {
        for x in 0..extent.width {
            let horizontal = f64::from(x) + 0.5 - f64::from(extent.width) * 0.5;
            let vertical = f64::from(y) + 0.5 - f64::from(extent.height) * 0.5;
            let in_sprite = horizontal >= -half_sprite_width
                && horizontal < half_sprite_width
                && vertical >= -half_sprite_height
                && vertical < half_sprite_height;
            if in_sprite {
                source.pixels[(y * extent.width + x) as usize] = [255, 0, 0, 255];
            }
        }
    }
    let pre_ui = source
        .blur(capture.configuration.radius)
        .grade(capture.configuration.grade);
    let mut expected = pre_ui.clone();
    assert_eq!(capture.stock.ui_packets().len(), 1);
    let packet = &capture.stock.ui_packets()[0];
    let scale = packet.scale_factor();
    let edge = |point: f32| (point * scale).round() as i32;
    let rectangle = UiPacketRect::new(
        edge(1.0),
        edge(1.0),
        edge(2.0),
        edge((scene::HEIGHT - 1) as f32),
    )
    .expect("the authored HUD has nonempty physical coverage");
    let [
        UiPacketPrimitive::Rect {
            rect,
            fill: Some(UiPacketFill::Solid { color }),
            border: None,
            corner_radius,
            ..
        },
    ] = packet.primitives()
    else {
        panic!("the original Runtime HUD must be its one authored solid rectangle")
    };
    assert_eq!(
        *rect, rectangle,
        "normal layout must preserve the authored physical HUD mapping"
    );
    assert_eq!(*color, nara::ui::UiColor::rgba(1.0, 1.0, 1.0, 1.0));
    assert_eq!(*corner_radius, 0.0);
    let inside = |x: u32, y: u32| {
        let x = x as i32;
        let y = y as i32;
        x >= rectangle.min_x()
            && x < rectangle.max_x()
            && y >= rectangle.min_y()
            && y < rectangle.max_y()
    };
    for y in 0..extent.height {
        for x in 0..extent.width {
            if inside(x, y) {
                expected.pixels[(y * extent.width + x) as usize] = [255; 4];
            }
        }
    }
    let actual = readback::read(&mut host, &composite);
    expected.assert_matches(
        &actual,
        EFFECT_TOLERANCE,
        "retired original Runtime blur/grade/HUD capture",
    );
    let mut hud_pixels = 0;
    for y in 0..extent.height {
        for x in 0..extent.width {
            if inside(x, y) {
                assert_eq!(
                    actual.pixels[(y * extent.width + x) as usize],
                    [255; 4],
                    "original Runtime HUD stays exact at ({x}, {y})"
                );
                hud_pixels += 1;
            }
        }
    }
    assert!(hud_pixels > 0 && hud_pixels < actual.pixels.len());
    let prefix = host
        .retire_producer(&producer)
        .expect("original replay submission prefix");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !prefix.poll()? {
        if Instant::now() >= deadline {
            return Err(io::Error::other("offscreen replay prefix timed out").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    drop(frame);
    while !host.poll_retirement()? {
        if Instant::now() >= deadline {
            return Err(io::Error::other("offscreen replay retirement timed out").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    println!(
        "STOCK_RUNTIME_OK native_presentations=1 ordinary_hud=true original_ui_completed=true original_runtime_retired=true offscreen_replays=1 original_input=true numeric_original_hud=true"
    );
    Ok(())
}
