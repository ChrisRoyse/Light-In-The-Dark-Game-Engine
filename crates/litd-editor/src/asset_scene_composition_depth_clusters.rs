use super::geometry::AssetPlacement;

#[derive(Debug, Clone, Copy)]
pub(super) struct DepthClusterPlacement {
    pub(super) relative_path: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl DepthClusterPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.relative_path,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn depth_cluster_placements() -> &'static [DepthClusterPlacement] {
    &DEPTH_CLUSTER_PLACEMENTS
}

macro_rules! castle {
    ($file:literal) => {
        concat!(
            "extracted/kenney/game-kit/castle-kit/Models/GLB_format/",
            $file
        )
    };
}

macro_rules! halloween {
    ($file:literal) => {
        concat!(
            "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/",
            "kaykit_halloween_bits/Assets/gltf/",
            $file
        )
    };
}

const DEPTH_CLUSTER_PLACEMENTS: [DepthClusterPlacement; 24] = [
    skyline_tower(
        castle!("tower-hexagon-base.glb"),
        -70.0,
        -3.0,
        -66.0,
        11.5,
        -13.0,
        10.0,
    ),
    skyline_tower(
        castle!("tower-hexagon-roof.glb"),
        -70.0,
        13.8,
        -66.0,
        11.5,
        -13.0,
        9.2,
    ),
    skyline_tower(
        castle!("tower-square-base.glb"),
        70.0,
        -3.0,
        -64.0,
        11.0,
        16.0,
        9.6,
    ),
    skyline_tower(
        castle!("tower-hexagon-roof-secondary.glb"),
        70.0,
        12.8,
        -64.0,
        11.0,
        16.0,
        9.0,
    ),
    skyline_wall(
        castle!("wall-narrow-gate.glb"),
        0.0,
        -3.0,
        -72.0,
        12.5,
        2.0,
        12.0,
    ),
    skyline_wall(
        castle!("wall-corner-slant.glb"),
        -43.0,
        -3.0,
        -68.0,
        10.5,
        -16.0,
        8.8,
    ),
    skyline_wall(
        castle!("wall-corner.glb"),
        43.0,
        -3.0,
        -68.0,
        10.5,
        18.0,
        8.8,
    ),
    skyline_wall(
        castle!("wall-stud.glb"),
        -23.0,
        -2.4,
        -70.0,
        13.0,
        -7.0,
        6.0,
    ),
    skyline_wall(castle!("wall-stud.glb"), 24.0, -2.4, -70.0, 13.0, 8.0, 6.0),
    skyline_banner(
        castle!("flag-pennant.glb"),
        -78.0,
        17.0,
        -62.0,
        10.0,
        12.0,
        4.0,
    ),
    skyline_banner(castle!("flag-wide.glb"), 78.0, 16.2, -61.0, 9.0, -15.0, 4.0),
    graveyard_anchor(halloween!("crypt.gltf"), -2.0, 0.8, 48.0, 4.8, 3.0, 6.0),
    graveyard_anchor(
        halloween!("shrine_candles.gltf"),
        -13.0,
        0.9,
        42.0,
        3.6,
        -19.0,
        4.8,
    ),
    graveyard_anchor(halloween!("shrine.gltf"), 16.0, 0.9, 41.0, 3.4, 18.0, 4.6),
    graveyard_fence(halloween!("fence_gate.gltf"), 0.0, 0.8, 36.0, 4.8, 0.0, 6.8),
    graveyard_fence(
        halloween!("fence_broken.gltf"),
        -32.0,
        0.8,
        31.0,
        4.5,
        33.0,
        6.4,
    ),
    graveyard_fence(
        halloween!("fence_seperate_broken.gltf"),
        31.0,
        0.8,
        30.0,
        4.4,
        -34.0,
        6.2,
    ),
    dead_tree(
        halloween!("tree_dead_large_decorated.gltf"),
        -45.0,
        0.0,
        20.0,
        4.1,
        -20.0,
        7.5,
    ),
    dead_tree(
        halloween!("tree_dead_large.gltf"),
        45.0,
        0.0,
        18.0,
        4.3,
        24.0,
        7.6,
    ),
    dead_tree(
        halloween!("tree_dead_medium.gltf"),
        -58.0,
        -0.3,
        -4.0,
        3.3,
        42.0,
        5.8,
    ),
    dead_tree(
        halloween!("tree_dead_medium.gltf"),
        60.0,
        -0.3,
        -6.0,
        3.4,
        -46.0,
        5.9,
    ),
    graveyard_fire(
        halloween!("lantern_standing.gltf"),
        -7.0,
        0.9,
        33.0,
        3.0,
        -11.0,
        3.5,
    ),
    graveyard_fire(
        halloween!("skull_candle.gltf"),
        8.0,
        0.9,
        34.0,
        2.8,
        13.0,
        3.2,
    ),
    graveyard_anchor(
        halloween!("coffin_decorated.gltf"),
        -23.0,
        0.8,
        34.0,
        3.1,
        -28.0,
        4.2,
    ),
];

const fn skyline_tower(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "ruin_skyline_tower",
        map_radius,
        color_tint: [0.11, 0.13, 0.19, 0.0],
    })
}

const fn skyline_wall(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "ruin_skyline_wall",
        map_radius,
        color_tint: [0.10, 0.12, 0.18, 0.0],
    })
}

const fn skyline_banner(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "ruin_skyline_banner",
        map_radius,
        color_tint: [0.27, 0.09, 0.08, 0.0],
    })
}

const fn graveyard_anchor(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "graveyard_anchor",
        map_radius,
        color_tint: [0.19, 0.18, 0.20, 0.0],
    })
}

const fn graveyard_fence(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "graveyard_fence",
        map_radius,
        color_tint: [0.14, 0.12, 0.11, 0.0],
    })
}

const fn dead_tree(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "dead_tree",
        map_radius,
        color_tint: [0.12, 0.10, 0.09, 0.0],
    })
}

const fn graveyard_fire(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DepthClusterPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "graveyard_fire",
        map_radius,
        color_tint: [1.35, 0.60, 0.24, 2.15],
    })
}

struct PlacementArgs {
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_kind: &'static str,
    map_radius: f32,
    color_tint: [f32; 4],
}

const fn placement(args: PlacementArgs) -> DepthClusterPlacement {
    let PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    } = args;
    DepthClusterPlacement {
        relative_path,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    }
}
