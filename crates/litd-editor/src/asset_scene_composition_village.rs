use super::geometry::AssetPlacement;

pub(super) const MEDIEVAL_VILLAGE_GLTF_ROOT: &str = "extracted/quaternius/manual-game-kit/medievalvillagemegakit/Medieval_Village_MegaKit_Standard/Medieval_Village_MegaKit_Standard/glTF";

#[derive(Debug, Clone, Copy)]
pub(super) struct VillagePlacement {
    pub(super) file_name: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl VillagePlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.file_name,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn village_placements() -> &'static [VillagePlacement] {
    &VILLAGE_PLACEMENTS
}

const VILLAGE_PLACEMENTS: [VillagePlacement; 22] = [
    tower(
        "Wall_UnevenBrick_Straight.gltf",
        -59.0,
        1.2,
        19.0,
        2.3,
        -28.0,
        5.0,
    ),
    tower(
        "Wall_UnevenBrick_Window_Wide_Round.gltf",
        -59.0,
        7.5,
        19.0,
        2.3,
        -28.0,
        5.0,
    ),
    tower(
        "Roof_Tower_RoundTiles.gltf",
        -59.0,
        13.4,
        19.0,
        1.75,
        -28.0,
        5.2,
    ),
    tower(
        "Wall_UnevenBrick_Door_Round.gltf",
        60.0,
        1.1,
        17.0,
        2.2,
        32.0,
        4.8,
    ),
    tower(
        "Wall_UnevenBrick_Straight.gltf",
        60.0,
        7.0,
        17.0,
        2.2,
        32.0,
        4.8,
    ),
    tower(
        "Roof_Tower_RoundTiles.gltf",
        60.0,
        12.8,
        17.0,
        1.65,
        32.0,
        5.0,
    ),
    wall(
        "Corner_ExteriorWide_Brick.gltf",
        -48.0,
        1.0,
        34.0,
        3.0,
        -18.0,
        4.0,
    ),
    wall("Wall_Arch.gltf", -34.0, 1.0, 38.0, 2.8, 22.0, 3.8),
    wall(
        "Wall_UnevenBrick_Straight.gltf",
        -14.0,
        1.1,
        41.0,
        2.8,
        45.0,
        3.8,
    ),
    wall(
        "Wall_UnevenBrick_Window_Wide_Round.gltf",
        15.0,
        1.1,
        40.0,
        2.8,
        61.0,
        3.8,
    ),
    wall(
        "DoorFrame_Round_Brick.gltf",
        42.0,
        1.0,
        34.0,
        2.9,
        98.0,
        4.0,
    ),
    roof(
        "Roof_RoundTiles_8x14.gltf",
        -36.0,
        6.8,
        39.0,
        1.85,
        22.0,
        4.0,
    ),
    roof("Roof_Front_Brick8.gltf", 0.0, 7.2, 43.0, 1.9, 56.0, 4.0),
    roof(
        "Roof_RoundTiles_8x14.gltf",
        37.0,
        6.6,
        37.0,
        1.75,
        98.0,
        4.0,
    ),
    stair(
        "Stairs_Exterior_Platform.gltf",
        -26.0,
        0.8,
        28.0,
        2.4,
        9.0,
        4.0,
    ),
    stair(
        "Stairs_Exterior_Straight.gltf",
        26.0,
        0.8,
        27.0,
        2.4,
        -14.0,
        4.0,
    ),
    prop(
        "Prop_MetalFence_Ornament.gltf",
        -58.0,
        1.0,
        2.0,
        2.6,
        83.0,
        3.0,
    ),
    prop(
        "Prop_MetalFence_Ornament.gltf",
        58.0,
        1.0,
        1.0,
        2.6,
        -88.0,
        3.0,
    ),
    prop("Prop_Chimney2.gltf", -59.0, 10.2, 20.0, 1.9, 28.0, 2.4),
    prop("Prop_Chimney2.gltf", 60.0, 9.8, 18.0, 1.8, 101.0, 2.4),
    prop("Prop_Vine1.gltf", -43.0, 3.2, 35.0, 2.4, 26.0, 3.0),
    prop("Window_Roof_Wide.gltf", 2.0, 9.2, 42.0, 1.8, 66.0, 3.0),
];

const fn tower(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> VillagePlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "tower",
        map_radius,
    })
}

const fn wall(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> VillagePlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "wall",
        map_radius,
    })
}

const fn roof(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> VillagePlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "roof",
        map_radius,
    })
}

const fn stair(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> VillagePlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "stair",
        map_radius,
    })
}

const fn prop(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> VillagePlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "prop",
        map_radius,
    })
}

struct PlacementArgs {
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_kind: &'static str,
    map_radius: f32,
}

const fn placement(args: PlacementArgs) -> VillagePlacement {
    let PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
    } = args;
    VillagePlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint: tint_for_kind(map_kind),
    }
}

const fn tint_for_kind(map_kind: &'static str) -> [f32; 4] {
    match map_kind.as_bytes() {
        b"tower" => [0.15, 0.18, 0.24, 0.0],
        b"wall" => [0.15, 0.18, 0.23, 0.0],
        b"roof" => [0.018, 0.019, 0.026, 0.0],
        b"stair" => [0.14, 0.16, 0.20, 0.0],
        b"prop" => [0.12, 0.11, 0.10, 0.0],
        _ => [0.28, 0.29, 0.32, 0.0],
    }
}
