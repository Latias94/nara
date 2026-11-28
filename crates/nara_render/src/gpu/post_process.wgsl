struct Settings {
    physical_rect: vec4f,
    fade: vec4f,
    bloom_threshold: f32,
    bloom_intensity: f32,
    bloom_radius: f32,
    vignette_intensity: f32,
    vignette_smoothness: f32,
    rgb_limit: f32,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: Settings;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var bloom: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let positions = array(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    return vec4f(positions[index], 0.0, 1.0);
}

fn highlight(pixel: vec2i, last: vec2i) -> vec3f {
    let rgb = max(textureLoad(source, min(pixel, last), 0).rgb, vec3f(0.0));
    let peak = max(rgb.r, max(rgb.g, rgb.b));
    return rgb * (max(peak - settings.bloom_threshold, 0.0) / max(peak, 0.00001));
}

@fragment
fn fs_extract(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let pixel = vec2i(position.xy) * 2;
    let last = vec2i(textureDimensions(source)) - vec2i(1);
    // Threshold each source pixel before averaging so single-pixel highlights contribute.
    let rgb = (highlight(pixel, last)
        + highlight(pixel + vec2i(1, 0), last)
        + highlight(pixel + vec2i(0, 1), last)
        + highlight(pixel + vec2i(1, 1), last)) * 0.25;
    return vec4f(rgb, 0.0);
}

fn gaussian(position: vec2f, axis: vec2f) -> vec4f {
    let size = vec2f(textureDimensions(source));
    let uv = position / size;
    // The outer taps sit one radius away in full-resolution world raster pixels.
    let step = axis * (settings.bloom_radius * 0.25) / size;
    let center = textureSampleLevel(source, linear_sampler, uv, 0.0).rgb;
    let inner = textureSampleLevel(source, linear_sampler, uv - step, 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv + step, 0.0).rgb;
    let outer = textureSampleLevel(source, linear_sampler, uv - 2.0 * step, 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv + 2.0 * step, 0.0).rgb;
    return vec4f(center * 0.40262 + inner * 0.24420 + outer * 0.05449, 0.0);
}

@fragment
fn fs_horizontal(@builtin(position) position: vec4f) -> @location(0) vec4f {
    return gaussian(position.xy, vec2f(1.0, 0.0));
}

@fragment
fn fs_vertical(@builtin(position) position: vec4f) -> @location(0) vec4f {
    return gaussian(position.xy, vec2f(0.0, 1.0));
}

fn world_pixel(position: vec2f) -> vec2i {
    let size = textureDimensions(source);
    let rect = settings.physical_rect;
    let uv = (position - rect.xy) / (rect.zw - rect.xy);
    return clamp(vec2i(floor(uv * vec2f(size))), vec2i(0), vec2i(size) - vec2i(1));
}

fn composite(pixel: vec2i, glow: vec3f) -> vec4f {
    let scene = textureLoad(source, pixel, 0);
    let alpha = clamp(scene.a, 0.0, 1.0);
    // Glow adds light to covered pixels. Retain scene coverage for downstream composition.
    var rgb = scene.rgb + glow * (settings.bloom_intensity * alpha);
    if settings.vignette_intensity > 0.0 {
        let uv = (vec2f(pixel) + vec2f(0.5)) / vec2f(textureDimensions(source));
        let distance = length(uv * 2.0 - vec2f(1.0));
        let width = max(settings.vignette_smoothness, 0.0001);
        rgb *= 1.0 - smoothstep(1.0 - width, 1.0, distance) * settings.vignette_intensity;
    }
    let world = vec4f(clamp(rgb, vec3f(0.0), vec3f(settings.rgb_limit * alpha)), alpha);
    // Both operands are linear and premultiplied. An sRGB target encodes the result on store.
    return settings.fade + world * (1.0 - settings.fade.a);
}

@fragment
fn fs_composite(@builtin(position) position: vec4f) -> @location(0) vec4f {
    return composite(world_pixel(position.xy), vec3f(0.0));
}

@fragment
fn fs_composite_bloom(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let pixel = world_pixel(position.xy);
    // Each bloom texel covers two world pixels, including the clamped edge at odd extents.
    let uv = (vec2f(pixel) + vec2f(0.5)) / (2.0 * vec2f(textureDimensions(bloom)));
    let glow = textureSampleLevel(bloom, linear_sampler, uv, 0.0).rgb;
    return composite(pixel, glow);
}
