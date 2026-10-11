// Live-preview shader for the no-code PBR material editor (#862).
//
// Draws an analytic (orthographic-impostor) unit sphere lit by one directional
// light plus a flat ambient term, shaded with the same metallic-roughness
// Cook-Torrance/GGX model as `material_pbr.wgsl` and the CPU `shade()` mirror.
// The material is supplied as a lowered, fingerprinted uniform; the scene lights
// come from a second uniform. A miss writes the background colour with alpha 0
// so the CPU readback can separate foreground (the ball) from background.

const PI: f32 = 3.14159265358979323846;

struct PbrMaterialGpu {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    scalars: vec4<f32>,
    flags: vec4<u32>,
};

struct PreviewSceneGpu {
    light_dir: vec4<f32>,
    light_radiance: vec4<f32>,
    ambient: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> material: PbrMaterialGpu;
@group(0) @binding(1) var<uniform> scene: PreviewSceneGpu;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn litd_material_preview_vs(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    // Oversized fullscreen triangle (covers the whole [-1,1] clip square).
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    let corner = corners[vertex_index];
    var out: VsOut;
    out.clip_position = vec4<f32>(corner, 0.0, 1.0);
    out.ndc = corner;
    return out;
}

fn litd_distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    return alpha2 / max(PI * denom * denom, 0.001);
}

fn litd_geometry_schlick_ggx(n_dot: f32, k: f32) -> f32 {
    return n_dot / max(n_dot * (1.0 - k) + k, 0.001);
}

fn litd_geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = ((roughness + 1.0) * (roughness + 1.0)) / 8.0;
    return litd_geometry_schlick_ggx(n_dot_v, k) * litd_geometry_schlick_ggx(n_dot_l, k);
}

fn litd_fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn litd_material_preview_fs(in: VsOut) -> @location(0) vec4<f32> {
    let aspect = max(scene.viewport.z, 0.0001);
    let radius = scene.viewport.w;
    let background = vec3<f32>(0.025, 0.029, 0.038);

    // Orthographic impostor: project the pixel onto the sphere's front hemisphere.
    let plane = vec2<f32>(in.ndc.x * aspect, in.ndc.y);
    let r2 = radius * radius;
    let d2 = plane.x * plane.x + plane.y * plane.y;
    if (d2 > r2) {
        return vec4<f32>(background, 0.0);
    }

    let z = sqrt(max(r2 - d2, 0.0));
    let normal = normalize(vec3<f32>(plane.x, plane.y, z));
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let light = normalize(scene.light_dir.xyz);
    let half_vec = normalize(view + light);

    let n_dot_l = max(dot(normal, light), 0.0);
    let n_dot_v = max(dot(normal, view), 0.0);
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let v_dot_h = max(dot(view, half_vec), 0.0);

    let base = material.base_color.rgb;
    let metallic = material.scalars.x;
    let roughness = material.scalars.y;
    let occlusion = material.scalars.w;

    let f0 = mix(vec3<f32>(0.04, 0.04, 0.04), base, metallic);
    let fresnel = litd_fresnel_schlick(v_dot_h, f0);
    let distribution = litd_distribution_ggx(n_dot_h, roughness);
    let geometry = litd_geometry_smith(n_dot_v, n_dot_l, roughness);
    let denom = max(4.0 * n_dot_v * n_dot_l, 0.001);
    let specular = fresnel * (distribution * geometry / denom);
    let diffuse = (vec3<f32>(1.0, 1.0, 1.0) - fresnel) * (1.0 - metallic) * base / PI;
    let direct = (diffuse + specular) * scene.light_radiance.rgb * n_dot_l;

    let ambient = scene.ambient.rgb * base * (1.0 - metallic) * occlusion;
    let emissive = material.emissive.rgb;
    var color = direct + ambient + emissive;

    // Reinhard tonemap + approximate sRGB gamma so the Rgba8Unorm target is stable.
    color = color / (color + vec3<f32>(1.0, 1.0, 1.0));
    color = pow(color, vec3<f32>(1.0 / 2.2, 1.0 / 2.2, 1.0 / 2.2));
    return vec4<f32>(color, 1.0);
}
