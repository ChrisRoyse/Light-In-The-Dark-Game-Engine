struct TaaSettings {
    feedback: f32,
    motion_rejection: f32,
    _pad0: vec2<f32>,
};

fn litd_fxaa_luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

fn litd_motion_vector_uv(current_ndc: vec2<f32>, previous_ndc: vec2<f32>) -> vec2<f32> {
    return (current_ndc - previous_ndc) * 0.5;
}

fn litd_taa_resolve(current_color: vec3<f32>, history_color: vec3<f32>, motion_vector: vec2<f32>, settings: TaaSettings) -> vec3<f32> {
    let rejection = clamp(length(motion_vector) * settings.motion_rejection, 0.0, 1.0);
    let history_weight = settings.feedback * (1.0 - rejection);
    return clamp(mix(current_color, history_color, history_weight), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn litd_ssao_occlusion(sample_depth: f32, neighbor_depth: f32, radius: f32, bias: f32) -> f32 {
    let delta = sample_depth - neighbor_depth;
    if (delta <= bias) {
        return 0.0;
    }
    return clamp(delta / radius, 0.0, 1.0);
}

fn litd_stylized_ssr_fade(step_index: u32, max_steps: u32, intensity: f32) -> f32 {
    let t = f32(step_index) / max(f32(max_steps), 1.0);
    return intensity * (1.0 - smoothstep(0.65, 1.0, t));
}

struct ScreenSpaceGpuSettings {
    dims: vec4<u32>,
    jitter: vec4<f32>,
    ssao: vec4<f32>,
    taa: vec4<f32>,
    ssr: vec4<f32>,
};

struct FullscreenVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var history_tex: texture_2d<f32>;
@group(0) @binding(2) var geometry_tex: texture_2d<f32>;
@group(0) @binding(3) var<uniform> screen_settings: ScreenSpaceGpuSettings;

@vertex
fn litd_screen_fullscreen_vs(@builtin(vertex_index) vertex_index: u32) -> FullscreenVertex {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    let position = positions[vertex_index];
    var output: FullscreenVertex;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = position * 0.5 + vec2<f32>(0.5);
    return output;
}

fn litd_screen_dims() -> vec2<i32> {
    return vec2<i32>(i32(screen_settings.dims.x), i32(screen_settings.dims.y));
}

fn litd_screen_coord(uv: vec2<f32>) -> vec2<i32> {
    let dims = litd_screen_dims();
    let raw = vec2<i32>(floor(uv * vec2<f32>(f32(screen_settings.dims.x), f32(screen_settings.dims.y))));
    return clamp(raw, vec2<i32>(0), dims - vec2<i32>(1));
}

fn litd_clamp_coord(coord: vec2<i32>) -> vec2<i32> {
    return clamp(coord, vec2<i32>(0), litd_screen_dims() - vec2<i32>(1));
}

fn litd_source(coord: vec2<i32>) -> vec4<f32> {
    return textureLoad(source_tex, litd_clamp_coord(coord), 0);
}

fn litd_history(coord: vec2<i32>) -> vec4<f32> {
    return textureLoad(history_tex, litd_clamp_coord(coord), 0);
}

fn litd_geometry(coord: vec2<i32>) -> vec4<f32> {
    return textureLoad(geometry_tex, litd_clamp_coord(coord), 0);
}

fn litd_decode_motion(geometry: vec4<f32>) -> vec2<f32> {
    return geometry.gb * 2.0 - vec2<f32>(1.0);
}

fn litd_screen_luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn litd_neighbor_average(coord: vec2<i32>) -> vec3<f32> {
    let c = litd_source(coord).rgb;
    let l = litd_source(coord + vec2<i32>(-1, 0)).rgb;
    let r = litd_source(coord + vec2<i32>(1, 0)).rgb;
    let u = litd_source(coord + vec2<i32>(0, -1)).rgb;
    let d = litd_source(coord + vec2<i32>(0, 1)).rgb;
    return (c + l + r + u + d) * 0.2;
}

@fragment
fn litd_screen_msaa_resolve_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let color = litd_source(coord).rgb;
    let samples = max(f32(screen_settings.dims.z), 1.0);
    let smooth_weight = clamp((samples - 1.0) / 7.0, 0.0, 1.0) * 0.22;
    return vec4<f32>(mix(color, litd_neighbor_average(coord), smooth_weight), 1.0);
}

@fragment
fn litd_screen_ssao_gather_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let color = litd_source(coord).rgb;
    let center_depth = litd_geometry(coord).r;
    let radius = screen_settings.ssao.x;
    let bias = screen_settings.ssao.y;
    var occlusion = 0.0;
    occlusion += litd_ssao_occlusion(center_depth, litd_geometry(coord + vec2<i32>(1, 0)).r, radius, bias);
    occlusion += litd_ssao_occlusion(center_depth, litd_geometry(coord + vec2<i32>(-1, 0)).r, radius, bias);
    occlusion += litd_ssao_occlusion(center_depth, litd_geometry(coord + vec2<i32>(0, 1)).r, radius, bias);
    occlusion += litd_ssao_occlusion(center_depth, litd_geometry(coord + vec2<i32>(0, -1)).r, radius, bias);
    let ao = pow(clamp(1.0 - occlusion * 0.25, 0.0, 1.0), screen_settings.ssao.z);
    return vec4<f32>(color * ao, 1.0);
}

@fragment
fn litd_screen_ssao_blur_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    return vec4<f32>(litd_neighbor_average(coord), 1.0);
}

@fragment
fn litd_screen_motion_vectors_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let color = litd_source(coord).rgb;
    let motion = abs(litd_decode_motion(litd_geometry(coord)));
    return vec4<f32>(clamp(color + vec3<f32>(motion.x, motion.y, motion.x + motion.y) * 0.035, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

@fragment
fn litd_screen_stylized_ssr_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let dims = litd_screen_dims();
    let reflected_coord = vec2<i32>(coord.x, dims.y - 1 - coord.y);
    let color = litd_source(coord).rgb;
    let reflected = litd_source(reflected_coord).rgb;
    let depth = litd_geometry(coord).r;
    let fade = litd_stylized_ssr_fade(u32(coord.y), screen_settings.dims.y, screen_settings.ssr.x);
    let water_mask = smoothstep(0.55, 0.88, f32(coord.y) / max(f32(dims.y - 1), 1.0));
    let depth_mask = smoothstep(0.20, 0.95, depth);
    return vec4<f32>(mix(color, reflected.bgr, fade * water_mask * depth_mask), 1.0);
}

@fragment
fn litd_screen_taa_resolve_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let current = litd_source(coord).rgb;
    let history = litd_history(coord).rgb;
    let motion = litd_decode_motion(litd_geometry(coord));
    let settings = TaaSettings(screen_settings.taa.x, screen_settings.taa.y, vec2<f32>(0.0, 0.0));
    let resolved = litd_taa_resolve(current, history, motion, settings);
    return vec4<f32>(resolved, 1.0);
}

@fragment
fn litd_screen_fxaa_fs(input: FullscreenVertex) -> @location(0) vec4<f32> {
    let coord = litd_screen_coord(input.uv);
    let color = litd_source(coord).rgb;
    let luma_c = litd_screen_luma(color);
    let luma_l = litd_screen_luma(litd_source(coord + vec2<i32>(-1, 0)).rgb);
    let luma_r = litd_screen_luma(litd_source(coord + vec2<i32>(1, 0)).rgb);
    let luma_u = litd_screen_luma(litd_source(coord + vec2<i32>(0, -1)).rgb);
    let luma_d = litd_screen_luma(litd_source(coord + vec2<i32>(0, 1)).rgb);
    let contrast = max(max(abs(luma_c - luma_l), abs(luma_c - luma_r)), max(abs(luma_c - luma_u), abs(luma_c - luma_d)));
    let smoothed = litd_neighbor_average(coord);
    return vec4<f32>(mix(color, smoothed, smoothstep(0.06, 0.22, contrast) * 0.55), 1.0);
}
