// #1787 FSV authoring helper -- textually `include!`d by issue1787_lan_match_flow_fsv.rs.
//
// Authors a 1v1 melee (two owners) with NO AI profile, so the internal macro-AI is inactive and
// the lockstep command stream (orders from BOTH processes' generic bots) is the sole order
// source. Uses the real EditorObject/EditorDocument APIs, then assigns a vault mesh to every
// Unit (the editor play/packaging path requires a model on each Unit).

fn author_1v1_melee() -> Result<EditorDocument, EngineError> {
    fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, &str)]) -> EditorObject {
        let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
        for (key, value) in props {
            object = object.with_property(*key, *value);
        }
        object
    }

    let mut document = EditorDocument::new("calyx://world/issue1787-lan-melee", "rev-1")?;
    document.upsert_object(EditorObject::new("world", "World", None, Transform::IDENTITY))?;
    document.upsert_object(obj(
        "runtime_config",
        "GameplayConstant",
        Transform::IDENTITY,
        &[
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
            ("runtime.match_seed", "1787"),
        ],
    ))?;
    document.upsert_object(obj(
        "type_worker",
        "Unit",
        Transform::IDENTITY,
        &[
            ("runtime.role", "worker"),
            ("economy.gold_cost", "50"),
            ("economy.build_time_secs", "1"),
        ],
    ))?;
    document.upsert_object(obj(
        "type_town_hall",
        "Unit",
        Transform::IDENTITY,
        &[
            ("runtime.role", "structure"),
            ("economy.gold_cost", "200"),
            ("economy.build_time_secs", "1"),
            ("economy.food_provided", "20"),
            ("runtime.footprint_w", "2"),
            ("runtime.footprint_h", "2"),
            ("runtime.trains", "type_worker"),
        ],
    ))?;
    document.upsert_object(obj(
        "gold_mine",
        "ResourceNode",
        Transform::from_translation(Vec3::new(5.0, 0.0, 3.0)),
        &[
            ("resource.key", "gold"),
            ("resource.amount_total", "5000"),
            ("resource.yield_per_trip", "10"),
        ],
    ))?;
    for (id, owner, role) in [
        ("placed_hall_p0", "player_one", "structure"),
        ("placed_soldier_p0", "player_one", "soldier"),
        ("placed_soldier_p1", "player_two", "soldier"),
    ] {
        document.upsert_object(obj(
            id,
            "Unit",
            Transform::IDENTITY,
            &[("runtime.role", role), ("template.owner_player_id", owner)],
        ))?;
    }
    // Prefer any real vault model present during progressive #1878 rematerialization.
    // medievaldungeon/Rock1.fbx is not always on disk after partial intake; fail-loud if
    // the catalog has zero usable model paths rather than hardcoding a missing pack.
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/George.gltf"
                || record.relative_path
                    == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
                || (record.relative_path.ends_with(".glb")
                    && record.relative_path.contains("kaykit"))
                || (record.relative_path.ends_with(".gltf")
                    && record.relative_path.contains("polyhaven/models"))
        })
        .cloned()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "issue1787.fsv.vault_model".to_string(),
            reason: "missing real vault model record (need rematerialized Stan.gltf or other vault mesh)".to_string(),
        })?;
    for unit_id in UNIT_IDS {
        assign_vault_model_to_unit(&mut document, unit_id, &record)?;
    }
    Ok(document)
}
