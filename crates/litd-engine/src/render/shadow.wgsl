struct ShadowSample {
    uv_depth: vec3<f32>,
    tile_min_size: vec4<f32>,
    texel_size_bias: vec4<f32>,
};

struct ShadowDepthParams {
    depth_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> litd_shadow_depth_params: ShadowDepthParams;
@group(1) @binding(0) var litd_shadow_atlas: texture_depth_2d;
@group(1) @binding(1) var litd_shadow_sampler: sampler_comparison;

@vertex
fn litd_shadow_depth_vs(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(-0.62, -0.62),
        vec2<f32>(0.62, -0.62),
        vec2<f32>(0.62, 0.62),
        vec2<f32>(-0.62, -0.62),
        vec2<f32>(0.62, 0.62),
        vec2<f32>(-0.62, 0.62),
    );
    return vec4<f32>(positions[vertex_index], litd_shadow_depth_params.depth_pad.x, 1.0);
}

fn litd_shadow_tile_uv(sample: ShadowSample, offset: vec2<i32>) -> vec3<f32> {
    let texel_offset = vec2<f32>(f32(offset.x), f32(offset.y)) * sample.texel_size_bias.xy;
    let atlas_uv = sample.tile_min_size.xy + (sample.uv_depth.xy + texel_offset) * sample.tile_min_size.zw;
    return vec3<f32>(atlas_uv, sample.uv_depth.z - sample.texel_size_bias.z);
}

fn litd_shadow_pcf(sample: ShadowSample, radius: i32) -> f32 {
    var visibility = 0.0;
    var taps = 0.0;
    for (var y = -radius; y <= radius; y = y + 1) {
        for (var x = -radius; x <= radius; x = x + 1) {
            let atlas = litd_shadow_tile_uv(sample, vec2<i32>(x, y));
            visibility = visibility + textureSampleCompare(
                litd_shadow_atlas,
                litd_shadow_sampler,
                atlas.xy,
                atlas.z,
            );
            taps = taps + 1.0;
        }
    }
    return visibility / max(taps, 1.0);
}

fn litd_shadow_apply_normal_bias(world_position: vec3<f32>, normal: vec3<f32>, normal_bias: f32) -> vec3<f32> {
    return world_position + normalize(normal) * normal_bias;
}
