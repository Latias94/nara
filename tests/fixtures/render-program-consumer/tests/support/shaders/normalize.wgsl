@group(0) @binding(0) var<storage, read_write> frequencies: array<u32, 4>;

@compute @workgroup_size(1)
fn normalize() {
    // The Host copied the exact raw predecessor into this separate semantic output.
    let total = frequencies[0] + frequencies[1] + frequencies[2] + frequencies[3];
    let denominator = f32(max(total, 1u));
    for (var bin = 0u; bin < 4u; bin = bin + 1u) {
        frequencies[bin] = bitcast<u32>(f32(frequencies[bin]) / denominator);
    }
}
