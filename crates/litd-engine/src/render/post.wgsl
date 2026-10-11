struct PostSettings {
    exposure_ev: f32,
    bloom_threshold: f32,
    bloom_knee: f32,
    bloom_intensity: f32,
    lut_strength: f32,
    saturation: f32,
    contrast: f32,
    output_gamma: f32,
    tonemap_operator: u32,
    bloom_enabled: u32,
    width: u32,
    height: u32,
};

struct PostGrade {
    lift: vec4<f32>,
    gamma: vec4<f32>,
    gain: vec4<f32>,
};

struct PostVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var litd_post_texture_a: texture_2d<f32>;
@group(0) @binding(1) var litd_post_texture_b: texture_2d<f32>;
@group(0) @binding(2) var<uniform> litd_post_settings: PostSettings;
@group(0) @binding(3) var<uniform> litd_post_grade: PostGrade;

fn litd_tonemap_aces(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp(
        (color * (vec3<f32>(a) * color + vec3<f32>(b))) /
            (color * (vec3<f32>(c) * color + vec3<f32>(d)) + vec3<f32>(e)),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

fn litd_tonemap_reinhard(color: vec3<f32>) -> vec3<f32> {
    return color / (vec3<f32>(1.0) + color);
}

fn litd_bloom_prefilter(color: vec3<f32>, threshold: f32, knee: f32) -> vec3<f32> {
    let brightness = max(max(color.r, color.g), color.b);
    let safe_knee = max(knee, 0.0001);
    let soft = pow(clamp(brightness - threshold + safe_knee, 0.0, safe_knee * 2.0), 2.0) / (4.0 * safe_knee);
    let contribution = max(max(brightness - threshold, soft), 0.0) / max(brightness, 0.0001);
    return color * contribution;
}

fn litd_apply_lut_grade(color: vec3<f32>, lift: vec3<f32>, gamma: vec3<f32>, gain: vec3<f32>, settings: PostSettings) -> vec3<f32> {
    let lifted = max(color + lift, vec3<f32>(0.0));
    let corrected = pow(lifted, vec3<f32>(1.0) / gamma) * gain;
    let luma = dot(corrected, vec3<f32>(0.2126, 0.7152, 0.0722));
    let saturated = mix(vec3<f32>(luma), corrected, settings.saturation);
    let contrasted = (saturated - vec3<f32>(0.5)) * settings.contrast + vec3<f32>(0.5);
    return clamp(mix(color, contrasted, settings.lut_strength), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn litd_gamma_encode(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    return pow(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / gamma));
}

fn litd_load_texture_a(uv: vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(litd_post_texture_a);
    let max_pixel = vec2<f32>(f32(dims.x - 1u), f32(dims.y - 1u));
    let pixel = vec2<i32>(clamp(
        floor(uv * vec2<f32>(f32(dims.x), f32(dims.y))),
        vec2<f32>(0.0),
        max_pixel,
    ));
    return textureLoad(litd_post_texture_a, pixel, 0).rgb;
}

fn litd_load_texture_b(uv: vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(litd_post_texture_b);
    let max_pixel = vec2<f32>(f32(dims.x - 1u), f32(dims.y - 1u));
    let pixel = vec2<i32>(clamp(
        floor(uv * vec2<f32>(f32(dims.x), f32(dims.y))),
        vec2<f32>(0.0),
        max_pixel,
    ));
    return textureLoad(litd_post_texture_b, pixel, 0).rgb;
}

fn litd_blur_texture_a(uv: vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(litd_post_texture_a);
    let texel = vec2<f32>(1.0 / f32(dims.x), 1.0 / f32(dims.y));
    let a = litd_load_texture_a(uv + texel * vec2<f32>(-0.5, -0.5));
    let b = litd_load_texture_a(uv + texel * vec2<f32>(0.5, -0.5));
    let c = litd_load_texture_a(uv + texel * vec2<f32>(-0.5, 0.5));
    let d = litd_load_texture_a(uv + texel * vec2<f32>(0.5, 0.5));
    return (a + b + c + d) * 0.25;
}

@vertex
fn litd_post_fullscreen_vs(@builtin(vertex_index) vertex_index: u32) -> PostVertexOut {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    var out: PostVertexOut;
    out.position = vec4<f32>(xy[vertex_index], 0.0, 1.0);
    out.uv = xy[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn litd_post_bloom_prefilter_fs(input: PostVertexOut) -> @location(0) vec4<f32> {
    let exposed = max(litd_load_texture_a(input.uv), vec3<f32>(0.0)) *
        pow(2.0, litd_post_settings.exposure_ev);
    let bloom = litd_bloom_prefilter(
        exposed,
        litd_post_settings.bloom_threshold,
        litd_post_settings.bloom_knee,
    );
    return vec4<f32>(bloom, 1.0);
}

@fragment
fn litd_post_bloom_downsample_fs(input: PostVertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(litd_blur_texture_a(input.uv), 1.0);
}

@fragment
fn litd_post_bloom_upsample_fs(input: PostVertexOut) -> @location(0) vec4<f32> {
    let upsampled = litd_blur_texture_a(input.uv);
    let base = litd_load_texture_b(input.uv);
    return vec4<f32>(base + upsampled, 1.0);
}

@fragment
fn litd_post_composite_fs(input: PostVertexOut) -> @location(0) vec4<f32> {
    var color = max(litd_load_texture_a(input.uv), vec3<f32>(0.0));
    color = color * pow(2.0, litd_post_settings.exposure_ev);
    if litd_post_settings.bloom_enabled != 0u {
        color = color + max(litd_load_texture_b(input.uv), vec3<f32>(0.0)) *
            litd_post_settings.bloom_intensity;
    }
    if litd_post_settings.tonemap_operator == 0u {
        color = litd_tonemap_aces(color);
    } else if litd_post_settings.tonemap_operator == 1u {
        color = litd_tonemap_reinhard(color);
    } else {
        color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    color = litd_apply_lut_grade(
        color,
        litd_post_grade.lift.xyz,
        litd_post_grade.gamma.xyz,
        litd_post_grade.gain.xyz,
        litd_post_settings,
    );
    color = litd_gamma_encode(color, litd_post_settings.output_gamma);
    return vec4<f32>(color, 1.0);
}
