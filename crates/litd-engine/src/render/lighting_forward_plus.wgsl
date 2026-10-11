const LIGHT_KIND_DIRECTIONAL: u32 = 0u;
const LIGHT_KIND_POINT: u32 = 1u;
const LIGHT_KIND_SPOT: u32 = 2u;

struct ClusterParams {
    cluster_count: u32,
    light_count: u32,
    max_lights_per_cluster: u32,
    x_slices: u32,
    y_slices: u32,
    z_slices: u32,
    target_width: u32,
    target_height: u32,
    shading_z_slice: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

struct ClusterBounds {
    min_pad: vec4<f32>,
    max_pad: vec4<f32>,
};

struct LightRecord {
    position_radius: vec4<f32>,
    direction_kind: vec4<f32>,
    color_intensity: vec4<f32>,
    spot_angles: vec4<f32>,
};

struct ClusterHeader {
    offset_count_clipped_pad: vec4<u32>,
};

struct AmbientLight {
    color_intensity: vec4<f32>,
};

@group(0) @binding(0) var<uniform> compute_params: ClusterParams;
@group(0) @binding(1) var<storage, read> compute_clusters: array<ClusterBounds>;
@group(0) @binding(2) var<storage, read> compute_lights: array<LightRecord>;
@group(0) @binding(3) var<storage, read_write> compute_headers: array<ClusterHeader>;
@group(0) @binding(4) var<storage, read_write> compute_light_indices: array<u32>;

@group(1) @binding(0) var<uniform> shade_params: ClusterParams;
@group(1) @binding(1) var<storage, read> shade_lights: array<LightRecord>;
@group(1) @binding(2) var<storage, read> shade_headers: array<ClusterHeader>;
@group(1) @binding(3) var<storage, read> shade_light_indices: array<u32>;
@group(1) @binding(4) var<uniform> shade_ambient: AmbientLight;

fn litd_sphere_intersects_cluster(center: vec3<f32>, radius: f32, bounds: ClusterBounds) -> bool {
    let closest = clamp(center, bounds.min_pad.xyz, bounds.max_pad.xyz);
    let delta = center - closest;
    return dot(delta, delta) <= radius * radius;
}

@compute @workgroup_size(64)
fn litd_cluster_lights(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let cluster_index = global_id.x;
    if (cluster_index >= compute_params.cluster_count) {
        return;
    }

    let bounds = compute_clusters[cluster_index];
    let base_offset = cluster_index * compute_params.max_lights_per_cluster;
    var accepted = 0u;
    var clipped = 0u;

    for (var light_index = 0u; light_index < compute_params.light_count; light_index = light_index + 1u) {
        let light = compute_lights[light_index];
        let kind = u32(light.direction_kind.w);
        if (kind == LIGHT_KIND_DIRECTIONAL) {
            continue;
        }

        if (kind == LIGHT_KIND_POINT || kind == LIGHT_KIND_SPOT) {
            let inside = litd_sphere_intersects_cluster(
                light.position_radius.xyz,
                light.position_radius.w,
                bounds,
            );
            if (inside && accepted < compute_params.max_lights_per_cluster) {
                compute_light_indices[base_offset + accepted] = light_index;
                accepted = accepted + 1u;
            } else if (inside) {
                clipped = clipped + 1u;
            }
        }
    }

    compute_headers[cluster_index].offset_count_clipped_pad =
        vec4<u32>(base_offset, accepted, clipped, 0u);
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn litd_forward_plus_vs(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let pos = positions[vertex_index];
    var out: VertexOut;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = pos * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

fn litd_cluster_index_from_uv(uv: vec2<f32>) -> u32 {
    let safe_uv = clamp(uv, vec2<f32>(0.0, 0.0), vec2<f32>(0.999999, 0.999999));
    let x = min(u32(safe_uv.x * f32(shade_params.x_slices)), shade_params.x_slices - 1u);
    let y = min(u32((1.0 - safe_uv.y) * f32(shade_params.y_slices)), shade_params.y_slices - 1u);
    let z = min(shade_params.shading_z_slice, shade_params.z_slices - 1u);
    return z * shade_params.x_slices * shade_params.y_slices + y * shade_params.x_slices + x;
}

@fragment
fn litd_forward_plus_fs(input: VertexOut) -> @location(0) vec4<f32> {
    var color = shade_ambient.color_intensity.rgb * shade_ambient.color_intensity.w;

    for (var light_index = 0u; light_index < shade_params.light_count; light_index = light_index + 1u) {
        let light = shade_lights[light_index];
        let kind = u32(light.direction_kind.w);
        if (kind == LIGHT_KIND_DIRECTIONAL) {
            let direction = normalize(light.direction_kind.xyz);
            let facing = max(direction.y, 0.0);
            color = color + light.color_intensity.rgb * light.color_intensity.w * (0.025 + 0.035 * facing);
        }
    }

    let cluster_index = litd_cluster_index_from_uv(input.uv);
    if (cluster_index < shade_params.cluster_count) {
        let header = shade_headers[cluster_index].offset_count_clipped_pad;
        let offset = header.x;
        let count = header.y;
        for (var ref_index = 0u; ref_index < count; ref_index = ref_index + 1u) {
            let light_index = shade_light_indices[offset + ref_index];
            if (light_index < shade_params.light_count) {
                let light = shade_lights[light_index];
                color = color + light.color_intensity.rgb * light.color_intensity.w * 0.045;
            }
        }
    }

    return vec4<f32>(min(color, vec3<f32>(1.0, 1.0, 1.0)), 1.0);
}
