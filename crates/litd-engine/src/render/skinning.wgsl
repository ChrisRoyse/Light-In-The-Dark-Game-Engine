struct LitdJointPalette {
    matrices: array<mat4x4<f32>>,
};

@group(3) @binding(0) var<storage, read> litd_joint_palette: LitdJointPalette;

struct LitdSkinningVsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(10) joints: vec4<u32>,
    @location(11) weights: vec4<f32>,
};

struct LitdSkinningVsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

fn litd_weighted_joint_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    let total = max(dot(weights, vec4<f32>(1.0, 1.0, 1.0, 1.0)), 0.000001);
    let normalized = weights / total;
    return litd_joint_palette.matrices[joints.x] * normalized.x
        + litd_joint_palette.matrices[joints.y] * normalized.y
        + litd_joint_palette.matrices[joints.z] * normalized.z
        + litd_joint_palette.matrices[joints.w] * normalized.w;
}

fn litd_skin_position(position: vec3<f32>, joints: vec4<u32>, weights: vec4<f32>) -> vec4<f32> {
    return litd_weighted_joint_matrix(joints, weights) * vec4<f32>(position, 1.0);
}

fn litd_skin_normal(normal: vec3<f32>, joints: vec4<u32>, weights: vec4<f32>) -> vec3<f32> {
    return normalize((litd_weighted_joint_matrix(joints, weights) * vec4<f32>(normal, 0.0)).xyz);
}

@vertex
fn litd_skinning_vs(input: LitdSkinningVsIn) -> LitdSkinningVsOut {
    var output: LitdSkinningVsOut;
    output.clip_position = litd_skin_position(input.position, input.joints, input.weights);
    output.normal = litd_skin_normal(input.normal, input.joints, input.weights);
    return output;
}
