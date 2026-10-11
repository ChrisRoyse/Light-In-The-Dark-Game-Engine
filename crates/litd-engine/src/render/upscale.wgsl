// FSR 3.1-style temporal upscaler, ported to wgpu-27-native WGSL (#1741).
//
// One compute pass reconstructs the output resolution from a low-resolution color
// buffer plus render-resolution motion vectors, applying reprojection, neighborhood
// clamping (anti-ghosting), temporal accumulation, and RCAS sharpening.

struct UpscaleParams {
    // render_width, render_height, output_width, output_height
    dims: vec4<u32>,
    // sharpness, history_feedback, disocclusion_threshold, first_frame (0/1)
    params: vec4<f32>,
};

@group(0) @binding(0) var source_color: texture_2d<f32>;
@group(0) @binding(1) var source_motion: texture_2d<f32>;
@group(0) @binding(2) var history_color: texture_2d<f32>;
@group(0) @binding(3) var linear_samp: sampler;
@group(0) @binding(4) var<uniform> uParams: UpscaleParams;
@group(0) @binding(5) var out_color: texture_storage_2d<rgba8unorm, write>;

fn sample_color(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(source_color, linear_samp, uv, 0.0).rgb;
}

// FSR robust contrast adaptive sharpening against the 4-neighbor cross.
fn rcas(center: vec3<f32>, n: vec3<f32>, e: vec3<f32>, s: vec3<f32>, w: vec3<f32>, amount: f32) -> vec3<f32> {
    let mn = min(center, min(min(n, e), min(s, w)));
    let mx = max(center, max(max(n, e), max(s, w)));
    // High-pass minus contrast gate (no division => no NaN in flat regions),
    // clamped to the neighborhood color box (RCAS robust overshoot limiter).
    let blur = (n + e + s + w) * 0.25;
    let high_pass = center - blur;
    let gate = clamp((mx - mn) * 4.0, vec3<f32>(0.0), vec3<f32>(1.0));
    let sharpened = center + high_pass * clamp(amount, 0.0, 1.0) * gate;
    return clamp(sharpened, mn, mx);
}

@compute @workgroup_size(8, 8, 1)
fn litd_upscale_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_w = uParams.dims.z;
    let out_h = uParams.dims.w;
    if (gid.x >= out_w || gid.y >= out_h) {
        return;
    }
    let out_dims = vec2<f32>(f32(out_w), f32(out_h));
    let render_dims = vec2<f32>(f32(uParams.dims.x), f32(uParams.dims.y));
    let out_uv = (vec2<f32>(f32(gid.x), f32(gid.y)) + vec2<f32>(0.5)) / out_dims;

    // Spatial reconstruction of the current frame (bilinear upsample + neighbor taps).
    let texel = vec2<f32>(1.0) / render_dims;
    let center = sample_color(out_uv);
    let n = sample_color(out_uv + vec2<f32>(0.0, -texel.y));
    let e = sample_color(out_uv + vec2<f32>(texel.x, 0.0));
    let s = sample_color(out_uv + vec2<f32>(0.0, texel.y));
    let w = sample_color(out_uv + vec2<f32>(-texel.x, 0.0));

    // Neighborhood color box for history rectification (anti-ghosting).
    let box_min = min(center, min(min(n, e), min(s, w)));
    let box_max = max(center, max(max(n, e), max(s, w)));

    // Reproject prior output through the (render-res) motion vector.
    let motion = textureSampleLevel(source_motion, linear_samp, out_uv, 0.0).rg * 2.0 - vec2<f32>(1.0);
    let prev_uv = out_uv - motion;

    let first_frame = uParams.params.w;
    let threshold = uParams.params.z;
    var disocc = 0.0;
    if (first_frame > 0.5 || prev_uv.x < 0.0 || prev_uv.x > 1.0 || prev_uv.y < 0.0 || prev_uv.y > 1.0) {
        disocc = 1.0;
    } else if (threshold <= 0.0) {
        if (length(motion) > 0.0) { disocc = 1.0; }
    } else {
        disocc = clamp(length(motion) / threshold, 0.0, 1.0);
    }

    let history = textureSampleLevel(history_color, linear_samp, prev_uv, 0.0).rgb;
    let rectified = clamp(history, box_min, box_max);

    let feedback = uParams.params.y;
    let weight = clamp(feedback * (1.0 - disocc), 0.0, 1.0);
    let accum = clamp(mix(center, rectified, weight), vec3<f32>(0.0), vec3<f32>(1.0));

    let sharp = rcas(accum, n, e, s, w, uParams.params.x);
    textureStore(out_color, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(sharp, 1.0));
}
