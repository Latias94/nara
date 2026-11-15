struct Camera {
    projection: mat4x4<f32>,
    eye_m: vec4<f32>,
    units: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vertex(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
    let eye_relative_m = position * camera.units.x - camera.eye_m.xyz;
    var output: VertexOutput;
    output.position = camera.projection * vec4<f32>(eye_relative_m, 1.0);
    output.color = color;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
