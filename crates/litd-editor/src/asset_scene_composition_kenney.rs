use super::geometry::AssetPlacement;

pub(super) const TOWER_DEFENSE_KIT_ASSET_ID: &str = "tower-defense-kit";
pub(super) const TOWER_DEFENSE_GLB_ROOT: &str =
    "extracted/kenney/game-kit/tower-defense-kit/Models/GLB_format";

#[derive(Debug, Clone, Copy)]
pub(super) struct KenneyPlacement {
    pub(super) file_name: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl KenneyPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.file_name,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn kenney_skyline_placements() -> &'static [KenneyPlacement] {
    &KENNEY_SKYLINE_PLACEMENTS
}

const KENNEY_SKYLINE_PLACEMENTS: [KenneyPlacement; 6] = [
    placement(PlacementArgs {
        file_name: "tower-square-middle-a.glb",
        x: -52.0,
        y: -4.0,
        z: -58.0,
        scale: 18.0,
        yaw_degrees: -12.0,
        map_kind: "kenney_skyline_tower",
        map_radius: 15.0,
        color_tint: STONE_TINT,
    }),
    placement(PlacementArgs {
        file_name: "tower-square-top-c.glb",
        x: -52.0,
        y: 26.9,
        z: -58.0,
        scale: 18.2,
        yaw_degrees: -12.0,
        map_kind: "kenney_skyline_roof",
        map_radius: 12.0,
        color_tint: ROOF_TINT,
    }),
    placement(PlacementArgs {
        file_name: "tower-round-middle-a.glb",
        x: 52.0,
        y: -4.5,
        z: -56.0,
        scale: 17.0,
        yaw_degrees: 18.0,
        map_kind: "kenney_skyline_tower",
        map_radius: 14.0,
        color_tint: STONE_TINT,
    }),
    placement(PlacementArgs {
        file_name: "tower-square-roof-c.glb",
        x: 52.0,
        y: 25.9,
        z: -56.0,
        scale: 17.2,
        yaw_degrees: 18.0,
        map_kind: "kenney_skyline_roof",
        map_radius: 11.5,
        color_tint: ROOF_TINT,
    }),
    placement(PlacementArgs {
        file_name: "wood-structure-high.glb",
        x: 0.0,
        y: -3.5,
        z: -68.0,
        scale: 21.0,
        yaw_degrees: 3.0,
        map_kind: "kenney_skyline_watchpost",
        map_radius: 14.5,
        color_tint: WOOD_TINT,
    }),
    placement(PlacementArgs {
        file_name: "detail-crystal-large.glb",
        x: 0.0,
        y: 17.5,
        z: -68.0,
        scale: 13.0,
        yaw_degrees: 3.0,
        map_kind: "kenney_skyline_crystal",
        map_radius: 12.0,
        color_tint: CRYSTAL_TINT,
    }),
];

const STONE_TINT: [f32; 4] = [0.13, 0.16, 0.23, 0.0];
const ROOF_TINT: [f32; 4] = [0.09, 0.11, 0.17, 0.0];
const WOOD_TINT: [f32; 4] = [0.14, 0.12, 0.13, 0.0];
const CRYSTAL_TINT: [f32; 4] = [0.14, 0.46, 1.35, 1.5];

struct PlacementArgs {
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_kind: &'static str,
    map_radius: f32,
    color_tint: [f32; 4],
}

const fn placement(args: PlacementArgs) -> KenneyPlacement {
    let PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    } = args;
    KenneyPlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    }
}
