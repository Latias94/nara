#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

#[path = "support/depth_features.rs"]
mod depth_features;
#[path = "support/depth_model.rs"]
mod depth_model;
#[path = "support/depth_readback.rs"]
mod depth_readback;
#[path = "support/depth_trajectory.rs"]
mod depth_trajectory;

use std::sync::Arc;

use depth_model::*;
use nara_render_wgpu::{
    AccessRole, GraphError, LimitKind, WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuRenderHost,
    WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

fn builder() -> WgpuRenderProgramBuilder {
    WgpuRenderProgramBuilder::new(source(), 1, limits()).unwrap()
}

#[test]
fn depth_output_is_valid_but_policy_final_requires_published_color() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    assert!(host.producer_submission_prefix(&producer).is_none());
    let mut foreign = WgpuRenderHost::offscreen();
    let foreign_producer = foreign.create_producer().unwrap();
    assert!(host.producer_submission_prefix(&foreign_producer).is_none());
    let installed = depth_features::Installed::new(&mut host, &producer);
    let workload = depth_features::build(
        source(),
        1,
        Arc::new(Scene::meters(DrawOrder::NearFirst)),
        &installed,
    );
    assert_eq!(workload.ports.depth.descriptor(), &depth_descriptor());
    assert!(matches!(
        WgpuPolicyProgram::new(
            workload.program.clone(),
            &workload.ports.depth,
            WgpuPolicyFrameFacts::default()
        ),
        Err(WgpuRenderProgramError::ResourceKind)
    ));
    assert!(matches!(
        WgpuPolicyProgram::new(
            workload.program.clone(),
            &workload.ports.preserved_depth,
            WgpuPolicyFrameFacts::default()
        ),
        Err(WgpuRenderProgramError::ResourceKind)
    ));
    WgpuPolicyProgram::new(
        workload.program.clone(),
        &workload.ports.color,
        WgpuPolicyFrameFacts::default(),
    )
    .expect("a published actual color attachment is a valid final image");
    WgpuPolicyProgram::new(
        workload.program,
        &workload.ports.resolved,
        WgpuPolicyFrameFacts::default(),
    )
    .expect("a published float-color depth resolve is a valid final image");
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(
        host.device_epoch(),
        0,
        "graph construction acquires no native device"
    );
    assert!(host.retire_producer(&producer).is_none());
}

#[test]
fn policy_final_still_requires_membership_in_the_complete_output_group() {
    let mut builder = builder();
    let published = builder.texture::<Color>(color_descriptor()).unwrap();
    let hidden = builder.texture::<Color>(color_descriptor()).unwrap();
    let depth = builder.texture::<Depth>(depth_descriptor()).unwrap();
    builder
        .initialize(
            1,
            &[
                published.clear(wgpu::Color::BLACK, AccessRole::Attachment),
                hidden.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment),
                depth.clear_depth(1.0, AccessRole::Attachment),
            ],
        )
        .unwrap();
    let program = builder
        .finish(&[depth.output(), published.output()])
        .unwrap();
    assert!(matches!(
        WgpuPolicyProgram::new(program.clone(), &hidden, WgpuPolicyFrameFacts::default()),
        Err(WgpuRenderProgramError::UnpublishedResource)
    ));
    WgpuPolicyProgram::new(program, &published, WgpuPolicyFrameFacts::default()).unwrap();
}

#[test]
fn depth32float_interpretation_and_four_byte_budget_are_public_construction_contracts() {
    for format in [
        wgpu::TextureFormat::Depth16Unorm,
        wgpu::TextureFormat::Depth24Plus,
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureFormat::Depth32FloatStencil8,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        let mut descriptor = depth_descriptor();
        descriptor.native.format = format;
        assert!(matches!(
            builder().texture::<Depth>(descriptor),
            Err(WgpuRenderProgramError::Graph(
                GraphError::UnsupportedTextureFormat
            ))
        ));
    }
    let mut wrong_color = color_descriptor();
    wrong_color.native.format = wgpu::TextureFormat::Depth32Float;
    assert!(matches!(
        builder().texture::<Depth>(wrong_color),
        Err(WgpuRenderProgramError::Graph(
            GraphError::UnsupportedTextureFormat
        ))
    ));
    let mut storage_depth = depth_descriptor();
    storage_depth.native.usage |= wgpu::TextureUsages::STORAGE_BINDING;
    assert!(matches!(
        builder().texture::<Depth>(storage_depth),
        Err(WgpuRenderProgramError::Graph(GraphError::UnsupportedUsage))
    ));
    let mut bounded = limits();
    bounded.max_modeled_resource_bytes = DEPTH_BYTES - 1;
    assert!(matches!(
        WgpuRenderProgramBuilder::new(source(), 1, bounded)
            .unwrap()
            .texture::<Depth>(depth_descriptor()),
        Err(WgpuRenderProgramError::Graph(GraphError::LimitExceeded(
            LimitKind::ResourceBytes
        )))
    ));
    bounded.max_modeled_resource_bytes = DEPTH_BYTES;
    let mut exact = WgpuRenderProgramBuilder::new(source(), 1, bounded).unwrap();
    let depth = exact.texture::<Depth>(depth_descriptor()).unwrap();
    exact
        .initialize(1, &[depth.clear_depth(1.0, AccessRole::Attachment)])
        .unwrap();
    exact
        .finish(&[depth.output()])
        .expect("Depth32Float charges exactly four modeled bytes per texel");
}

#[test]
fn invalid_depth_clears_refuse_without_consuming_the_original_candidate() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
        let mut builder = builder();
        let depth = builder.texture::<Depth>(depth_descriptor()).unwrap();
        assert!(matches!(
            builder.initialize(7, &[depth.clear_depth(value, AccessRole::Attachment)]),
            Err(WgpuRenderProgramError::Graph(
                GraphError::InvalidInitialization(_)
            ))
        ));
        assert!(matches!(
            builder.initialize(
                7,
                &[depth.clear(wgpu::Color::WHITE, AccessRole::Attachment)]
            ),
            Err(WgpuRenderProgramError::Graph(
                GraphError::InvalidInitialization(_)
            ))
        ));
        builder
            .initialize(7, &[depth.clear_depth(1.0, AccessRole::Attachment)])
            .unwrap();
        builder
            .finish(&[depth.output()])
            .expect("refused clears preserve the original writer slot and operation key");
    }
    let mut builder = builder();
    let color = builder.texture::<Color>(color_descriptor()).unwrap();
    assert!(matches!(
        builder.initialize(1, &[color.clear_depth(1.0, AccessRole::Attachment)]),
        Err(WgpuRenderProgramError::Graph(
            GraphError::InvalidInitialization(_)
        ))
    ));
    builder
        .initialize(
            1,
            &[color.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    builder.finish(&[color.output()]).unwrap();
}

#[test]
fn depth_preserve_accepts_compatible_storage_and_refuses_color_or_different_extents() {
    let mut builder = builder();
    let original = builder.texture::<Depth>(depth_descriptor()).unwrap();
    let preserved = builder.texture::<Depth>(depth_descriptor()).unwrap();
    let color = builder.texture::<Color>(color_descriptor()).unwrap();
    let mut smaller = depth_descriptor();
    smaller.native.size.width /= 2;
    let smaller = builder.texture::<Depth>(smaller).unwrap();
    assert!(matches!(
        builder.initialize(
            10,
            &[preserved.preserve(&color, AccessRole::CopyDestination)]
        ),
        Err(WgpuRenderProgramError::Graph(
            GraphError::IncompatiblePreserve(_)
        ))
    ));
    assert!(matches!(
        builder.initialize(
            10,
            &[preserved.preserve(&smaller, AccessRole::CopyDestination)]
        ),
        Err(WgpuRenderProgramError::Graph(
            GraphError::IncompatiblePreserve(_)
        ))
    ));
    builder
        .initialize(
            10,
            &[preserved.preserve(&original, AccessRole::CopyDestination)],
        )
        .unwrap();
    builder
        .initialize(
            90,
            &[
                original.clear_depth(1.0, AccessRole::Attachment),
                smaller.clear_depth(0.0, AccessRole::Attachment),
                color.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment),
            ],
        )
        .unwrap();
    builder
        .finish(&[original.output(), preserved.output()])
        .unwrap();
}

#[test]
fn independent_ray_plane_oracle_covers_all_pixels_and_distinguishes_camera_units_and_occlusion() {
    let meters = Scene::meters(DrawOrder::NearFirst);
    let expected = reference(&meters);
    assert_eq!(expected.color.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(expected.depth.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(
        expected
            .color
            .iter()
            .filter(|pixel| **pixel == [1.0, 0.0, 0.0, 1.0])
            .count(),
        216
    );
    assert_eq!(
        expected
            .color
            .iter()
            .filter(|pixel| **pixel == [0.0, 0.0, 1.0, 1.0])
            .count(),
        360
    );
    assert_eq!(
        expected
            .color
            .iter()
            .filter(|pixel| **pixel == [0.0; 4])
            .count(),
        2496
    );
    assert!((expected.depth[(24 * WIDTH + 32) as usize] - 0.5625).abs() <= TOLERANCE);
    assert!((expected.depth[(13 * WIDTH + 21) as usize] - 0.9375).abs() <= TOLERANCE);
    assert_eq!(expected.depth[0], 1.0);
    let preserved = preserved_reference(&meters);
    assert_eq!(
        preserved.color[(24 * WIDTH + 32) as usize],
        [0.0, 1.0, 0.0, 1.0]
    );
    assert!((preserved.depth[(24 * WIDTH + 32) as usize] - 0.375).abs() <= TOLERANCE);
    assert_eq!(
        expected
            .color
            .iter()
            .zip(&preserved.color)
            .filter(|(a, b)| a != b)
            .count(),
        (PRESERVE_SCISSOR[2] * PRESERVE_SCISSOR[3]) as usize,
    );
    assert_eq!(
        expected
            .depth
            .iter()
            .zip(&preserved.depth)
            .filter(|(a, b)| a != b)
            .count(),
        (PRESERVE_SCISSOR[2] * PRESERVE_SCISSOR[3]) as usize,
    );
    for scene in [
        Scene::meters(DrawOrder::FarFirst),
        Scene::centimeters(DrawOrder::NearFirst),
        Scene::centimeters(DrawOrder::FarFirst),
    ] {
        assert_eq!(reference(&scene), expected);
        assert_eq!(preserved_reference(&scene), preserved);
    }
    let mut shifted = meters.clone();
    shifted.camera.eye_m[0] = 0.25;
    let shifted = reference(&shifted);
    assert_ne!(shifted.color, expected.color);
    assert_ne!(shifted.depth, expected.depth);
    let mut wrong_unit = Scene::centimeters(DrawOrder::NearFirst);
    wrong_unit.meters_per_unit = 1.0;
    assert_ne!(
        reference(&wrong_unit),
        expected,
        "missing centimeter conversion must fail the oracle"
    );
}

#[test]
#[ignore = "requires explicit WGPU_BACKEND=dx12 or vulkan and NARA_WGPU_FORCE_FALLBACK=0 or 1; missing requested adapter fails"]
fn independent_depth_units_preserve_capture_and_retired_outputs_on_original_host() {
    depth_trajectory::run();
}
