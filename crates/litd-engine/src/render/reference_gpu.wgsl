struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    point_pos_radius: vec4<f32>,
    point_color_intensity: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) base_color: vec4<f32>,
    @location(7) emissive: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec4<f32>,
};

@vertex
fn litd_reference_scene_vs(input: VsIn) -> VsOut {
    let model = mat4x4<f32>(input.model_0, input.model_1, input.model_2, input.model_3);
    let world = model * vec4<f32>(input.position, 1.0);
    var out: VsOut;
    out.clip_position = camera.view_proj * world;
    out.world_position = world.xyz;
    out.normal = normalize((model * vec4<f32>(input.normal, 0.0)).xyz);
    out.base_color = input.base_color;
    out.emissive = input.emissive;
    return out;
}

fn litd_reference_tonemap(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + vec3<f32>(b))) / (color * (c * color + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn litd_reference_scene_fs(input: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let dir_l = max(dot(n, normalize(camera.light_dir.xyz)), 0.0);
    let to_point = camera.point_pos_radius.xyz - input.world_position;
    let dist = max(length(to_point), 0.001);
    let point_l = max(dot(n, normalize(to_point)), 0.0);
    let point_falloff = clamp(1.0 - dist / camera.point_pos_radius.w, 0.0, 1.0);
    let direct = camera.light_color.rgb * dir_l * camera.light_color.a;
    let local = camera.point_color_intensity.rgb * point_l * point_falloff * point_falloff * camera.point_color_intensity.a;
    let ambient = vec3<f32>(0.065, 0.065, 0.065);
    let hdr = input.base_color.rgb * (ambient + direct + local) + input.emissive.rgb;
    let mapped = pow(litd_reference_tonemap(hdr), vec3<f32>(1.0 / 2.2));
    return vec4<f32>(mapped, input.base_color.a);
}
