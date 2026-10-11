struct InstanceBounds {
    min_depth: vec4<f32>,
    max_key: vec4<f32>,
    rect: vec4<f32>,
};

struct FrustumPlane {
    normal_distance: vec4<f32>,
};

struct HzbOccluder {
    rect: vec4<f32>,
    depth: f32,
    _pad0: vec3<f32>,
};

struct CullingParams {
    instance_count: u32,
    plane_count: u32,
    occluder_count: u32,
    flags: u32,
    occlusion_bias: f32,
    _pad0: vec3<f32>,
};

struct IndirectDrawCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
};

@group(0) @binding(0) var<storage, read> instance_bounds: array<InstanceBounds>;
@group(0) @binding(1) var<storage, read> frustum_planes: array<FrustumPlane>;
@group(0) @binding(2) var<storage, read> hzb_occluders: array<HzbOccluder>;
@group(0) @binding(3) var<storage, read_write> visibility: array<u32>;
@group(0) @binding(4) var<uniform> params: CullingParams;

fn litd_frustum_visible(instance_index: u32) -> bool {
    let bounds = instance_bounds[instance_index];
    for (var plane_index = 0u; plane_index < params.plane_count; plane_index = plane_index + 1u) {
        let plane = frustum_planes[plane_index].normal_distance;
        var positive = bounds.min_depth.xyz;
        if (plane.x >= 0.0) {
            positive.x = bounds.max_key.x;
        }
        if (plane.y >= 0.0) {
            positive.y = bounds.max_key.y;
        }
        if (plane.z >= 0.0) {
            positive.z = bounds.max_key.z;
        }
        if (dot(plane.xyz, positive) + plane.w < 0.0) {
            return false;
        }
    }
    return true;
}

fn litd_hzb_occluded(instance_index: u32) -> bool {
    let bounds = instance_bounds[instance_index];
    if ((params.flags & 1u) == 0u) {
        return false;
    }
    for (var occluder_index = 0u; occluder_index < params.occluder_count; occluder_index = occluder_index + 1u) {
        let occluder = hzb_occluders[occluder_index];
        let contained = occluder.rect.x <= bounds.rect.x
            && occluder.rect.y <= bounds.rect.y
            && occluder.rect.z >= bounds.rect.z
            && occluder.rect.w >= bounds.rect.w;
        let farther = bounds.min_depth.w > occluder.depth + params.occlusion_bias;
        if (contained && farther) {
            return true;
        }
    }
    return false;
}

@compute @workgroup_size(64)
fn litd_cull_instances(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let instance_index = global_id.x;
    if (instance_index >= params.instance_count) {
        return;
    }
    if (litd_frustum_visible(instance_index) && !litd_hzb_occluded(instance_index)) {
        visibility[instance_index] = 1u;
    } else {
        visibility[instance_index] = 0u;
    }
}
