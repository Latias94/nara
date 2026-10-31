#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

#[path = "support/drawing_gpu.rs"]
mod drawing_gpu;
#[path = "support/drawing_model.rs"]
mod drawing_model;
#[path = "support/drawing_readback.rs"]
mod drawing_readback;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_render_wgpu::{
    WgpuRenderError, WgpuRenderHost, WgpuRenderHostError, WgpuRenderProgramError,
};

use drawing_gpu::{CapturedDrawing, DRAWING, Drawing, EFFECT, PATTERN, Pattern, SecondEffect};
use drawing_model::*;
use drawing_readback::{Rendered, await_retained, finish, settle};

#[test]
fn finite_oracle_exercises_clipping_letterbox_alpha_and_front_popup() {
    for placement in [Placement::first(), Placement::moved()] {
        assert!(placement.valid());
        assert_eq!(
            placement.uniform_bytes().len() as u64,
            DRAWING_UNIFORM_BYTES
        );
        let (pixels, coverage) = drawing(1, placement);
        assert_coverage(&coverage);
        assert_eq!(pixels.len(), (WIDTH * HEIGHT) as usize);
        assert!(pixels.iter().all(|p| p.iter().all(|v| v.is_finite())));
        assert!(pixels.iter().any(|p| p[3] > 0.0 && p[3] < 1.0));
        let no_alpha_zero_layer = Placement {
            transparent: [60.0, 60.0, 1.0, 1.0],
            ..placement
        };
        assert_eq!(
            pixels,
            drawing(1, no_alpha_zero_layer).0,
            "a premultiplied alpha-zero control must be an exact identity"
        );
        let no_popup = Placement {
            popup_color: [0.0; 4],
            ..placement
        };
        assert_ne!(
            pixels,
            drawing(1, no_popup).0,
            "front popup visibly changes covered pixels"
        );
    }
    assert_ne!(
        drawing(1, Placement::first()).0,
        drawing(2, Placement::first()).0
    );
    assert_ne!(
        drawing(1, Placement::first()).0,
        drawing(1, Placement::moved()).0
    );
    let pattern: Vec<_> = (0..SCENE_HEIGHT)
        .flat_map(|y| (0..SCENE_WIDTH).map(move |x| scene_pixel(x, y, 1)))
        .collect();
    assert!(pattern.iter().any(|p| p[3] == 0.0));
    assert!(pattern.iter().any(|p| p[3] == 0.25));
    assert!(pattern.iter().any(|p| p[3] == 0.5));
    assert!(pattern.iter().any(|p| p[3] == 1.0));
}

#[test]
fn captured_placement_rejects_nonfinite_geometry_and_invalid_premultiplication() {
    for dpi in [0.0, -1.0, f32::NAN, f32::INFINITY, 8.0] {
        assert!(
            !Placement {
                dpi,
                ..Placement::first()
            }
            .valid()
        );
    }
    for scene in [
        [0.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, -1.0],
        [f32::NAN, 0.0, 1.0, 1.0],
        [0.0, f32::INFINITY, 1.0, 1.0],
    ] {
        assert!(
            !Placement {
                scene,
                ..Placement::first()
            }
            .valid()
        );
    }
    for control_color in [
        [0.25, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.5],
        [0.0, 0.0, 0.0, f32::NAN],
        [0.0, 0.0, 0.0, 2.0],
    ] {
        assert!(
            !Placement {
                control_color,
                ..Placement::first()
            }
            .valid()
        );
    }
}

#[test]
#[ignore = "requires Windows, explicit WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0; missing adapter is a failure"]
fn retained_a1_composes_exact_pixels_while_a2_b_and_rejected_second_effect_interleave() {
    assert!(
        cfg!(target_os = "windows"),
        "native qualification requires Windows/DX12 hardware"
    );
    assert_eq!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12"),
        "explicitly select WGPU_BACKEND=dx12; an absent adapter cannot pass by skipping"
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "explicitly select hardware policy with NARA_WGPU_FORCE_FALLBACK=0"
    );
    let mut host = WgpuRenderHost::offscreen().with_resource_byte_limit(1024 * 1024);
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let c = host.create_producer().unwrap();
    let a_source = source();
    let b_source = source();
    let c_source = source();
    assert_ne!(a_source, b_source);
    assert_ne!(a.id(), b.id());
    let a_encodes = Arc::new(AtomicUsize::new(0));
    let b_encodes = Arc::new(AtomicUsize::new(0));
    let effect_prepares = Arc::new(AtomicUsize::new(0));
    let a_pattern = drawing_gpu::install(&mut host, &a, Pattern(Arc::clone(&a_encodes)));
    let b_pattern = drawing_gpu::install(&mut host, &b, Pattern(Arc::clone(&b_encodes)));
    let painter = drawing_gpu::install(&mut host, &c, Drawing);
    let lookalike = drawing_gpu::install(&mut host, &c, Drawing);
    let effect = drawing_gpu::install(&mut host, &c, SecondEffect(Arc::clone(&effect_prepares)));

    host.begin_frame(1)
        .expect("the original production Host must acquire the selected native GPU");
    let a1_program = drawing_gpu::pattern(a_source, 1, 1, &a_pattern);
    let a1_group = host
        .submit_program(&a, &a1_program)
        .expect("publish the actual A1 pattern");
    let a1 = Rendered::published(&a1_group, PATTERN, &a_pattern);
    assert_scene(&a1.read(&mut host), 1);
    drop(a1_group);
    let mut current_placement = Placement::first();
    let original = CapturedDrawing {
        scene: a1.output.clone(),
        placement: current_placement,
    };
    current_placement = Placement::moved();

    host.begin_frame(2).unwrap();
    let a2_program = drawing_gpu::pattern(a_source, 2, 2, &a_pattern);
    let a2_group = host
        .submit_program(&a, &a2_program)
        .expect("publish different successor A2");
    let a2 = Rendered::published(&a2_group, PATTERN, &a_pattern);
    assert_scene(&a2.read(&mut host), 2);
    drop(a2_group);
    let b1_program = drawing_gpu::pattern(b_source, 1, 3, &b_pattern);
    let b1_group = host
        .submit_program(&b, &b1_program)
        .expect("publish unrelated B1");
    let b1 = Rendered::published(&b1_group, PATTERN, &b_pattern);
    assert_scene(&b1.read(&mut host), 3);
    drop(b1_group);

    let c1_program = drawing_gpu::compose(c_source, 1, &original, &painter, None);
    let c1_group = host
        .submit_program(&c, &c1_program)
        .expect("compose the exact old A1 output");
    assert!(matches!(
        c1_group.report::<Drawing>(DRAWING, &lookalike),
        Err(WgpuRenderProgramError::FeatureState)
    ));
    let c1 = Rendered::published(&c1_group, DRAWING, &painter);
    let first_pixels = c1.read(&mut host);
    let (first_expected, coverage) = drawing(1, original.placement);
    assert_coverage(&coverage);
    assert_pixels(&first_pixels, &first_expected, WIDTH);
    assert_differs(
        &first_pixels,
        &drawing(2, original.placement).0,
        "sampling latest A2",
    );
    assert_differs(
        &first_pixels,
        &drawing(1, current_placement).0,
        "joining old receipt to current layout",
    );
    drop(c1_group);

    host.begin_frame(3).unwrap();
    let b2_program = drawing_gpu::pattern(b_source, 2, 6, &b_pattern);
    let b2_group = host
        .submit_program(&b, &b2_program)
        .expect("B advances independently");
    let b2 = Rendered::published(&b2_group, PATTERN, &b_pattern);
    assert_scene(&b2.read(&mut host), 6);
    drop(b2_group);
    let c2_program = drawing_gpu::compose(c_source, 2, &original, &painter, None);
    let c2_group = host
        .submit_program(&c, &c2_program)
        .expect("recompose the same immutable drawing");
    let c2 = Rendered::published(&c2_group, DRAWING, &painter);
    let repeated_pixels = c2.read(&mut host);
    assert_pixels(&repeated_pixels, &first_expected, WIDTH);
    assert_eq!(
        repeated_pixels, first_pixels,
        "repaint does not reinterpret the retained capture"
    );
    drop(c2_group);
    let moved = CapturedDrawing {
        scene: a1.output.clone(),
        placement: current_placement,
    };
    let c3_program = drawing_gpu::compose(c_source, 3, &moved, &painter, None);
    let c3_group = host
        .submit_program(&c, &c3_program)
        .expect("capture a new placement for the same old image");
    let c3 = Rendered::published(&c3_group, DRAWING, &painter);
    let (moved_expected, moved_coverage) = drawing(1, moved.placement);
    assert_coverage(&moved_coverage);
    assert_pixels(&c3.read(&mut host), &moved_expected, WIDTH);
    drop(c3_group);
    assert_eq!(
        a_encodes.load(Ordering::SeqCst),
        2,
        "composition never resubmits or advances A"
    );
    assert_eq!(a1.output.frame_index(), 1);
    assert_eq!(a2.output.frame_index(), 2);
    assert_eq!(a1.output.source_generation(), a_source);
    assert_eq!(a2.output.producer_id(), a.id());
    assert!(matches!(
        host.submit_program(&a, &a2_program),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::FrameAlreadyConsumed { frame_index: 2 }
        ))
    ));

    host.begin_frame(4).unwrap();
    let retained = host.retained_resource_bytes();
    assert_eq!(
        retained,
        4 * SCENE_BYTES + 3 * FINAL_BYTES,
        "only actual retained pixels remain after completed preparation and readback"
    );
    // Both full local attachments and Drawing's uniform fit; SecondEffect's 16-byte uniform
    // is one byte short. This reaches that second Feature's real public reserve_bytes call.
    let refusal_limit =
        retained + 2 * FINAL_BYTES + DRAWING_UNIFORM_BYTES + SMALL_UNIFORM_BYTES - 1;
    host = host.with_resource_byte_limit(refusal_limit);
    let pending = drawing_gpu::compose(c_source, 4, &original, &painter, Some(&effect));
    let error = host
        .submit_program(&c, &pending)
        .expect_err("the second effect must hit real budget pressure");
    assert!(matches!(
        error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::ResourceCapacity)
    ));
    assert!(error.is_deferred());
    assert_eq!(
        effect_prepares.load(Ordering::SeqCst),
        1,
        "the second effect was actually prepared"
    );
    await_retained(&mut host, 5, retained);
    assert_eq!(
        host.retained_resource_bytes(),
        retained,
        "failed whole group publishes no storage"
    );
    assert_eq!(
        c1.read(&mut host),
        first_pixels,
        "old composition survives refusal without regeneration"
    );
    let b3_program = drawing_gpu::pattern(b_source, 3, 7, &b_pattern);
    let b3_group = host
        .submit_program(&b, &b3_program)
        .expect("unrelated B3 publishes under the same refusing byte limit");
    let b3 = Rendered::published(&b3_group, PATTERN, &b_pattern);
    assert_scene(&b3.read(&mut host), 7);
    drop(b3_group);
    assert_eq!(b_encodes.load(Ordering::SeqCst), 3);
    assert_eq!(a_encodes.load(Ordering::SeqCst), 2);

    host = host.with_resource_byte_limit(1024 * 1024);
    host.begin_frame(6).unwrap();
    let retried = host.submit_program(&c, &pending).expect(
        "the same immutable program and capture index can retry after pressure is released",
    );
    let base = Rendered::published(&retried, DRAWING, &painter);
    let final_effect = Rendered::published(&retried, EFFECT, &effect);
    assert_eq!(final_effect.output.frame_index(), 4);
    assert_pixels(&base.read(&mut host), &first_expected, WIDTH);
    let attenuated: Vec<_> = first_expected
        .iter()
        .map(|pixel| [pixel[0] * 0.5, pixel[1] * 0.5, pixel[2] * 0.5, pixel[3]])
        .collect();
    assert_pixels(&final_effect.read(&mut host), &attenuated, WIDTH);
    assert_eq!(effect_prepares.load(Ordering::SeqCst), 2);
    drop(retried);

    host.begin_frame(7).unwrap();
    let invalid = CapturedDrawing {
        placement: Placement {
            dpi: f32::NAN,
            ..original.placement
        },
        ..original.clone()
    };
    let invalid_program = drawing_gpu::compose(c_source, 5, &invalid, &painter, Some(&effect));
    let before_invalid = host.retained_resource_bytes();
    host = host.with_resource_byte_limit(0);
    let error = host.submit_program(&c, &invalid_program).unwrap_err();
    assert!(matches!(
        error,
        WgpuRenderHostError::Program(WgpuRenderProgramError::Feature { .. })
    ));
    assert!(
        error.is_content_rejected() && !error.is_deferred(),
        "nonfinite placement preflight precedes native resource-budget refusal"
    );
    assert_eq!(effect_prepares.load(Ordering::SeqCst), 2);
    assert_eq!(host.retained_resource_bytes(), before_invalid);
    host = host.with_resource_byte_limit(1024 * 1024);
    assert_scene(&a1.read(&mut host), 1);
    assert_scene(&a2.read(&mut host), 2);
    assert_eq!(c1.read(&mut host), first_pixels);
    assert_eq!(a_encodes.load(Ordering::SeqCst), 2);
    assert_eq!(host.frame_transaction_stats().surface_acquire_attempts(), 0);
    assert_eq!(host.frame_transaction_stats().presents(), 0);

    for producer in [&a, &b, &c] {
        settle(
            &host
                .retire_producer(producer)
                .expect("each actual producer submitted its own prefix"),
        );
    }
    drop((a1, a2, b1, b2, b3, c1, c2, c3, base, final_effect));
    drop((
        original,
        moved,
        invalid,
        pending,
        invalid_program,
        c1_program,
        c2_program,
        c3_program,
    ));
    finish(&mut host);
    assert_eq!(
        host.retained_resource_bytes(),
        0,
        "all original output and use leases retire"
    );
}

fn assert_scene(actual: &[[f32; 4]], phase: u32) {
    let expected: Vec<_> = (0..SCENE_HEIGHT)
        .flat_map(|y| (0..SCENE_WIDTH).map(move |x| scene_pixel(x, y, phase)))
        .collect();
    assert_pixels(actual, &expected, SCENE_WIDTH);
}

fn assert_differs(actual: &[[f32; 4]], wrong: &[[f64; 4]], mistake: &str) {
    assert_eq!(actual.len(), wrong.len());
    assert!(
        actual.iter().zip(wrong).any(|(actual, wrong)| {
            (0..4).any(|c| (f64::from(actual[c]) - wrong[c]).abs() > TOLERANCE)
        }),
        "the numerical fixture must detect {mistake}"
    );
}
