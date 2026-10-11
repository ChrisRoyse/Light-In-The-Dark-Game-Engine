//! #1788 phase-1 map "Ashford Crossing": one 1v1 melee map authored through the real editor
//! project model -- a 16x16 world (#807 terrain, #806 tiles), mirrored starts (#817), resource
//! nodes per side + a contested center expansion, a cliff ridge choke, regions + named cameras
//! for future cinematics (#812/#813), fog enabled (#847), melee victory (#838). The runtime
//! config + placed forces + one authored AI profile per faction (#1786) feed lower_runtime_datasets.

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
/// Deterministic per-slot start cells the runtime seeds field units at (mirror of
/// `game_runtime::start_cell` for a 16x16 grid): slot 0 near (2,2), slot 1 near (13,13).
pub const START_P0: (i32, i32) = (2, 2);
pub const START_P1: (i32, i32) = (13, 13);

/// The `runtime.*` tuning the game host instantiates the match systems from (#827/#847/#848).
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
    ("runtime.match_seed", "1788"),
];

fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, String)]) -> EditorObject {
    let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
    for (key, value) in props {
        object = object.with_property(*key, value.clone());
    }
    object
}

/// Build the Ashford Crossing base document: a 16x16 world with a filled terrain grid,
/// playable bounds, and the auto-injected playable region.
pub fn ashford_document() -> Result<EditorDocument, EngineError> {
    let properties = EditorWorldProperties {
        display_name: "Ashford Crossing".to_string(),
        description: "M1 1v1 melee -- mirrored river crossing".to_string(),
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
        calyx_document_id: "calyx://world/issue1788-ashford-crossing".to_string(),
        revision_id: "issue1788-ashford-rev-1".to_string(),
        properties,
    };
    Ok(create_editor_world_record(spec)?.document)
}

/// Carve a cliff ridge down the map center (a choke): a wall of raised cliff cells at x==8
/// with a two-cell crossing gap at z in {7,8}, so the terrain narrows the approach while the
/// nav grid stays connected through the crossing.
pub fn carve_choke(document: &mut EditorDocument) {
    for cell in document.terrain_cells.values_mut() {
        if cell.tile_x == 8 && cell.tile_z != 7 && cell.tile_z != 8 {
            cell.cliff_level = 2;
            cell.height = 2.0;
        }
    }
}

/// Author the runtime config, resource nodes, placed starting forces for BOTH owners, regions,
/// named cameras, and start locations. AI profiles are added separately (per-config).
pub fn author_runtime_and_map(
    document: &mut EditorDocument,
    unit_art: &AssetVaultRecord,
    struct_art: &AssetVaultRecord,
) -> Result<(), EngineError> {
    carve_choke(document);

    let config: Vec<(&str, String)> = RUNTIME_CONFIG
        .iter()
        .map(|(k, v)| (*k, v.to_string()))
        .collect();
    document.upsert_object(obj(
        "ashford_config",
        "GameplayConstant",
        Transform::IDENTITY,
        &config,
    ))?;

    // Resource nodes: one main harvest node per side + one contested center expansion.
    for (id, x, z, amount) in [
        ("node_emberhold_main", 3.0, 3.0, 12000u64),
        ("node_veilspun_main", 12.0, 12.0, 12000),
        ("node_contested", 8.0, 8.0, 8000),
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

    // Placed starting forces: each side gets two structures (so losing one triggers retreat,
    // not defeat) plus a field soldier (the vision emitter that carries the wave).
    place_forces(document, "emberhold", "player_one", unit_art, struct_art)?;
    place_forces(document, "veilspun", "player_two", unit_art, struct_art)?;

    // Mirrored start locations (#817).
    for (slot, player, x, z) in [
        (0u32, "player_one", 2.0, 2.0),
        (1, "player_two", 13.0, 13.0),
    ] {
        document.upsert_object(obj(
            &format!("start_{player}"),
            "StartLocation",
            Transform::from_translation(Vec3::new(x, 0.0, z)),
            &[
                ("player_id", player.to_string()),
                ("slot", slot.to_string()),
                ("kind", "start_location".to_string()),
            ],
        ))?;
    }

    // Regions for future cinematics (#812) -- two bases + the crossing.
    for (id, name, cx, cz, hx, hz) in [
        (
            "region_emberhold_base",
            "Emberhold Base",
            3.0,
            3.0,
            2.5,
            2.5,
        ),
        (
            "region_veilspun_base",
            "Veilspun Base",
            12.0,
            12.0,
            2.5,
            2.5,
        ),
        ("region_crossing", "Ashford Crossing", 8.0, 8.0, 2.0, 3.0),
    ] {
        document.upsert_region(EditorRegion {
            id: id.to_string(),
            name: name.to_string(),
            center: Vec3::new(cx, 0.0, cz),
            half_extents: Vec3::new(hx, 8.0, hz),
            properties: Default::default(),
        })?;
    }

    // Named cameras (#813): a top-down overview + a cinematic finale framed on the crossing.
    document.upsert_named_camera(EditorNamedCamera::top_down(
        "camera_overview",
        "Ashford Overview",
        Vec3::new(8.0, 0.0, 8.0),
    ))?;
    document.upsert_named_camera(EditorNamedCamera::cinematic(
        "camera_finale",
        "Crossing Finale",
        PlayCameraConfig::preset(PlayCameraRig::Isometric),
        Vec3::new(8.0, 0.0, 7.0),
    ))?;
    Ok(())
}

fn place_forces(
    document: &mut EditorDocument,
    key: &str,
    owner: &str,
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

/// Add one authored computer-opponent profile (#1786) for a faction. `home_build` is a valid
/// build cell near that faction's start. The hall trains the army unit, so the wave forms from
/// the first built structure.
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
