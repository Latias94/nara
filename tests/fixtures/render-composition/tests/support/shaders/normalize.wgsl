// This is the fresh COPY-from-raw successor, never the retained original counts buffer.
@group(0) @binding(0) var<storage, read_write> normalized: array<u32, 4>;

@compute @workgroup_size(1, 1, 1)
fn normalize_counts() {
    // One invocation reads the complete integer total before replacing any word with float bits.
    let total = normalized[0] + normalized[1] + normalized[2] + normalized[3];
    let denominator = f32(max(total, 1u));
    for (var bin = 0u; bin < 4u; bin = bin + 1u) {
        normalized[bin] = bitcast<u32>(f32(normalized[bin]) / denominator);
    }
}
