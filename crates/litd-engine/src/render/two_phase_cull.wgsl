// Two-phase Hi-Z occlusion cull + indirect-draw compaction (issue #1740).
//
// One invocation per instance. Each instance is tested against the frustum and the
// Hi-Z depth pyramid (built from last frame's visible set). Visible instances are
// stream-compacted into an indirect draw-command buffer via an atomic counter; that
// counter is the count buffer consumed by multi_draw_indirect_count.
//
// "Two phase" split (recorded per instance so the readback can prove it):
//   phase 1 = instances that were visible LAST frame and are still visible now
//             (these prime the depth pyramid first, hence drawn first);
//   phase 2 = instances visible now that were NOT drawn in phase 1
//             (newly disoccluded / newly entered the frustum).
// Occluded instances fall into neither phase -- they are absent from the compacted
// set and counter, which is exactly what the FSV reads back to prove the cull.
//
// Depth convention: 0 = near, 1 = far. The pyramid holds nearest-occluder depth;
// an instance is occluded iff its nearest depth is strictly farther than the max
// pyramid depth over its screen footprint (+ bias). See hzb_build.wgsl.

struct InstanceBounds {
    aabb_min_depth: vec4<f32>,  // xyz = world-space AABB min, w = nearest depth (0..1)
    aabb_max: vec4<f32>,        // xyz = world-space AABB max, w = unused
    rect: vec4<f32>,            // screen rect: min.x, min.y, max.x, max.y (all 0..1)
    draw_meta: vec4<u32>,       // draw_meta.x = index_count for the emitted draw command
};

struct FrustumPlane {
    normal_distance: vec4<f32>, // xyz = normal, w = distance
};

struct IndirectDrawCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
};

struct CullParams {
    instance_count: u32,
    plane_count: u32,
    base_dim: u32,
    level_count: u32,
    flags: u32,            // bit 0 = Hi-Z occlusion enabled
    occlusion_bias: f32,
    _pad0: u32,
    _pad1: u32,
};

// Only 8 storage buffers are bound: the min-spec DX12/WebGPU baseline caps
// max_storage_buffers_per_shader_stage at 8 (#928 $500-PC target), so the flat mip
// offsets are recomputed in-shader (`litd_level_offset`) rather than bound.
@group(0) @binding(0) var<storage, read> instance_bounds: array<InstanceBounds>;
@group(0) @binding(1) var<storage, read> frustum_planes: array<FrustumPlane>;
@group(0) @binding(2) var<storage, read> pyramid: array<f32>;
@group(0) @binding(3) var<storage, read> prev_visible: array<u32>;
@group(0) @binding(4) var<uniform> params: CullParams;
@group(0) @binding(5) var<storage, read_write> counters: array<atomic<u32>>; // 0=total 1=phase1 2=phase2
@group(0) @binding(6) var<storage, read_write> compacted: array<u32>;
@group(0) @binding(7) var<storage, read_write> indirect: array<IndirectDrawCommand>;
@group(0) @binding(8) var<storage, read_write> visibility_class: array<u32>; // 0=cull 1=phase1 2=phase2

// Flat-buffer texel offset where a square mip level begins (finest = level 0). Matches
// HzbPyramidDims::level_offset on the CPU side.
fn litd_level_offset(level: i32) -> u32 {
    var off = 0u;
    var k = 0;
    loop {
        if (k >= level) { break; }
        let d = params.base_dim >> u32(k);
        off = off + d * d;
        k = k + 1;
    }
    return off;
}

fn litd_frustum_visible(index: u32) -> bool {
    let b = instance_bounds[index];
    for (var p = 0u; p < params.plane_count; p = p + 1u) {
        let plane = frustum_planes[p].normal_distance;
        var positive = b.aabb_min_depth.xyz;
        if (plane.x >= 0.0) { positive.x = b.aabb_max.x; }
        if (plane.y >= 0.0) { positive.y = b.aabb_max.y; }
        if (plane.z >= 0.0) { positive.z = b.aabb_max.z; }
        if (dot(plane.xyz, positive) + plane.w < 0.0) {
            return false;
        }
    }
    return true;
}

fn litd_hzb_occluded(index: u32) -> bool {
    if ((params.flags & 1u) == 0u) {
        return false;
    }
    let b = instance_bounds[index];
    let rect = b.rect;
    let obj_depth = b.aabb_min_depth.w;
    let base = f32(params.base_dim);
    let span = max((rect.z - rect.x) * base, (rect.w - rect.y) * base);
    var level = i32(ceil(log2(max(span, 1.0))));
    level = clamp(level, 0, i32(params.level_count) - 1);
    let dim = params.base_dim >> u32(level);
    let off = litd_level_offset(level);
    let fdim = f32(dim);
    let last = i32(dim) - 1;
    let x0 = clamp(i32(floor(rect.x * fdim)), 0, last);
    let y0 = clamp(i32(floor(rect.y * fdim)), 0, last);
    let x1 = clamp(i32(floor(rect.z * fdim)), 0, last);
    let y1 = clamp(i32(floor(rect.w * fdim)), 0, last);
    var maxd = 0.0;
    for (var y = y0; y <= y1; y = y + 1) {
        for (var x = x0; x <= x1; x = x + 1) {
            maxd = max(maxd, pyramid[off + u32(y) * dim + u32(x)]);
        }
    }
    return obj_depth > maxd + params.occlusion_bias;
}

@compute @workgroup_size(64)
fn litd_two_phase_cull(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= params.instance_count) {
        return;
    }
    let visible = litd_frustum_visible(index) && !litd_hzb_occluded(index);
    if (!visible) {
        visibility_class[index] = 0u;
        return;
    }
    let slot = atomicAdd(&counters[0], 1u);
    compacted[slot] = index;
    let b = instance_bounds[index];
    indirect[slot] = IndirectDrawCommand(b.draw_meta.x, 1u, 0u, 0, index);
    if (prev_visible[index] == 1u) {
        atomicAdd(&counters[1], 1u);
        visibility_class[index] = 1u;
    } else {
        atomicAdd(&counters[2], 1u);
        visibility_class[index] = 2u;
    }
}
