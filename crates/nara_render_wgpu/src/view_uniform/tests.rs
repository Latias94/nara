use super::*;
use nara_core::{Color, Mat3, Vec2};
use nara_render::{RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect};
use std::mem::{align_of, offset_of, size_of};

fn view() -> ExtractedView {
    let viewport = ViewportRect::new(7, 13, 120, 60).unwrap();
    ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: nara_ecs::World::new().spawn_empty().id(),
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(
            Mat3::from_scale_angle_translation(Vec2::new(2.0, 3.0), 0.7, Vec2::new(12.0, -9.0)),
            30.0,
            viewport,
        )
        .unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    }
}

#[test]
fn packed_view_matches_every_naga_member_offset_and_exact_span() {
    let source = format!(
        "{}\nfn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {{ return sampled * tint; }}",
        nara_material::__STOCK_QUAD_SHADER_SOURCE,
    );
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let variable = module
        .global_variables
        .iter()
        .map(|(_, variable)| variable)
        .find(|variable| variable.name.as_deref() == Some("nara_view"))
        .unwrap();
    assert_eq!(variable.space, wgpu::naga::AddressSpace::Uniform);
    assert_eq!(
        variable.binding,
        Some(wgpu::naga::ResourceBinding {
            group: 2,
            binding: 0
        }),
    );
    let wgpu::naga::TypeInner::Struct { members, span } = &module.types[variable.ty].inner else {
        panic!("the engine View uniform must be a WGSL structure");
    };
    let offsets = [
        ("world_to_clip", offset_of!(WgpuViewUniform, world_to_clip)),
        ("clip_to_world", offset_of!(WgpuViewUniform, clip_to_world)),
        ("viewport", offset_of!(WgpuViewUniform, viewport)),
        ("real_elapsed", offset_of!(WgpuViewUniform, real_elapsed)),
        ("real_delta", offset_of!(WgpuViewUniform, real_delta)),
        (
            "virtual_elapsed",
            offset_of!(WgpuViewUniform, virtual_elapsed),
        ),
        ("virtual_delta", offset_of!(WgpuViewUniform, virtual_delta)),
        ("fixed_elapsed", offset_of!(WgpuViewUniform, fixed_elapsed)),
        ("fixed_delta", offset_of!(WgpuViewUniform, fixed_delta)),
        (
            "fixed_timestep",
            offset_of!(WgpuViewUniform, fixed_timestep),
        ),
        (
            "interpolation_alpha",
            offset_of!(WgpuViewUniform, interpolation_alpha),
        ),
        ("time_scale", offset_of!(WgpuViewUniform, time_scale)),
        (
            "clock_available",
            offset_of!(WgpuViewUniform, clock_available),
        ),
        ("paused", offset_of!(WgpuViewUniform, paused)),
        (
            "spatial_interpolated",
            offset_of!(WgpuViewUniform, spatial_interpolated),
        ),
        ("frame", offset_of!(WgpuViewUniform, frame)),
        ("fixed_tick", offset_of!(WgpuViewUniform, fixed_tick)),
        ("real_period", offset_of!(WgpuViewUniform, real_period)),
        (
            "virtual_period",
            offset_of!(WgpuViewUniform, virtual_period),
        ),
        ("fixed_period", offset_of!(WgpuViewUniform, fixed_period)),
        ("_reserved0", offset_of!(WgpuViewUniform, _reserved0)),
        ("virtual_frame", offset_of!(WgpuViewUniform, virtual_frame)),
        ("fixed_steps", offset_of!(WgpuViewUniform, fixed_steps)),
        ("_reserved1", offset_of!(WgpuViewUniform, _reserved1)),
    ];
    assert_eq!(members.len(), offsets.len());
    assert_eq!(u64::from(*span), VIEW_UNIFORM_BYTES);
    assert_eq!(size_of::<WgpuViewUniform>() as u64, VIEW_UNIFORM_BYTES);
    assert_eq!(align_of::<WgpuViewUniform>(), 16);
    for (member, (name, offset)) in members.iter().zip(offsets) {
        assert_eq!(member.name.as_deref(), Some(name));
        assert_eq!(member.offset as usize, offset, "{name}");
    }
}

#[test]
fn manual_view_keeps_original_affine_mapping_and_advertises_no_clock() {
    let view = view();
    let uniform = WgpuViewUniform::capture(&view, None);
    assert_eq!(uniform.viewport, [7.0, 13.0, 120.0, 60.0]);
    for (packed, matrix) in [
        (uniform.world_to_clip, view.mapping.world_to_clip_matrix()),
        (uniform.clip_to_world, view.mapping.clip_to_world_matrix()),
    ] {
        for (column, original) in packed.iter().zip(matrix.to_cols_array_2d()) {
            assert_eq!(&column[..3], &original);
            assert_eq!(column[3].to_bits(), 0);
        }
    }
    assert_eq!(uniform.clock_available, 0);
    assert!(
        bytemuck::bytes_of(&uniform)[112..]
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn elapsed_wraps_with_integer_periods_and_never_rounds_to_the_next_period() {
    assert_eq!(period_time(Duration::ZERO), (0.0, [0, 0]));
    assert_eq!(period_time(Duration::from_secs(3_600)), (0.0, [1, 0]));
    assert_eq!(
        period_time(Duration::from_millis(3_600_125)),
        (0.125, [1, 0])
    );
    let period = (1_u64 << 32) + 0x1234_5678;
    assert_eq!(
        period_time(Duration::new(
            period * TIME_PERIOD_SECONDS + 17,
            500_000_000
        )),
        (17.5, [0x1234_5678, 1]),
    );
    let late = Duration::new(3_599, 999_999_999);
    assert_eq!(late.as_secs_f32(), 3_600.0);
    assert_eq!(period_time(late).0.to_bits(), 3_600.0_f32.to_bits() - 1);
    assert_eq!(period_time(late).1, [0, 0]);
    let maximum = period_time(Duration::MAX);
    assert!(maximum.0.is_finite() && (0.0..3_600.0).contains(&maximum.0));
    assert_eq!(
        u64::from(maximum.1[0]) | (u64::from(maximum.1[1]) << 32),
        u64::MAX / TIME_PERIOD_SECONDS,
    );
    for counter in [0, u64::from(u32::MAX), 1_u64 << 32, u64::MAX] {
        let words = counter_words(counter);
        assert_eq!(u64::from(words[0]) | (u64::from(words[1]) << 32), counter);
    }
}

#[test]
fn clocks_preserve_maximum_deltas_scale_pause_and_original_render_alpha() {
    let uniform = WgpuViewUniform::capture(&view(), None).with_clocks(
        RealTime {
            elapsed: Duration::MAX,
            delta: Duration::MAX,
            frame: u64::MAX,
        },
        VirtualTime {
            elapsed: Duration::from_millis(7_200_750),
            delta: Duration::MAX,
            frame: 0x1234_5678_9abc_def0,
        },
        FixedTime::default(),
        RenderTime {
            interpolation_alpha: 0.625,
            remainder: Duration::from_millis(4),
        },
        RuntimeTimeSettings::default()
            .with_time_scale(0.25)
            .unwrap()
            .with_paused(true),
        false,
    );
    assert_eq!(uniform.clock_available, 1);
    assert_eq!(uniform.real_delta, Duration::MAX.as_secs_f32());
    assert!(uniform.real_delta.is_finite());
    assert_eq!(uniform.virtual_delta, Duration::MAX.as_secs_f32());
    assert!(uniform.virtual_delta.is_finite());
    assert_eq!(uniform.real_period, period_time(Duration::MAX).1);
    assert_eq!(uniform.virtual_elapsed, 0.75);
    assert_eq!(uniform.virtual_period, [2, 0]);
    assert_eq!(uniform.frame, [u32::MAX, u32::MAX]);
    assert_eq!(uniform.virtual_frame, [0x9abc_def0, 0x1234_5678]);
    assert_eq!(uniform.time_scale, 0.25);
    assert_eq!(uniform.paused, 1);
    assert_eq!(uniform.interpolation_alpha, 0.625);
    assert_eq!(uniform.spatial_interpolated, 0);
    assert_eq!(uniform.fixed_delta, 0.0);
    assert_eq!(uniform.fixed_steps, 0);
    assert_eq!(uniform._reserved0, [0, 0]);
    assert_eq!(uniform._reserved1, 0);
}

#[test]
fn fixed_delta_is_the_latest_tick_and_steps_remain_a_separate_count() {
    let mut app = nara_app::App::new();
    app.insert_resource(FixedTime::new(Duration::from_millis(10)).unwrap())
        .unwrap();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    for (delta, steps, tick) in [
        (Duration::from_millis(35), 3, 3),
        (Duration::from_millis(2), 0, 3),
        (Duration::from_millis(3), 1, 4),
    ] {
        runtime.drive(delta).unwrap();
        let world = runtime.world();
        let uniform = WgpuViewUniform::capture(&view(), None).with_clocks(
            *world.resource::<RealTime>(),
            *world.resource::<VirtualTime>(),
            *world.resource::<FixedTime>(),
            *world.resource::<RenderTime>(),
            *world.resource::<RuntimeTimeSettings>(),
            true,
        );
        assert_eq!(uniform.fixed_steps, steps);
        assert_eq!(uniform.fixed_tick, [tick, 0]);
        assert_eq!(uniform.fixed_timestep, 0.01);
        assert_eq!(uniform.fixed_delta, if steps == 0 { 0.0 } else { 0.01 });
        assert_eq!(uniform.spatial_interpolated, 1);
    }
    runtime.finish().unwrap();
}
