use super::geometry::AssetPlacement;

#[derive(Debug, Clone, Copy)]
pub(super) struct PolyhavenPlacement {
    pub(super) relative_path: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl PolyhavenPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.relative_path,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn polyhaven_placements() -> &'static [PolyhavenPlacement] {
    &POLYHAVEN_PLACEMENTS
}

const POLYHAVEN_PLACEMENTS: [PolyhavenPlacement; 8] = [
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/modular_fort_01/2k/modular_fort_01_2k.gltf",
        x: -4.0,
        y: -1.2,
        z: -28.0,
        scale: 2.12,
        yaw_degrees: -4.0,
        map_kind: "polyhaven_fort",
        map_radius: 50.0,
        color_tint: [0.18, 0.23, 0.32, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/large_castle_door/2k/large_castle_door_2k.gltf",
        x: 3.0,
        y: 1.0,
        z: -49.0,
        scale: 20.5,
        yaw_degrees: 0.0,
        map_kind: "polyhaven_gate",
        map_radius: 19.0,
        color_tint: [0.22, 0.24, 0.29, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/coastal_cliff_01/2k/coastal_cliff_01_2k.gltf",
        x: -58.0,
        y: -20.0,
        z: 6.0,
        scale: 0.70,
        yaw_degrees: 88.0,
        map_kind: "polyhaven_cliff",
        map_radius: 34.0,
        color_tint: [0.22, 0.28, 0.38, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/namaqualand_cliff_01/2k/namaqualand_cliff_01_2k.gltf",
        x: -42.0,
        y: -14.5,
        z: 39.0,
        scale: 5.8,
        yaw_degrees: 122.0,
        map_kind: "polyhaven_foreground_cliff",
        map_radius: 24.0,
        color_tint: [0.17, 0.22, 0.31, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/rock_face_01/2k/rock_face_01_2k.gltf",
        x: 67.0,
        y: -10.0,
        z: 30.0,
        scale: 7.4,
        yaw_degrees: -42.0,
        map_kind: "polyhaven_rock_face",
        map_radius: 18.0,
        color_tint: [0.20, 0.26, 0.35, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/rock_face_01/2k/rock_face_01_2k.gltf",
        x: -68.0,
        y: -10.5,
        z: -41.0,
        scale: 6.6,
        yaw_degrees: 32.0,
        map_kind: "polyhaven_rock_face",
        map_radius: 16.0,
        color_tint: [0.19, 0.25, 0.34, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/stone_01/2k/stone_01_2k.gltf",
        x: 37.0,
        y: -15.0,
        z: -58.0,
        scale: 7.2,
        yaw_degrees: -16.0,
        map_kind: "polyhaven_rock_face",
        map_radius: 16.0,
        color_tint: [0.18, 0.23, 0.32, 0.0],
    }),
    placement(PlacementArgs {
        relative_path: "sources/polyhaven/models/stone_01/2k/stone_01_2k.gltf",
        x: 0.0,
        y: -2.5,
        z: 29.0,
        scale: 21.0,
        yaw_degrees: -6.0,
        map_kind: "polyhaven_rock_face",
        map_radius: 19.0,
        color_tint: [0.16, 0.20, 0.28, 0.0],
    }),
];

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

const fn placement(args: PlacementArgs) -> PolyhavenPlacement {
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
    PolyhavenPlacement {
        relative_path,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    }
}
