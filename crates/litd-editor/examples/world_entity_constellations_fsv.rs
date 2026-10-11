use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::EditorObject,
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties, EditorWorldRecord,
        create_editor_world_record,
    },
    world_memory::{
        AuthoredWorldMemoryInput, WorldMemoryConstellation, WorldMemoryIndex, WorldMemorySlotKind,
        WorldMemorySourceKind, WorldMemoryTypedSlot, build_world_memory_index,
        generated_world_script, parse_world_memory_constellation, parse_world_memory_index,
        read_world_memory_constellation, read_world_memory_index, validate_world_memory_index,
        write_world_memory_index,
    },
};
use litd_engine::math::Transform;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_ENTITY_CONSTELLATIONS_FSV",
        "issue1209_entity_constellations",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1209 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("world-memory-index.json");
    let aster_dir = root.join("aster");
    let before = path_state(&state_path, &aster_dir);

    let index = build_world_memory_index(&[entity_input("issue1209-fsv")])?;
    let writeback = write_world_memory_index(&state_path, &aster_dir, "issue1209-fsv", &index)?;
    let readback = read_world_memory_index(&aster_dir, "issue1209-fsv")?
        .ok_or("missing issue1209-fsv world-memory index")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster world-memory index bytes".into());
    }
    let parsed = parse_world_memory_index(&readback.bytes)?;
    let unit = read_constellation(&aster_dir, &parsed, WorldMemorySourceKind::Unit)?;
    let item = read_constellation(&aster_dir, &parsed, WorldMemorySourceKind::Item)?;
    let quest = read_constellation(&aster_dir, &parsed, WorldMemorySourceKind::Quest)?;
    let place = read_constellation(&aster_dir, &parsed, WorldMemorySourceKind::Place)?;
    let agent = read_constellation(&aster_dir, &parsed, WorldMemorySourceKind::Agent)?;

    let shape_error = shape_mismatch_edge(&parsed);
    let document = json!({
        "schema": "litd.issue1209.entity_constellations_fsv.v1",
        "issue": 1209,
        "source_of_truth": "Aster world-memory index row, per-constellation Aster rows, and JSON mirror readback",
        "before": before,
        "happy_path": {
            "before": {
                "entity_kinds": ["Unit", "Item", "Quest", "Place", "Agent"],
                "expected_per_entity_slots": ["Text", "Structured", "Code", "VectorSet"]
            },
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "index_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "index_bytes": readback.bytes.len(),
                "constellation_count": parsed.constellation_count,
                "constellation_rows": readback.constellation_rows.len(),
                "graph_rows": readback.graph_rows,
                "unit": summarize_entity(&unit),
                "item": summarize_entity(&item),
                "quest": summarize_entity(&quest),
                "place": summarize_entity(&place),
                "agent": summarize_entity(&agent),
                "state_matches_aster": disk_bytes == readback.bytes,
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "absent_code_slot",
                "before": {"entity": "unit_ashen_guard", "script_property": false},
                "after": summarize_slot(slot(&unit, WorldMemorySlotKind::Code))
            },
            {
                "edge": "multi_vector_slot",
                "before": {"entity": "unit_ashen_guard", "source_lenses": ["Text", "Structured"]},
                "after": summarize_slot(slot(&unit, WorldMemorySlotKind::VectorSet))
            },
            {
                "edge": "slot_shape_mismatch_fails_closed",
                "before": shape_error["before"].clone(),
                "after": shape_error["after"].clone()
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1209 entity constellation FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "index rows={} sha={}",
        readback.constellation_rows.len(),
        readback.sha256
    );
    println!("unit {}", summarize_entity(&unit));
    Ok(())
}

fn read_constellation(
    aster_dir: &Path,
    index: &WorldMemoryIndex,
    kind: WorldMemorySourceKind,
) -> Result<WorldMemoryConstellation, Box<dyn std::error::Error>> {
    let expected = index
        .constellations
        .iter()
        .find(|item| item.source_kind == kind)
        .ok_or_else(|| format!("missing {kind:?} constellation"))?;
    let row = read_world_memory_constellation(aster_dir, &expected.constellation_id)?
        .ok_or_else(|| format!("missing row {}", expected.constellation_id))?;
    let parsed = parse_world_memory_constellation(&row.bytes)?;
    if parsed.constellation_id != expected.constellation_id {
        return Err(format!("row id mismatch for {kind:?}").into());
    }
    Ok(parsed)
}

fn summarize_entity(entity: &WorldMemoryConstellation) -> serde_json::Value {
    json!({
        "constellation_id": entity.constellation_id,
        "source_kind": format!("{:?}", entity.source_kind),
        "source_id": entity.source_id,
        "slot_count": entity.slots.len(),
        "text": summarize_slot(slot(entity, WorldMemorySlotKind::Text)),
        "structured": summarize_slot(slot(entity, WorldMemorySlotKind::Structured)),
        "code": summarize_slot(slot(entity, WorldMemorySlotKind::Code)),
        "vector_set": summarize_slot(slot(entity, WorldMemorySlotKind::VectorSet)),
    })
}

fn summarize_slot(slot: &WorldMemoryTypedSlot) -> serde_json::Value {
    json!({
        "kind": format!("{:?}", slot.kind),
        "state": format!("{:?}", slot.state),
        "term_count": slot.terms.len(),
        "text_bytes": slot.shape.text_bytes,
        "vector_count": slot.shape.vector_count,
        "vector_dimensions": slot.shape.vector_dimensions,
        "missing_reason": slot.missing_reason,
        "fingerprint": slot.fingerprint,
        "vectors": slot.vectors.iter().map(|vector| json!({
            "vector_id": vector.vector_id,
            "source_lens": format!("{:?}", vector.source_lens),
            "dimension": vector.dimension,
            "values_len": vector.values.len(),
            "nonzero": vector.values.iter().filter(|value| **value != 0).count()
        })).collect::<Vec<_>>()
    })
}

fn shape_mismatch_edge(index: &WorldMemoryIndex) -> serde_json::Value {
    let mut invalid = index.clone();
    let unit = invalid
        .constellations
        .iter_mut()
        .find(|item| item.source_id == "unit_ashen_guard")
        .expect("unit constellation");
    let vector_slot = unit
        .slots
        .iter_mut()
        .find(|slot| slot.kind == WorldMemorySlotKind::VectorSet)
        .expect("vector slot");
    let before_dimension = vector_slot.vectors[0].dimension;
    vector_slot.vectors[0].values.push(99);
    let after_values = vector_slot.vectors[0].values.len();
    let error = validate_world_memory_index(&invalid).unwrap_err();
    json!({
        "before": {"dimension": before_dimension, "values_len": before_dimension},
        "after": {"values_len": after_values, "error": error.to_string(), "passed": true}
    })
}

fn entity_input(world_id: &str) -> AuthoredWorldMemoryInput {
    let world = world_record(world_id);
    AuthoredWorldMemoryInput {
        scripts: vec![generated_world_script(&world).unwrap()],
        world,
        abilities: Vec::new(),
        objects: vec![
            entity_object(
                "unit_ashen_guard",
                "Unit",
                "Ashen Guard",
                [
                    ("role", "gate defender"),
                    ("stats", "hp 420 armor 7 damage 18"),
                    ("faction", "Dawnward"),
                ],
            ),
            entity_object(
                "item_cinder_key",
                "Item",
                "Cinder Key",
                [
                    ("description", "opens the ember gate"),
                    ("rarity", "quest"),
                    ("charges", "1"),
                ],
            ),
            entity_object(
                "quest_open_gate",
                "Quest",
                "Open the Ember Gate",
                [
                    ("summary", "unlock the ember gate for Dawnward scouts"),
                    ("required_item", "item_cinder_key"),
                    ("reward_xp", "120"),
                ],
            ),
            entity_object(
                "place_ember_gate",
                "Place",
                "Ember Gate",
                [
                    ("description", "sealed obsidian gate at the ash road"),
                    ("biome", "ash_road"),
                    ("danger", "medium"),
                ],
            ),
            entity_object(
                "agent_gate_keeper",
                "Agent",
                "Gate Keeper",
                [
                    ("role", "warns players before opening the gate"),
                    ("mood", "stern"),
                    ("trust", "0.65"),
                ],
            ),
        ],
    }
}

fn entity_object<const N: usize>(
    id: &str,
    kind: &str,
    name: &str,
    properties: [(&str, &str); N],
) -> EditorObject {
    let mut object = EditorObject::new(id, kind, None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("source_ref", format!("Aster://issue1209/{kind}/{id}"));
    for (key, value) in properties {
        object = object.with_property(key, value);
    }
    object
}

fn world_record(world_id: &str) -> EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: format!("Issue 1209 {world_id}"),
        description: "Known no-flatten entity constellation world".to_string(),
        width_tiles: 8,
        height_tiles: 8,
        playable_bounds: EditorPlayableBounds {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: 8,
            max_tile_z_exclusive: 8,
        },
        ..Default::default()
    };
    create_editor_world_record(EditorWorldCreationSpec {
        calyx_document_id: format!("calyx://world/{world_id}"),
        revision_id: format!("{world_id}-rev"),
        properties,
    })
    .unwrap()
}

fn slot(entity: &WorldMemoryConstellation, kind: WorldMemorySlotKind) -> &WorldMemoryTypedSlot {
    entity
        .slots
        .iter()
        .find(|slot| slot.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} slot"))
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            if fs::metadata(path)?.len() != expected_len {
                return Err("issue1209 FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("issue1209 FSV artifact byte-count stabilization failed".into())
}

fn path_state(state_path: &Path, aster_dir: &Path) -> serde_json::Value {
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "aster_dir_exists": aster_dir.exists()
    })
}
