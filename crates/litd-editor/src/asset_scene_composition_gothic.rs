use super::geometry::AssetPlacement;

#[derive(Debug, Clone, Copy)]
pub(super) struct GothicPlacement {
    pub(super) relative_path: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl GothicPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.relative_path,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }

    pub(super) fn mtl_relative_path(self) -> String {
        self.relative_path.replace(".obj", ".mtl")
    }
}

pub(super) fn gothic_placements() -> &'static [GothicPlacement] {
    &GOTHIC_PLACEMENTS
}

macro_rules! ruins {
    ($file:literal) => {
        concat!(
            "sources/quaternius/game-kit/ultimatemodularruins/OBJ/",
            $file
        )
    };
}

macro_rules! medieval {
    ($file:literal) => {
        concat!(
            "sources/quaternius/game-kit/modularmedievalbuildings/OBJ/",
            $file
        )
    };
}

const GOTHIC_PLACEMENTS: [GothicPlacement; 33] = [
    arch(
        ruins!("BridgeSection.obj"),
        -13.0,
        1.8,
        -39.0,
        15.8,
        -14.0,
        9.0,
    ),
    arch(
        ruins!("BridgeSection.obj"),
        15.0,
        1.8,
        -39.0,
        15.4,
        16.0,
        9.0,
    ),
    arch(
        ruins!("BridgeSection.obj"),
        0.0,
        3.2,
        -54.0,
        18.4,
        2.0,
        10.5,
    ),
    arch(
        ruins!("BridgeSection.obj"),
        -32.0,
        0.7,
        -12.0,
        11.0,
        42.0,
        7.5,
    ),
    arch(
        ruins!("BridgeSection.obj"),
        33.0,
        0.8,
        -13.0,
        11.0,
        -56.0,
        7.5,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        -43.0,
        -3.2,
        -40.0,
        19.0,
        0.0,
        7.5,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        44.0,
        -3.2,
        -39.0,
        18.8,
        0.0,
        7.5,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        -24.0,
        -4.0,
        -55.0,
        17.8,
        8.0,
        7.4,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        26.0,
        -4.0,
        -54.0,
        17.4,
        -8.0,
        7.4,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        -43.0,
        0.4,
        10.0,
        13.0,
        0.0,
        5.8,
    ),
    column(
        ruins!("Column_Round_Short.obj"),
        43.0,
        0.4,
        10.0,
        13.0,
        0.0,
        5.8,
    ),
    bridge(
        ruins!("BridgeSection.obj"),
        -7.0,
        -0.2,
        -56.0,
        12.6,
        -3.0,
        9.5,
    ),
    bridge(
        ruins!("BridgeSection.obj"),
        20.0,
        -0.4,
        -55.0,
        11.8,
        7.0,
        8.8,
    ),
    candle(ruins!("Candles_1.obj"), -18.0, 1.2, -37.0, 11.8, -15.0, 3.8),
    candle(ruins!("Candles_2.obj"), 22.0, 1.2, -36.0, 11.8, 22.0, 3.8),
    candle(ruins!("Candles_1.obj"), -4.0, 1.6, -50.0, 9.8, -4.0, 3.4),
    candle(ruins!("Candles_2.obj"), 8.0, 1.6, -48.0, 9.8, 10.0, 3.4),
    tower(
        medieval!("PointyTower.obj"),
        -9.0,
        -2.8,
        -27.0,
        15.8,
        -10.0,
        9.0,
    ),
    tower(
        medieval!("LargeTower.obj"),
        9.0,
        -3.0,
        -25.0,
        15.2,
        12.0,
        9.0,
    ),
    tower(
        medieval!("LargeTower.obj"),
        -28.0,
        -3.4,
        -27.0,
        12.4,
        24.0,
        8.0,
    ),
    tower(
        medieval!("LargeSimpleTower.obj"),
        29.0,
        -3.4,
        -25.0,
        12.2,
        -28.0,
        8.0,
    ),
    tower(
        medieval!("PointyTower.obj"),
        -1.0,
        -2.2,
        -44.0,
        13.0,
        0.0,
        8.4,
    ),
    tower(medieval!("Tower.obj"), 4.0, -5.2, -58.0, 9.6, 4.0, 7.4),
    tower(
        medieval!("SmallTower.obj"),
        -37.0,
        -1.0,
        3.0,
        7.0,
        58.0,
        6.0,
    ),
    tower(
        medieval!("SmallTower.obj"),
        38.0,
        -1.0,
        2.0,
        7.0,
        -58.0,
        6.0,
    ),
    wall(medieval!("Bridge.obj"), -17.0, -0.8, -1.0, 9.8, 90.0, 7.0),
    wall(medieval!("Tunnel.obj"), 8.0, -0.8, -2.0, 10.0, 90.0, 7.0),
    wall(medieval!("Tunnel.obj"), -3.0, -0.4, 15.0, 8.8, 0.0, 7.0),
    wall(medieval!("Bridge.obj"), 19.0, -0.4, 18.0, 8.2, 8.0, 6.2),
    wall(medieval!("Bridge.obj"), -24.0, -0.4, 17.0, 8.2, -12.0, 6.2),
    bridge(medieval!("Bridge.obj"), 0.0, -0.8, 32.0, 8.8, 0.0, 8.8),
    column(medieval!("Tower.obj"), -55.0, -4.0, -8.0, 6.4, 28.0, 5.0),
    column(medieval!("Tower.obj"), 55.0, -4.0, -7.0, 6.4, -28.0, 5.0),
];

const fn arch(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_arch",
        map_radius,
        color_tint: [0.13, 0.17, 0.24, 0.0],
    })
}

const fn column(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_column",
        map_radius,
        color_tint: [0.12, 0.15, 0.22, 0.0],
    })
}

const fn bridge(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_bridge",
        map_radius,
        color_tint: [0.12, 0.14, 0.20, 0.0],
    })
}

const fn candle(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_fire",
        map_radius,
        color_tint: [1.45, 0.66, 0.30, 2.85],
    })
}

const fn tower(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_tower",
        map_radius,
        color_tint: [0.09, 0.11, 0.16, 0.0],
    })
}

const fn wall(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> GothicPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "gothic_wall",
        map_radius,
        color_tint: [0.09, 0.11, 0.16, 0.0],
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

const fn placement(args: PlacementArgs) -> GothicPlacement {
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
    GothicPlacement {
        relative_path,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    }
}
