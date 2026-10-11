struct VsIn {
    @location(0) corner: vec2<f32>,
    @location(1) center_size: vec4<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn litd_vfx_particle_vs(input: VsIn) -> VsOut {
    var out: VsOut;
    let position = input.center_size.xy + input.corner * input.center_size.zw;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.local = input.corner;
    out.color = input.color;
    return out;
}

@fragment
fn litd_vfx_particle_fs(input: VsOut) -> @location(0) vec4<f32> {
    let distance = length(input.local);
    let falloff = 1.0 - smoothstep(0.18, 1.0, distance);
    let alpha = clamp(falloff * input.color.a, 0.0, 1.0);
    return vec4<f32>(input.color.rgb * alpha, alpha);
}
