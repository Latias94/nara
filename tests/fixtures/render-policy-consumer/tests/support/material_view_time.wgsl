fn time_byte(seconds: f32) -> u32 {
    return 16u + 3u * (u32(floor(seconds * 256.0 + 0.125)) % 64u);
}

fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    if nara_view.clock_available != 1u
        || any(nara_view.real_period != vec2<u32>(0u))
        || any(nara_view.virtual_period != vec2<u32>(0u))
        || any(nara_view.fixed_period != vec2<u32>(0u))
        || nara_view.frame.y != 0u
        || nara_view.virtual_frame.y != 0u
        || nara_view.fixed_tick.y != 0u {
        return vec4<f32>(1.0, 0.0, 1.0, 1.0);
    }

    let unprojected = nara_view.clip_to_world * vec3<f32>(0.5, -0.25, 1.0);
    let projected = nara_view.world_to_clip * vec3<f32>(0.75, 2.375, 1.0);
    let view_byte = select(
        32u + u32(round(unprojected.x * 64.0 + unprojected.y * 32.0)),
        64u + u32(round(projected.x * 32.0 + projected.y * 24.0)),
        uv.x >= 0.5,
    );
    let viewport_byte = u32(round(
        (nara_view.viewport.z + nara_view.viewport.w) * 0.5
            + nara_view.viewport.x * 0.5 + nara_view.viewport.y,
    ));

    var bytes: vec3<u32>;
    switch u32(floor(uv.y * 4.0)) {
        case 0u: {
            bytes = vec3<u32>(
                time_byte(nara_view.real_elapsed),
                time_byte(nara_view.virtual_elapsed),
                view_byte,
            );
        }
        case 1u: {
            bytes = vec3<u32>(
                time_byte(nara_view.real_delta),
                time_byte(nara_view.virtual_delta),
                16u + 7u * (nara_view.fixed_tick.x % 32u) + viewport_byte,
            );
        }
        case 2u: {
            bytes = vec3<u32>(
                time_byte(nara_view.fixed_elapsed),
                time_byte(nara_view.fixed_delta),
                time_byte(nara_view.fixed_timestep),
            );
        }
        default: {
            bytes = vec3<u32>(
                16u + 11u * (nara_view.frame.x % 16u),
                16u + 11u * (nara_view.virtual_frame.x % 16u),
                16u + 64u * nara_view.spatial_interpolated + 32u * nara_view.paused
                    + u32(round(nara_view.time_scale * 16.0))
                    + u32(round(nara_view.interpolation_alpha * 8.0))
                    + 4u * nara_view.fixed_steps,
            );
        }
    }
    return vec4<f32>(vec3<f32>(bytes) * (material_parameters.gain / 255.0), 1.0);
}
