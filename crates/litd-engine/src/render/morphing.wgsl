struct LitdMorphWeights {
    weights: array<f32>,
};

struct LitdMorphPositionDeltas {
    deltas: array<vec4<f32>>,
};

@group(4) @binding(0) var<storage, read> litd_morph_weights: LitdMorphWeights;
@group(4) @binding(1) var<storage, read> litd_morph_position_deltas: LitdMorphPositionDeltas;

struct LitdMorphingVsIn {
    @location(0) position: vec3<f32>,
    @location(12) vertex_index: u32,
    @location(13) target_count: u32,
    @location(14) vertex_count: u32,
};

struct LitdMorphingVsOut {
    @builtin(position) clip_position: vec4<f32>,
};

fn litd_morph_position(
    base_position: vec3<f32>,
    vertex_index: u32,
    target_count: u32,
    vertex_count: u32,
) -> vec3<f32> {
    var result = base_position;
    var target = 0u;
    loop {
        if (target >= target_count) {
            break;
        }
        let delta_index = target * vertex_count + vertex_index;
        let delta = litd_morph_position_deltas.deltas[delta_index].xyz;
        result += delta * litd_morph_weights.weights[target];
        target += 1u;
    }
    return result;
}

@vertex
fn litd_morphing_vs(input: LitdMorphingVsIn) -> LitdMorphingVsOut {
    var output: LitdMorphingVsOut;
    let morphed = litd_morph_position(input.position, input.vertex_index, input.target_count, input.vertex_count);
    output.clip_position = vec4<f32>(morphed, 1.0);
    return output;
}
