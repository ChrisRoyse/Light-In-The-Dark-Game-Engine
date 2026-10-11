struct LitdStylizedCamera {
    view_proj: mat4x4<f32>,
};

struct LitdStylizedParams {
    primary_color: vec4<f32>,
    secondary_color: vec4<f32>,
    params0: vec4<f32>,
};

@group(0) @binding(0) var<uniform> litd_stylized_camera: LitdStylizedCamera;
@group(1) @binding(0) var litd_stylized_mask_tex: texture_2d<f32>;
@group(1) @binding(1) var litd_stylized_mask_sampler: sampler;
@group(2) @binding(0) var<uniform> litd_stylized_params: LitdStylizedParams;

struct LitdStylizedVertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct LitdStylizedVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn litd_stylized_vs(input: LitdStylizedVertexIn) -> LitdStylizedVertexOut {
    var out: LitdStylizedVertexOut;
    out.position = litd_stylized_camera.view_proj * vec4<f32>(input.position, 1.0);
    out.uv = input.uv;
    out.normal = normalize(input.normal);
    return out;
}

fn litd_mask(uv: vec2<f32>) -> f32 {
    return textureSample(litd_stylized_mask_tex, litd_stylized_mask_sampler, uv).r;
}

@fragment
fn litd_stylized_water_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let wave = sin((input.uv.x + input.uv.y) * 24.0 + litd_stylized_params.params0.y);
    let foam = smoothstep(0.55, 1.0, litd_mask(input.uv) + wave * 0.15);
    return mix(litd_stylized_params.primary_color, litd_stylized_params.secondary_color, foam);
}

@fragment
fn litd_stylized_fire_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let lick = litd_mask(input.uv + vec2<f32>(0.0, litd_stylized_params.params0.y * 0.05));
    return mix(litd_stylized_params.secondary_color, litd_stylized_params.primary_color, lick)
        * litd_stylized_params.params0.x;
}

@fragment
fn litd_stylized_glow_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let pulse = 0.75 + 0.25 * sin(litd_stylized_params.params0.z);
    return litd_stylized_params.primary_color * litd_stylized_params.params0.x * pulse;
}

@fragment
fn litd_stylized_toon_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let light = max(dot(normalize(input.normal), normalize(vec3<f32>(0.3, 0.6, 0.8))), 0.0);
    let bands = max(litd_stylized_params.params0.x, 2.0);
    let stepped = floor(light * bands) / max(bands - 1.0, 1.0);
    return mix(litd_stylized_params.secondary_color, litd_stylized_params.primary_color, stepped);
}

@fragment
fn litd_stylized_dissolve_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let noise = litd_mask(input.uv);
    let edge = smoothstep(litd_stylized_params.params0.x, litd_stylized_params.params0.x + litd_stylized_params.params0.y, noise);
    return mix(litd_stylized_params.secondary_color, litd_stylized_params.primary_color, edge);
}

@fragment
fn litd_stylized_force_field_fs(input: LitdStylizedVertexOut) -> @location(0) vec4<f32> {
    let grid = abs(fract(input.uv.x * 10.0) - 0.5) + abs(fract(input.uv.y * 10.0) - 0.5);
    let line = 1.0 - smoothstep(0.08, 0.18, grid);
    return vec4<f32>(litd_stylized_params.primary_color.rgb, litd_stylized_params.params0.x + line * 0.35);
}
