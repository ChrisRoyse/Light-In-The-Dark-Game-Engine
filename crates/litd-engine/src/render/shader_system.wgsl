struct LitdShaderCamera {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> litd_camera: LitdShaderCamera;
@group(1) @binding(0) var litd_albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var litd_albedo_sampler: sampler;

struct LitdShaderVertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct LitdShaderVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn litd_shader_system_vs(input: LitdShaderVertexIn) -> LitdShaderVertexOut {
    var out: LitdShaderVertexOut;
    out.position = litd_camera.view_proj * vec4<f32>(input.position, 1.0);
    out.uv = input.uv;
    return out;
}

@fragment
fn litd_shader_system_fs(input: LitdShaderVertexOut) -> @location(0) vec4<f32> {
    return textureSample(litd_albedo_tex, litd_albedo_sampler, input.uv);
}
