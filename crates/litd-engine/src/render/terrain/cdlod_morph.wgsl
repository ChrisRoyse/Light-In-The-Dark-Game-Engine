// First-party CDLOD geomorph + per-tile splat vertex program (#1739).
//
// One shared NxN vertex grid is drawn per selected tile via instancing: the
// per-tile instance data stretches the grid over the tile's world rectangle and
// carries the tile's LOD morph region. Each vertex snaps toward the next-coarser
// LOD grid over [morph_start, morph_end] so adjacent LODs stay C0 continuous --
// no popping. Height and splat weights are sampled from the streamed node atlas.
//
// No mesh shaders, no runtime splat bake (render-tech skip list #1746): the splat
// weights are looked up from an authored per-tile splat map and blended here.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec3<f32>,
    _pad: f32,
};

// Per-tile instance: where the shared grid lands + its morph region.
struct TileInstance {
    // xy = tile origin (world XZ), zw = tile size (world XZ).
    rect: vec4<f32>,
    // x = morph_start, y = morph_end (world distances), z = lod, w = splat layer.
    morph: vec4<f32>,
};

// Global per-frame terrain params shared by every drawn tile. `grid_dim` is the
// vertex-grid resolution (vertices per tile side); it is a CONSTANT of the
// runtime config (`TerrainConfig::grid_dim`), not derivable from any buffer
// length, so it is supplied here as a uniform.
struct TerrainParams {
    grid_dim: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> tiles: array<TileInstance>;
@group(0) @binding(2) var height_atlas: texture_2d<f32>;
@group(0) @binding(3) var height_sampler: sampler;
@group(0) @binding(4) var splat_atlas: texture_2d<f32>;
@group(0) @binding(5) var<uniform> params: TerrainParams;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) splat: vec4<f32>,
};

// Morph factor in [0,1]: 0 = full detail, 1 = collapsed onto the coarser grid.
// Mirrors `morph::morph_factor` on the CPU exactly.
fn cdlod_morph_factor(morph_start: f32, morph_end: f32, dist: f32) -> f32 {
    let span = morph_end - morph_start;
    if (span <= 1e-6) {
        return select(0.0, 1.0, dist >= morph_end);
    }
    return clamp((dist - morph_start) / span, 0.0, 1.0);
}

// Snap odd grid indices toward their lower even neighbour as morph -> 1. Mirrors
// `morph::morph_grid_xz`: fract(grid * 0.5) * 2 is 1.0 on odd indices, 0 on even.
fn cdlod_snap(grid_index: f32, morph: f32) -> f32 {
    let is_odd = fract(grid_index * 0.5) * 2.0;
    return grid_index - is_odd * morph;
}

@vertex
fn vs_terrain(
    @builtin(instance_index) instance: u32,
    // grid vertex integer index on X and Z, in [0, grid_dim-1].
    @location(0) grid_xz: vec2<f32>,
) -> VsOut {
    let tile = tiles[instance];
    let grid_dim = params.grid_dim;
    let cell = tile.rect.zw / max(grid_dim - 1.0, 1.0);

    // Local world position before morph.
    let local = tile.rect.xy + grid_xz * cell;
    let height0 = textureSampleLevel(height_atlas, height_sampler, local, 0.0).r;
    let world0 = vec3<f32>(local.x, height0, local.y);

    // Distance-driven geomorph: recompute the snapped grid position + coarser
    // height and blend by the morph factor.
    let dist = distance(camera.eye, world0);
    let morph = cdlod_morph_factor(tile.morph.x, tile.morph.y, dist);
    let snapped_grid = vec2<f32>(
        cdlod_snap(grid_xz.x, morph),
        cdlod_snap(grid_xz.y, morph),
    );
    let snapped_local = tile.rect.xy + snapped_grid * cell;
    let coarse_height = textureSampleLevel(height_atlas, height_sampler, snapped_local, 0.0).r;
    let height = mix(height0, coarse_height, morph);

    let world = vec3<f32>(snapped_local.x, height, snapped_local.y);
    let splat = textureSampleLevel(splat_atlas, height_sampler, snapped_local, 0.0);

    var out: VsOut;
    out.clip_pos = camera.view_proj * vec4<f32>(world, 1.0);
    out.world_pos = world;
    out.splat = splat;
    return out;
}

@fragment
fn fs_terrain(in: VsOut) -> @location(0) vec4<f32> {
    // Per-tile splat blend hook: 4 authored layers weighted by `in.splat`.
    let w = in.splat / max(dot(in.splat, vec4<f32>(1.0)), 1e-4);
    let layer0 = vec3<f32>(0.32, 0.28, 0.22); // soil
    let layer1 = vec3<f32>(0.24, 0.42, 0.18); // grass
    let layer2 = vec3<f32>(0.55, 0.52, 0.48); // rock
    let layer3 = vec3<f32>(0.92, 0.94, 0.98); // snow
    let albedo = layer0 * w.x + layer1 * w.y + layer2 * w.z + layer3 * w.w;
    return vec4<f32>(albedo, 1.0);
}
