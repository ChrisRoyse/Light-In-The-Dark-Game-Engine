// Renderer consumption of the lowered world-environment surfaces
// (issues #808 #814 #815 #816). The uniform is built on the CPU from the frozen
// lowered sim package (world_environment::uniform) -- never a live Calyx query.
//
// Covered here:
//   - #815 global exponential fog with an explicit visibility (cutoff) horizon.
//   - #808 deep/shallow water depth tint (transparent-at-shore -> deep colour).
//   - #814 ambient/sun term from the interpolated day/night keyframe.
// The map-boundary/skybox (#808) and minimap (#816) surfaces are data-only and do
// not need a fragment; their FSV proves persist + lowering, not this shader.

struct EnvUniform {
    // rgb = fog colour (0..1), w = fog density coefficient.
    fog_color: vec4<f32>,
    // x = fog start distance (m), y = fog visibility/cutoff distance (m).
    fog_range: vec4<f32>,
    // shallow water colour, rgba (0..1).
    water_shallow: vec4<f32>,
    // deep water colour, rgba (0..1).
    water_deep: vec4<f32>,
    // x = shallow depth (m), y = deep depth (m), z = shore blend (m).
    water_depths: vec4<f32>,
    // rgb = ambient/sun colour (0..1), w = sun intensity (0..1).
    ambient: vec4<f32>,
};

@group(0) @binding(0) var<uniform> env: EnvUniform;

// Exponential fog with a hard visibility cutoff. Returns 0 (clear) .. 1 (fully
// fogged). Matches world_environment::uniform::cpu_fog_factor op-for-op.
fn env_fog_factor(dist: f32) -> f32 {
    let d = max(dist - env.fog_range.x, 0.0);
    let f = 1.0 - exp(-env.fog_color.w * d);
    let capped = select(f, 1.0, dist >= env.fog_range.y);
    return clamp(capped, 0.0, 1.0);
}

// Deep/shallow water tint by bathymetry depth (m). At/below the shallow depth the
// water is the transparent shallow colour; at/above the deep depth it is the deep
// colour; between, a linear blend. Matches cpu_water_tint op-for-op.
fn env_water_tint(depth: f32) -> vec4<f32> {
    let span = max(env.water_depths.y - env.water_depths.x, 1.0e-3);
    let t = clamp((depth - env.water_depths.x) / span, 0.0, 1.0);
    return mix(env.water_shallow, env.water_deep, t);
}

// Apply ambient lighting then global fog to a base colour.
fn env_apply(base: vec3<f32>, dist: f32) -> vec3<f32> {
    let lit = base * env.ambient.rgb * env.ambient.w;
    let fog = env_fog_factor(dist);
    return mix(lit, env.fog_color.rgb, fog);
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen triangle: three clip-space verts covering the viewport.
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    out.uv = vec2<f32>(x, y);
    out.clip = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Depth/distance proxies from screen uv for the headless slice: the real
    // renderer feeds world depth; here uv drives the same math the CPU mirrors.
    let dist = in.uv.x * env.fog_range.y;
    let water_depth = in.uv.y * env.water_depths.y;
    let water = env_water_tint(water_depth);
    let shaded = env_apply(water.rgb, dist);
    return vec4<f32>(shaded, water.a);
}
