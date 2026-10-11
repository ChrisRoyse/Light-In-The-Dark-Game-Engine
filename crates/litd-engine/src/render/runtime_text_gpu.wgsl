struct TextVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0)
var font_atlas: texture_2d<f32>;

@group(0) @binding(1)
var font_sampler: sampler;

@vertex
fn runtime_text_vs(
    @location(0) position_ndc: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
) -> TextVertexOut {
    var out: TextVertexOut;
    out.position = vec4<f32>(position_ndc, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    return out;
}

@fragment
fn runtime_text_fs(in: TextVertexOut) -> @location(0) vec4<f32> {
    let distance_field = textureSample(font_atlas, font_sampler, in.uv).r;
    let width = max(fwidth(distance_field), 0.015);
    let coverage = smoothstep(0.5 - width, 0.5 + width, distance_field);
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
