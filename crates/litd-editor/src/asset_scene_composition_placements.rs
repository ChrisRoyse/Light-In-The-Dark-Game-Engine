use super::geometry::AssetPlacement;

pub(super) fn asset_placements() -> &'static [AssetPlacement] {
    &ASSET_PLACEMENTS
}

const ASSET_PLACEMENTS: [AssetPlacement; 26] = [
    placement("CommonTree_1.gltf", -68.0, 0.0, -38.0, 1.5, 18.0),
    placement("CommonTree_2.gltf", -50.0, 0.0, -47.0, 1.45, -22.0),
    placement("CommonTree_4.gltf", -70.0, 0.0, -10.0, 1.55, 64.0),
    placement("CommonTree_5.gltf", 66.0, 0.0, 27.0, 1.45, 144.0),
    placement("Pine_1.gltf", 49.0, 0.0, -44.0, 1.55, 8.0),
    placement("Pine_3.gltf", 72.0, 0.0, -16.0, 1.45, -44.0),
    placement("Pine_5.gltf", 43.0, 0.0, 46.0, 1.55, 94.0),
    placement("CommonTree_3.gltf", -21.0, 0.0, 48.0, 1.35, -72.0),
    placement("Pine_4.gltf", 9.0, 0.0, 51.0, 1.35, 35.0),
    placement("DeadTree_3.gltf", -61.0, 0.0, 22.0, 4.2, 12.0),
    placement("Rock_Medium_1.gltf", -7.0, 0.0, 6.0, 2.2, -15.0),
    placement("Rock_Medium_2.gltf", 24.0, 0.0, 8.0, 2.0, 32.0),
    placement("Rock_Medium_3.gltf", -34.0, 0.0, 24.0, 2.1, 78.0),
    placement("Bush_Common_Flowers.gltf", -58.0, 0.0, -28.0, 1.8, 28.0),
    placement("Bush_Common_Flowers.gltf", 57.0, 0.0, 13.0, 1.7, -36.0),
    placement("Fern_1.gltf", -18.0, 0.0, 32.0, 1.6, 114.0),
    placement("Plant_1_Big.gltf", 50.0, 0.0, 1.0, 1.6, 40.0),
    placement("Plant_7_Big.gltf", -61.0, 0.0, -21.0, 1.7, -18.0),
    placement("Grass_Common_Tall.gltf", -39.0, 0.0, -34.0, 2.0, 8.0),
    placement("Grass_Wispy_Tall.gltf", 50.0, 0.0, -8.0, 1.9, 76.0),
    placement("Flower_3_Group.gltf", -25.0, 0.0, 27.0, 1.4, 0.0),
    placement("Flower_4_Group.gltf", 21.0, 0.0, 36.0, 1.35, 88.0),
    placement("Mushroom_Common.gltf", -48.0, 0.0, 2.0, 1.3, 30.0),
    placement("Mushroom_Laetiporus.gltf", 58.0, 0.0, -27.0, 1.25, -48.0),
    placement("RockPath_Round_Wide.gltf", -12.0, 0.05, -14.0, 3.4, -18.0),
    placement("RockPath_Square_Wide.gltf", 18.0, 0.05, -1.0, 3.2, 24.0),
];

const fn placement(
    file_name: &'static str,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    yaw_degrees: f32,
) -> AssetPlacement {
    AssetPlacement {
        file_name,
        translation: [x, y, z],
        scale,
        yaw_degrees,
    }
}
