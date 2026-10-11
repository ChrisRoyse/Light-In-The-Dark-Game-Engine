//! #1788 phase-2 map "Ridgefork Crossing": 2v2 melee with four corner starts, two
//! forces (teams), mirrored resource nodes, and choke terrain. Authored through the
//! real editor project model (#817 starts, #836 forces, #838 victory, #847 fog).

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultRecord;
use litd_editor::history::{EditorDocument, EditorNamedCamera, EditorObject, EditorRegion};
use litd_editor::play::{PlayCameraConfig, PlayCameraRig};
use litd_editor::world_creation::{
    EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
    create_editor_world_record,
};
use litd_engine::EngineError;
use litd_engine::math::{Transform, Vec3};

use crate::factions::FactionIds;

pub const GRID: u32 = 16;

/// Force 0 = Emberhold team (player_one + player_two); force 1 = Veilspun team.
pub const FORCE_EMBER: &str = "0";
pub const FORCE_VEIL: &str = "1";

pub const RUNTIME_CONFIG: &[(&str, &str)] = &[
    ("runtime.tick_rate", "4"),
    ("runtime.worker_cells_per_sec", "8"),
    ("runtime.deposit_ticks", "1"),
    ("runtime.start_gold", "10000"),
    ("runtime.start_lumber", "10000"),
    ("runtime.upkeep_food_cap", "80"),
    ("runtime.day_length_ticks", "200"),
    ("runtime.base_sight_radius_mm", "6000"),
    ("runtime.night_sight_penalty_permille", "500"),
    ("runtime.fog_cell_size_mm", "1000"),
    ("runtime.fog_grid_width", "16"),
    ("runtime.fog_grid_height", "16"),
    ("runtime.match_seed", "17882"),
    ("runtime.map_id", "ridgefork_crossing"),
];

fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, String)]) -> EditorObject {
    let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
    for (key, value) in props {
        object = object.with_property(*key, value.clone());
    }
    object
}

pub fn ridgefork_document() -> Result<EditorDocument, EngineError> {
    let properties = EditorWorldProperties {
        display_name: "Ridgefork Crossing".to_string(),
        description: "M1 2v2 melee -- four corner starts, two forces".to_string(),
        width_tiles: GRID,
        height_tiles: GRID,
        playable_bounds: EditorPlayableBounds {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: GRID as i32,
            max_tile_z_exclusive: GRID as i32,
        },
        ..EditorWorldProperties::default()
    };
    let spec = EditorWorldCreationSpec {
        calyx_document_id: "calyx://world/issue1788-ridgefork-crossing".to_string(),
        revision_id: "issue1788-ridgefork-rev-1".to_string(),
        properties,
    };
    Ok(create_editor_world_record(spec)?.document)
}

/// Cross-shaped ridge with a center gap so all four starts stay path-connected.
pub fn carve_cross_choke(document: &mut EditorDocument) {
    for cell in document.terrain_cells.values_mut() {
        let on_x_wall = cell.tile_x == 8 && !(7..=8).contains(&cell.tile_z);
        let on_z_wall = cell.tile_z == 8 && !(7..=8).contains(&cell.tile_x);
        if on_x_wall || on_z_wall {
            cell.cliff_level = 2;
            cell.height = 2.0;
        }
    }
}

pub fn author_runtime_and_map(
    document: &mut EditorDocument,
    unit_art: &AssetVaultRecord,
    struct_art: &AssetVaultRecord,
) -> Result<(), EngineError> {
    carve_cross_choke(document);

    let config: Vec<(&str, String)> = RUNTIME_CONFIG
        .iter()
        .map(|(k, v)| (*k, v.to_string()))
        .collect();
    document.upsert_object(obj(
        "ridgefork_config",
        "GameplayConstant",
        Transform::IDENTITY,
        &config,
    ))?;

    // One harvest node near each start + contested center.
    for (id, x, z, amount) in [
        ("node_p0", 3.0, 3.0, 10000u64),
        ("node_p1", 12.0, 12.0, 10000),
        ("node_p2", 3.0, 12.0, 10000),
        ("node_p3", 12.0, 3.0, 10000),
        ("node_contested", 8.0, 8.0, 6000),
    ] {
        document.upsert_object(obj(
            id,
            "ResourceNode",
            Transform::from_translation(Vec3::new(x, 0.0, z)),
            &[
                ("resource.key", "gold".to_string()),
                ("resource.amount_total", amount.to_string()),
                ("resource.yield_per_trip", "10".to_string()),
            ],
        ))?;
    }

    // Force 0 (Emberhold team): player_one SW + player_two NE (slot 1 corner).
    place_forces(
        document,
        "ember_a",
        "player_one",
        FORCE_EMBER,
        unit_art,
        struct_art,
    )?;
    place_forces(
        document,
        "ember_b",
        "player_two",
        FORCE_EMBER,
        unit_art,
        struct_art,
    )?;
    // Force 1 (Veilspun team): player_three NW + player_four SE.
    place_forces(
        document,
        "veil_a",
        "player_three",
        FORCE_VEIL,
        unit_art,
        struct_art,
    )?;
    place_forces(
        document,
        "veil_b",
        "player_four",
        FORCE_VEIL,
        unit_art,
        struct_art,
    )?;

    for (slot, player, x, z) in [
        (0u32, "player_one", 2.0, 2.0),
        (1, "player_two", 12.0, 13.0),
        (2, "player_three", 2.0, 13.0),
        (3, "player_four", 13.0, 2.0),
    ] {
        document.upsert_object(obj(
            &format!("start_{player}"),
            "StartLocation",
            Transform::from_translation(Vec3::new(x, 0.0, z)),
            &[
                ("player_id", player.to_string()),
                ("slot", slot.to_string()),
                ("kind", "start_location".to_string()),
                (
                    "force_id",
                    if slot < 2 {
                        FORCE_EMBER.to_string()
                    } else {
                        FORCE_VEIL.to_string()
                    },
                ),
            ],
        ))?;
    }

    for (id, name, cx, cz) in [
        ("region_ember_sw", "Emberhold SW", 3.0, 3.0),
        ("region_ember_ne", "Emberhold NE", 12.0, 12.0),
        ("region_veil_nw", "Veilspun NW", 3.0, 12.0),
        ("region_veil_se", "Veilspun SE", 12.0, 3.0),
        ("region_cross", "Ridgefork Cross", 8.0, 8.0),
    ] {
        document.upsert_region(EditorRegion {
            id: id.to_string(),
            name: name.to_string(),
            center: Vec3::new(cx, 0.0, cz),
            half_extents: Vec3::new(2.5, 8.0, 2.5),
            properties: Default::default(),
        })?;
    }

    document.upsert_named_camera(EditorNamedCamera::top_down(
        "camera_overview",
        "Ridgefork Overview",
        Vec3::new(8.0, 0.0, 8.0),
    ))?;
    document.upsert_named_camera(EditorNamedCamera::cinematic(
        "camera_finale",
        "Cross Finale",
        PlayCameraConfig::preset(PlayCameraRig::Isometric),
        Vec3::new(8.0, 0.0, 8.0),
    ))?;
    Ok(())
}

fn place_forces(
    document: &mut EditorDocument,
    key: &str,
    owner: &str,
    force: &str,
    unit_art: &AssetVaultRecord,
    struct_art: &AssetVaultRecord,
) -> Result<(), EngineError> {
    for (suffix, role) in [
        ("hall_a", "structure"),
        ("hall_b", "structure"),
        ("soldier", "soldier"),
    ] {
        let id = format!("placed_{key}_{suffix}");
        document.upsert_object(obj(
            &id,
            "Unit",
            Transform::IDENTITY,
            &[
                ("runtime.role", role.to_string()),
                ("template.owner_player_id", owner.to_string()),
                ("template.force_id", force.to_string()),
            ],
        ))?;
        let art = if role == "structure" {
            struct_art
        } else {
            unit_art
        };
        assign_vault_model_to_unit(document, &id, art)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn author_ai_profile(
    document: &mut EditorDocument,
    profile_id: &str,
    display: &str,
    owner: &str,
    node_id: &str,
    ids: &FactionIds,
    home_build: (i32, i32),
    handicap_permille: u32,
) -> Result<(), EngineError> {
    let (bx, bz) = home_build;
    let build_order = format!(
        "{}@0:{bx},{bz};{}@120:{},{bz}",
        ids.hall_id,
        ids.barracks_id,
        bx + 2
    );
    document.upsert_object(obj(
        profile_id,
        "AiProfile",
        Transform::IDENTITY,
        &[
            ("ai.controlled_player", owner.to_string()),
            ("ai.display_name", display.to_string()),
            ("ai.decision_interval_ticks", "1".to_string()),
            ("ai.worker_node_id", node_id.to_string()),
            ("ai.worker_target", "3".to_string()),
            ("ai.army_unit_id", ids.army_id.clone()),
            ("ai.wave_size", "2".to_string()),
            ("ai.retreat_when_structures_below", "2".to_string()),
            ("ai.retreat_cooldown_ticks", "6".to_string()),
            ("ai.handicap_permille", handicap_permille.to_string()),
            ("ai.build_order", build_order),
        ],
    ))?;
    Ok(())
}
