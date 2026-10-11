struct LitdUiVertex {
    rect_px: vec4<f32>,
    uv: vec4<f32>,
    color: vec4<f32>,
};

struct LitdMinimap {
    world_min: vec4<f32>,
    world_max: vec4<f32>,
    rect_px: vec4<f32>,
};

struct LitdFogCell {
    state: u32,
};

fn litd_ui_sdf_alpha(distance_field: f32, smoothing: f32) -> f32 {
    return smoothstep(0.5 - smoothing, 0.5 + smoothing, distance_field);
}

fn litd_minimap_project(world_position: vec3<f32>, minimap: LitdMinimap) -> vec2<f32> {
    let uv = clamp(
        (world_position.xz - minimap.world_min.xz) / (minimap.world_max.xz - minimap.world_min.xz),
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    return minimap.rect_px.xy + uv * (minimap.rect_px.zw - minimap.rect_px.xy);
}

fn litd_fog_visibility(cell: LitdFogCell) -> f32 {
    if (cell.state == 2u) {
        return 1.0;
    }
    if (cell.state == 1u) {
        return 0.45;
    }
    return 0.0;
}
