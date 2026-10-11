use super::geometry::AssetPlacement;

pub(super) const CASTLE_KIT_ASSET_ID: &str = "castle-kit";
pub(super) const CASTLE_KIT_GLB_ROOT: &str =
    "extracted/kenney/game-kit/castle-kit/Models/GLB_format";

#[derive(Debug, Clone, Copy)]
pub(super) struct ArchitecturePlacement {
    pub(super) file_name: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
}

impl ArchitecturePlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.file_name,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn architecture_placements() -> &'static [ArchitecturePlacement] {
    &ARCHITECTURE_PLACEMENTS
}

const ARCHITECTURE_PLACEMENTS: [ArchitecturePlacement; 12] = [
    structure("tower-base.glb", -23.0, 0.0, -19.0, 7.4, 10.0, 9.0),
    structure(
        "tower-square-mid-open-simple.glb",
        -7.0,
        0.0,
        -25.0,
        7.2,
        -8.0,
        8.0,
    ),
    structure("tower-square-base.glb", -17.0, 0.0, -7.0, 6.8, 28.0, 7.8),
    structure("wall-half.glb", -7.0, 0.0, -13.0, 8.0, 74.0, 7.0),
    structure("wall-narrow.glb", 7.0, 0.0, -10.0, 8.0, 74.0, 7.0),
    structure("wall-corner.glb", -29.0, 0.0, -8.0, 7.0, -18.0, 7.0),
    structure("wall-narrow-gate.glb", 13.0, 0.0, -18.0, 7.5, 43.0, 7.4),
    structure("wall-narrow-gate.glb", 23.0, 0.0, -5.0, 6.3, 60.0, 6.4),
    structure("gate.glb", 1.0, 0.0, -31.0, 7.0, 0.0, 7.0),
    structure("stairs-stone.glb", 5.0, 0.0, -21.0, 6.5, 15.0, 5.6),
    debris(
        "siege-ram-demolished.glb",
        26.0,
        0.0,
        -27.0,
        5.2,
        -25.0,
        5.2,
    ),
    debris("rocks-large.glb", -37.0, 0.0, -28.0, 4.6, 20.0, 4.6),
];

const fn structure(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> ArchitecturePlacement {
    ArchitecturePlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind: "structure",
        map_radius,
    }
}

const fn debris(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> ArchitecturePlacement {
    ArchitecturePlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind: "debris",
        map_radius,
    }
}
