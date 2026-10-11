struct PostUniform {
    texel_size: vec2<f32>,
    bloom_intensity: f32,
    exposure: f32,
    display: vec4<f32>,
    grid: vec4<f32>,
};

struct FullscreenOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var blur_source: texture_2d<f32>;
@group(0) @binding(1)
var post_sampler: sampler;
@group(0) @binding(2)
var<uniform> post: PostUniform;

@group(1) @binding(0)
var scene_texture: texture_2d<f32>;
@group(1) @binding(1)
var bloom_texture: texture_2d<f32>;
@group(1) @binding(2)
var composite_sampler: sampler;
@group(1) @binding(3)
var<uniform> composite: PostUniform;

@vertex
fn litd_editor_world_fullscreen_vs(@builtin(vertex_index) vertex_index: u32) -> FullscreenOut {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: FullscreenOut;
    out.clip_position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

fn litd_editor_world_blur_sample(uv: vec2<f32>, step: vec2<f32>) -> vec3<f32> {
    var color = textureSample(blur_source, post_sampler, uv).rgb * 0.227027;
    color += textureSample(blur_source, post_sampler, uv + step * 1.384615).rgb * 0.316216;
    color += textureSample(blur_source, post_sampler, uv - step * 1.384615).rgb * 0.316216;
    color += textureSample(blur_source, post_sampler, uv + step * 3.230769).rgb * 0.070270;
    color += textureSample(blur_source, post_sampler, uv - step * 3.230769).rgb * 0.070270;
    return color;
}

@fragment
fn litd_editor_world_blur_horizontal_fs(input: FullscreenOut) -> @location(0) vec4<f32> {
    let color = litd_editor_world_blur_sample(input.uv, vec2<f32>(post.texel_size.x, 0.0));
    return vec4<f32>(color, 1.0);
}

@fragment
fn litd_editor_world_blur_vertical_fs(input: FullscreenOut) -> @location(0) vec4<f32> {
    let color = litd_editor_world_blur_sample(input.uv, vec2<f32>(0.0, post.texel_size.y));
    return vec4<f32>(color, 1.0);
}

fn litd_editor_world_post_tonemap_aces(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp(
        (color * (a * color + vec3<f32>(b))) /
            (color * (c * color + vec3<f32>(d)) + vec3<f32>(e)),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

fn litd_editor_world_color_grade(color: vec3<f32>, shadow_strength: f32) -> vec3<f32> {
    let pivot = vec3<f32>(0.24);
    let contrasted = (color - pivot) * 1.16 + pivot;
    let lifted = max(contrasted + vec3<f32>(0.000, 0.004, 0.009), vec3<f32>(0.0));
    let luma = dot(lifted, vec3<f32>(0.2126, 0.7152, 0.0722));
    let shadow_cool = (1.0 - smoothstep(0.18, 0.55, luma)) *
        vec3<f32>(-0.004, 0.002, 0.010) * shadow_strength;
    let warm_highlights = smoothstep(0.45, 0.95, luma) *
        vec3<f32>(0.014, 0.008, -0.004) * shadow_strength;
    let graded = clamp(lifted + shadow_cool + warm_highlights, vec3<f32>(0.0), vec3<f32>(1.0));
    return clamp(mix(vec3<f32>(luma), graded, 1.10), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn litd_editor_world_ed11_cinematic_veil(uv: vec2<f32>, color: vec3<f32>) -> vec3<f32> {
    let y = clamp(uv.y, 0.0, 1.0);
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let smoke_band = exp(-pow((y - 0.38) / 0.30, 2.0)) * 0.16;
    let low_mist = smoothstep(0.86, 0.48, y) * 0.07;
    let smoke = clamp(smoke_band + low_mist, 0.0, 0.22);
    let mist_color = vec3<f32>(0.108, 0.126, 0.138);
    let highlight_protect = smoothstep(0.52, 0.92, luma) * 0.34;
    let smoky = mix(color, max(color, mist_color), smoke * (1.0 - highlight_protect));
    let edge_distance = distance(uv, vec2<f32>(0.50, 0.42));
    let vignette = smoothstep(0.46, 0.96, edge_distance);
    let focal_warmth = (1.0 - vignette) * smoothstep(0.18, 0.82, luma);
    let warmed = smoky + vec3<f32>(0.034, 0.020, 0.008) * focal_warmth;
    return clamp(warmed * (1.0 - vignette * 0.18), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn litd_editor_blank_grid_line(pixel: vec2<f32>, step: f32, width: f32) -> f32 {
    let cell = abs(fract(pixel / step) - vec2<f32>(0.5)) * step;
    let nearest = min(cell.x, cell.y);
    return 1.0 - smoothstep(width, width + 1.0, nearest);
}

fn litd_editor_blank_world_grid(uv: vec2<f32>) -> vec3<f32> {
    let pixel = uv / composite.texel_size;
    let minor = litd_editor_blank_grid_line(
        pixel,
        composite.grid.x,
        composite.grid.z,
    );
    let major = litd_editor_blank_grid_line(
        pixel,
        composite.grid.y,
        composite.grid.z * 1.45,
    );
    let minor_color = vec3<f32>(0.10, 0.15, 0.20) * minor;
    let major_color = vec3<f32>(0.34, 0.27, 0.13) * major;
    return (minor_color + major_color) * composite.display.x;
}

fn litd_editor_world_background_atmosphere(uv: vec2<f32>) -> vec3<f32> {
    let y = clamp(uv.y, 0.0, 1.0);
    let top = vec3<f32>(0.010, 0.026, 0.052);
    let lower = vec3<f32>(0.020, 0.052, 0.088);
    let horizon = vec3<f32>(0.080, 0.135, 0.175);
    let vertical = mix(top, lower, smoothstep(0.08, 1.0, y));
    let mist = exp(-pow((y - 0.43) / 0.22, 2.0)) * 0.34;
    let vignette = smoothstep(0.86, 0.18, distance(uv, vec2<f32>(0.50, 0.45)));
    let distant_glow = horizon * mist * (0.72 + vignette * 0.28);
    return vertical + distant_glow;
}

fn litd_editor_world_clear_background_mask(scene: vec3<f32>) -> f32 {
    let clear_color = vec3<f32>(0.022, 0.030, 0.044);
    let delta = distance(scene, clear_color);
    return 1.0 - smoothstep(0.004, 0.020, delta);
}

@fragment
fn litd_editor_world_composite_fs(input: FullscreenOut) -> @location(0) vec4<f32> {
    let scene = textureSample(scene_texture, composite_sampler, input.uv).rgb;
    let bloom = textureSample(bloom_texture, composite_sampler, input.uv).rgb * composite.bloom_intensity;
    let blank_grid = litd_editor_blank_world_grid(input.uv);
    let background = litd_editor_world_background_atmosphere(input.uv);
    let background_mask = litd_editor_world_clear_background_mask(scene);
    let scene_with_background = mix(scene, background, background_mask * composite.display.y);
    let hdr = max((scene_with_background + bloom + blank_grid) * composite.exposure, vec3<f32>(0.0));
    let mapped = litd_editor_world_post_tonemap_aces(hdr);
    let graded = litd_editor_world_color_grade(mapped, composite.display.w);
    let cinematic = litd_editor_world_ed11_cinematic_veil(input.uv, graded);
    let weathered = mix(graded, cinematic, composite.display.z);
    let bar = max(
        1.0 - smoothstep(0.095, 0.115, input.uv.y),
        smoothstep(0.885, 0.905, input.uv.y),
    ) * composite.grid.w;
    let letterboxed = mix(weathered, vec3<f32>(0.0), clamp(bar, 0.0, 1.0));
    return vec4<f32>(pow(letterboxed, vec3<f32>(1.0 / 2.2)), 1.0);
}
