use super::geometry::AssetPlacement;

pub(super) const MODULAR_DUNGEON_KIT_ASSET_ID: &str = "modular-dungeon-kit";
pub(super) const MODULAR_DUNGEON_GLB_ROOT: &str =
    "extracted/kenney/game-kit/modular-dungeon-kit/Models/GLB_format";

#[derive(Debug, Clone, Copy)]
pub(super) struct CitadelPlacement {
    pub(super) file_name: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
}

impl CitadelPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.file_name,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn citadel_placements() -> &'static [CitadelPlacement] {
    &CITADEL_PLACEMENTS
}

const CITADEL_PLACEMENTS: [CitadelPlacement; 14] = [
    foundation("template-floor-big.glb", 0.0, 0.0, -4.0, 4.4, 8.0, 14.0),
    foundation(
        "template-floor-layer-hole.glb",
        -20.0,
        0.0,
        -22.0,
        4.1,
        18.0,
        11.0,
    ),
    foundation(
        "template-floor-layer-raised.glb",
        20.0,
        0.0,
        -20.0,
        4.1,
        -16.0,
        11.0,
    ),
    structure("room-large.glb", 0.0, 0.0, -15.0, 3.5, 17.0, 15.0),
    structure("room-wide.glb", -24.0, 0.0, -2.0, 3.2, 28.0, 12.0),
    structure("room-small.glb", 23.0, 0.0, -2.0, 3.3, -18.0, 9.0),
    structure("stairs.glb", 3.0, 0.0, -37.0, 3.5, 0.0, 9.0),
    structure("corridor-wide.glb", 0.0, 0.0, 14.0, 3.1, 20.0, 9.0),
    structure("corridor-wide-corner.glb", -25.0, 0.0, 15.0, 3.1, 45.0, 8.0),
    structure(
        "corridor-wide-junction.glb",
        25.0,
        0.0,
        15.0,
        3.1,
        -35.0,
        8.0,
    ),
    structure("gate-door.glb", 0.0, 0.0, -51.0, 3.6, 0.0, 7.0),
    structure("gate-metal-bars.glb", 35.0, 0.0, -25.0, 3.5, 42.0, 6.0),
    wall(
        "template-wall-detail-a.glb",
        -40.0,
        0.0,
        -24.0,
        4.2,
        -12.0,
        7.0,
    ),
    wall("template-wall-top.glb", 39.0, 0.0, 5.0, 4.2, 70.0, 7.0),
];

const fn foundation(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> CitadelPlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "foundation",
        map_radius,
    })
}

const fn structure(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> CitadelPlacement {
    placement(PlacementArgs {
        file_name,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "structure",
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
) -> CitadelPlacement {
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

const fn placement(args: PlacementArgs) -> CitadelPlacement {
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
    CitadelPlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
    }
}
