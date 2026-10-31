// A finite drawing tracer: one Scene image, one control, an alpha-zero control, and a popup.
// No filtering, hardware blending, toolkit, live layout lookup, or interaction authority.
struct Placement {
    mapping: vec4<f32>,
    scene: vec4<f32>,
    scene_clip: vec4<f32>,
    background: vec4<f32>,
    control: vec4<f32>,
    control_clip: vec4<f32>,
    control_color: vec4<f32>,
    transparent: vec4<f32>,
    transparent_clip: vec4<f32>,
    popup: vec4<f32>,
    popup_clip: vec4<f32>,
    popup_color: vec4<f32>,
};
@group(0) @binding(0) var scene_image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> placement: Placement;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn inside(point: vec2<f32>, rectangle: vec4<f32>) -> bool {
    return all(point >= rectangle.xy) && all(point < rectangle.xy + rectangle.zw);
}

fn over(front: vec4<f32>, back: vec4<f32>) -> vec4<f32> {
    return front + back * (1.0 - front.a);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = (position.xy - placement.mapping.yz) / placement.mapping.x;
    let extent = vec2<f32>(textureDimensions(scene_image));
    let scale = min(placement.scene.z / extent.x, placement.scene.w / extent.y);
    let size = extent * scale;
    let image = vec4<f32>(placement.scene.xy + (placement.scene.zw - size) * 0.5, size);
    var color = placement.background;
    if inside(point, image) && inside(point, placement.scene_clip) {
        let texel = vec2<i32>(floor((point - image.xy) / scale));
        color = over(textureLoad(scene_image, texel, 0), color);
    }
    if inside(point, placement.control) && inside(point, placement.control_clip) {
        color = over(placement.control_color, color);
    }
    if inside(point, placement.transparent) && inside(point, placement.transparent_clip) {
        color = over(vec4<f32>(0.0), color);
    }
    if inside(point, placement.popup) && inside(point, placement.popup_clip) {
        color = over(placement.popup_color, color);
    }
    return color;
}
