// Hi-Z depth pyramid build (issue #1740): max-reduce one mip level per dispatch.
//
// The pyramid is stored as a single flat f32 buffer holding every level packed
// back to back (level 0 = base_dim x base_dim, halving each level). Each coarser
// texel is the MAX of the 2x2 block below it. MAX is the conservative reduction
// for a "nearest occluder depth" buffer (0 = near, 1 = far): a screen region is
// only fully occluded when its farthest nearest-surface is still nearer than the
// tested object, so a single max-sample can never wrongly cull a visible object.
//
// Reference: two-pass GPU occlusion culling (Kruskonja / Momber 2024; Anagnostou,
// "Experiments in GPU-based occlusion culling"). The base level is primed from the
// previous frame's visible set; this pass rebuilds the pyramid over it each frame.

struct BuildParams {
    src_offset: u32,
    dst_offset: u32,
    src_dim: u32,
    dst_dim: u32,
};

@group(0) @binding(0) var<storage, read_write> pyramid: array<f32>;
@group(0) @binding(1) var<uniform> params: BuildParams;

@compute @workgroup_size(8, 8, 1)
fn litd_hzb_build(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dx = gid.x;
    let dy = gid.y;
    if (dx >= params.dst_dim || dy >= params.dst_dim) {
        return;
    }
    let sdim = params.src_dim;
    let sx = dx * 2u;
    let sy = dy * 2u;
    // Dims are powers of two so the +1 neighbours always exist, but clamp anyway
    // so a misconfigured dispatch degrades safely instead of reading out of range.
    let x1 = min(sx + 1u, sdim - 1u);
    let y1 = min(sy + 1u, sdim - 1u);
    let a = pyramid[params.src_offset + sy * sdim + sx];
    let b = pyramid[params.src_offset + sy * sdim + x1];
    let c = pyramid[params.src_offset + y1 * sdim + sx];
    let d = pyramid[params.src_offset + y1 * sdim + x1];
    let reduced = max(max(a, b), max(c, d));
    pyramid[params.dst_offset + dy * params.dst_dim + dx] = reduced;
}
