struct PbrMaterial {
    base_color_factor: vec4<f32>,
    emissive_factor: vec3<f32>,
    metallic_factor: f32,
    roughness_factor: f32,
    normal_scale: f32,
    occlusion_strength: f32,
    texture_mask: u32,
};

fn litd_distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    return alpha2 / max(3.14159265 * denom * denom, 0.001);
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
