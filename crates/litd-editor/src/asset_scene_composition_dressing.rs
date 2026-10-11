use super::geometry::AssetPlacement;

#[derive(Debug, Clone, Copy)]
pub(super) struct DressingPlacement {
    pub(super) relative_path: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
    pub(super) map_kind: &'static str,
    pub(super) map_radius: f32,
    pub(super) color_tint: [f32; 4],
}

impl DressingPlacement {
    pub(super) fn asset_placement(self) -> AssetPlacement {
        AssetPlacement {
            file_name: self.relative_path,
            translation: self.translation,
            scale: self.scale,
            yaw_degrees: self.yaw_degrees,
        }
    }
}

pub(super) fn dressing_placements() -> &'static [DressingPlacement] {
    &DRESSING_PLACEMENTS
}

const KAYKIT_ARCH_GATE: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/arch_gate.gltf";
const KAYKIT_ARCH: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/arch.gltf";
const KAYKIT_GRAVE_A_DESTROYED: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/grave_A_destroyed.gltf";
const KAYKIT_GRAVE_B: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/grave_B.gltf";
const KAYKIT_POST_LANTERN: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/post_lantern.gltf";
const KAYKIT_PILLAR: &str = "sources/kaykit/game-kit/KayKit-Halloween-Bits-1.0/addons/kaykit_halloween_bits/Assets/gltf/pillar.gltf";

const DRESSING_PLACEMENTS: [DressingPlacement; 28] = [
    fire(
        "extracted/quaternius/manual-game-kit/fantasypropsmegakit/Fantasy_Props_MegaKit_Standard/Exports/glTF/CandleStick_Stand.gltf",
        -15.0,
        4.4,
        -32.0,
        4.2,
        -36.0,
        4.0,
    ),
    fire(
        "extracted/quaternius/manual-game-kit/fantasypropsmegakit/Fantasy_Props_MegaKit_Standard/Exports/glTF/CandleStick_Triple.gltf",
        16.0,
        4.4,
        -32.0,
        4.2,
        38.0,
        4.0,
    ),
    fire(
        "extracted/quaternius/manual-game-kit/fantasypropsmegakit/Fantasy_Props_MegaKit_Standard/Exports/glTF/Lantern_Wall.gltf",
        -31.0,
        5.0,
        -16.0,
        3.2,
        82.0,
        3.6,
    ),
    fire(
        "extracted/quaternius/manual-game-kit/fantasypropsmegakit/Fantasy_Props_MegaKit_Standard/Exports/glTF/Lantern_Wall.gltf",
        31.0,
        5.0,
        -17.0,
        3.2,
        -82.0,
        3.6,
    ),
    crystal(
        "sources/quaternius/game-kit/cubeworldkit/Environment/glTF/Crystal_Big.gltf",
        -52.0,
        0.9,
        -18.0,
        4.6,
        -28.0,
        5.2,
    ),
    crystal(
        "sources/quaternius/game-kit/cubeworldkit/Environment/glTF/Crystal_Big.gltf",
        48.0,
        1.0,
        20.0,
        4.1,
        37.0,
        5.0,
    ),
    crystal(
        "sources/quaternius/game-kit/cubeworldkit/Environment/glTF/Crystal_Small.gltf",
        -42.0,
        0.7,
        -5.0,
        3.4,
        18.0,
        3.7,
    ),
    crystal(
        "sources/quaternius/game-kit/cubeworldkit/Environment/glTF/Crystal_Big.gltf",
        37.0,
        0.8,
        30.0,
        2.8,
        -24.0,
        4.0,
    ),
    bone(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_LargeBones.gltf",
        -47.0,
        0.4,
        25.0,
        0.9,
        34.0,
        6.0,
    ),
    bone(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_Skulls.gltf",
        43.0,
        0.5,
        -31.0,
        1.0,
        -18.0,
        5.6,
    ),
    bone(
        "sources/quaternius/game-kit/piratekit/glTF/Prop_Skull.gltf",
        -6.0,
        0.8,
        -41.0,
        2.0,
        12.0,
        2.4,
    ),
    bone(
        "sources/quaternius/game-kit/piratekit/glTF/Prop_Skull.gltf",
        24.0,
        0.8,
        27.0,
        1.7,
        -68.0,
        2.2,
    ),
    cliff(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_Cliff1.gltf",
        -72.0,
        -12.0,
        4.0,
        0.28,
        78.0,
        8.0,
    ),
    cliff(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_Cliff2.gltf",
        70.0,
        -13.5,
        -6.0,
        0.24,
        -82.0,
        8.0,
    ),
    cliff(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_Cliff3.gltf",
        -24.0,
        -14.0,
        56.0,
        0.24,
        164.0,
        8.0,
    ),
    cliff(
        "sources/quaternius/game-kit/piratekit/glTF/Environment_Cliff4.gltf",
        26.0,
        -14.5,
        54.0,
        0.22,
        -156.0,
        8.0,
    ),
    spire(
        "sources/quaternius/game-kit/ultimateplatformer/Level_and_Mechanics/glTF/Tower.gltf",
        -55.0,
        -1.0,
        -36.0,
        0.19,
        -22.0,
        6.0,
    ),
    spire(
        "sources/quaternius/game-kit/ultimateplatformer/Level_and_Mechanics/glTF/Tower.gltf",
        58.0,
        -1.0,
        35.0,
        0.17,
        144.0,
        6.0,
    ),
    gate(KAYKIT_ARCH_GATE, -18.0, 0.8, 38.0, 4.8, 24.0, 6.2),
    gate(KAYKIT_ARCH, 21.0, 0.8, 37.0, 4.2, -22.0, 5.4),
    grave(KAYKIT_GRAVE_A_DESTROYED, -9.0, 0.7, 46.0, 2.7, -18.0, 2.8),
    grave(KAYKIT_GRAVE_B, 8.0, 0.7, 45.0, 2.4, 16.0, 2.5),
    grave(KAYKIT_GRAVE_A_DESTROYED, -27.0, 0.7, 29.0, 2.4, 41.0, 2.6),
    grave(KAYKIT_GRAVE_B, 28.0, 0.7, 29.0, 2.2, -36.0, 2.4),
    lantern(KAYKIT_POST_LANTERN, -31.0, 1.1, 36.0, 3.2, 68.0, 3.4),
    lantern(KAYKIT_POST_LANTERN, 32.0, 1.1, 34.0, 3.0, -68.0, 3.2),
    pillar(KAYKIT_PILLAR, -21.0, 0.8, 42.0, 3.9, 12.0, 3.4),
    pillar(KAYKIT_PILLAR, 19.0, 0.8, 42.0, 3.7, -14.0, 3.2),
];

const fn fire(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "fire",
        map_radius,
        color_tint: [1.55, 0.72, 0.28, 2.15],
    })
}

const fn crystal(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "crystal",
        map_radius,
        color_tint: [0.42, 0.86, 1.85, 2.35],
    })
}

const fn bone(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "bones",
        map_radius,
        color_tint: [0.70, 0.66, 0.58, 0.0],
    })
}

const fn cliff(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "cliff",
        map_radius,
        color_tint: [0.23, 0.29, 0.33, 0.0],
    })
}

const fn spire(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "spire",
        map_radius,
        color_tint: [0.42, 0.46, 0.54, 0.0],
    })
}

const fn gate(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "kaykit_gate",
        map_radius,
        color_tint: [0.22, 0.20, 0.18, 0.0],
    })
}

const fn grave(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "kaykit_grave",
        map_radius,
        color_tint: [0.32, 0.34, 0.38, 0.0],
    })
}

const fn lantern(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "kaykit_lantern",
        map_radius,
        color_tint: [1.45, 0.68, 0.27, 2.30],
    })
}

const fn pillar(
    relative_path: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
    map_radius: f32,
) -> DressingPlacement {
    placement(PlacementArgs {
        relative_path,
        x,
        y,
        z,
        scale,
        yaw_degrees,
        map_kind: "kaykit_pillar",
        map_radius,
        color_tint: [0.24, 0.24, 0.27, 0.0],
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

const fn placement(args: PlacementArgs) -> DressingPlacement {
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
    DressingPlacement {
        relative_path,
        translation: [x, y, z],
        scale,
        yaw_degrees,
        map_kind,
        map_radius,
        color_tint,
    }
}
