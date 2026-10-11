use std::{fs, path::PathBuf};

use litd_engine::math::Transform;

use super::*;
use crate::{
    history::EditorObject,
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

#[test]
fn game_entities_are_typed_no_flatten_constellations() {
    let index = build_world_memory_index(&[entity_input("issue1209-typed")]).unwrap();

    for (kind, id) in [
        (WorldMemorySourceKind::Unit, "unit_ashen_guard"),
        (WorldMemorySourceKind::Item, "item_cinder_key"),
        (WorldMemorySourceKind::Quest, "quest_open_gate"),
        (WorldMemorySourceKind::Place, "place_ember_gate"),
        (WorldMemorySourceKind::Agent, "agent_gate_keeper"),
    ] {
        let entity = constellation(&index, kind, id);
        assert_eq!(entity.slots.len(), 4);
        assert_populated(entity, WorldMemorySlotKind::Text);
        assert_populated(entity, WorldMemorySlotKind::Structured);
        assert_populated(entity, WorldMemorySlotKind::VectorSet);
        assert_eq!(
            slot(entity, WorldMemorySlotKind::Code).state,
            WorldMemorySlotState::Missing
        );
        assert_eq!(
            slot(entity, WorldMemorySlotKind::VectorSet).vectors.len(),
            2
        );
    }

    let unit = constellation(&index, WorldMemorySourceKind::Unit, "unit_ashen_guard");
    assert!(
        !slot(unit, WorldMemorySlotKind::Text)
            .text
            .as_ref()
            .unwrap()
            .contains("hp 420")
    );
    assert!(
        slot(unit, WorldMemorySlotKind::Structured)
            .text
            .as_ref()
            .unwrap()
            .contains("hp 420")
    );
    println!(
        "issue1209 happy constellations={} unit_slots={} unit_code={:?} vector_count={}",
        index.constellation_count,
        unit.slots.len(),
        slot(unit, WorldMemorySlotKind::Code).state,
        slot(unit, WorldMemorySlotKind::VectorSet).vectors.len()
    );
}

#[test]
fn authored_code_slot_is_separate_from_text_and_structured_slots() {
    let mut input = entity_input("issue1209-code");
    input.objects[0] = input.objects[0]
        .clone()
        .with_property("script", "function on_enter() return 'guarded' end");
    let index = build_world_memory_index(&[input]).unwrap();
    let unit = constellation(&index, WorldMemorySourceKind::Unit, "unit_ashen_guard");

    assert_populated(unit, WorldMemorySlotKind::Code);
    assert!(
        slot(unit, WorldMemorySlotKind::Code)
            .text
            .as_ref()
            .unwrap()
            .contains("on_enter")
    );
    assert!(
        !slot(unit, WorldMemorySlotKind::Text)
            .text
            .as_ref()
            .unwrap()
            .contains("on_enter")
    );
    println!(
        "issue1209 edge=code_slot before=script_property after=code_state:{:?} code_terms:{}",
        slot(unit, WorldMemorySlotKind::Code).state,
        slot(unit, WorldMemorySlotKind::Code).terms.len()
    );
}

#[test]
fn slot_shape_mismatch_fails_closed() {
    let mut index = build_world_memory_index(&[entity_input("issue1209-shape")]).unwrap();
    let (before_dimension, after_values) = {
        let unit = index
            .constellations
            .iter_mut()
            .find(|item| item.source_id == "unit_ashen_guard")
            .unwrap();
        let vector_slot = unit
            .slots
            .iter_mut()
            .find(|slot| slot.kind == WorldMemorySlotKind::VectorSet)
            .unwrap();
        let before_dimension = vector_slot.vectors[0].dimension;
        vector_slot.vectors[0].values.push(99);
        (before_dimension, vector_slot.vectors[0].values.len())
    };

    let error = validate_world_memory_index(&index).unwrap_err();

    assert!(error.to_string().contains("editor.world_memory.slot.shape"));
    println!(
        "issue1209 edge=shape_mismatch before_dimension={} after_values={} error={}",
        before_dimension, after_values, error
    );
}

#[test]
fn index_and_constellation_rows_persist_to_aster_and_read_back() {
    let index = build_world_memory_index(&[entity_input("issue1209-store")]).unwrap();
    let root = test_root("store");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("world-memory-index.json");
    let aster_dir = root.join("aster");

    let writeback =
        write_world_memory_index(&state_path, &aster_dir, "issue1209-store", &index).unwrap();
    let readback = read_world_memory_index(&aster_dir, "issue1209-store")
        .unwrap()
        .unwrap();
    let unit_id = constellation(&index, WorldMemorySourceKind::Unit, "unit_ashen_guard")
        .constellation_id
        .clone();
    let unit_row = read_world_memory_constellation(&aster_dir, &unit_id)
        .unwrap()
        .unwrap();
    let unit = parse_world_memory_constellation(&unit_row.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(readback.constellation_rows.len(), index.constellation_count);
    assert_eq!(unit.source_kind, WorldMemorySourceKind::Unit);
    assert_eq!(unit.source_id, "unit_ashen_guard");
    assert_eq!(slot(&unit, WorldMemorySlotKind::VectorSet).vectors.len(), 2);
    println!(
        "issue1209 aster index_bytes={} rows={} graph_rows={} unit_row_sha={} unit_slots={}",
        readback.bytes.len(),
        readback.constellation_rows.len(),
        readback.graph_rows,
        unit_row.sha256,
        unit.slots.len()
    );
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

fn world_record(world_id: &str) -> crate::world_creation::EditorWorldRecord {
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

fn constellation<'a>(
    index: &'a WorldMemoryIndex,
    kind: WorldMemorySourceKind,
    source_id: &str,
) -> &'a WorldMemoryConstellation {
    index
        .constellations
        .iter()
        .find(|item| item.source_kind == kind && item.source_id == source_id)
        .unwrap_or_else(|| panic!("missing {kind:?} constellation {source_id}"))
}

fn slot(
    constellation: &WorldMemoryConstellation,
    kind: WorldMemorySlotKind,
) -> &WorldMemoryTypedSlot {
    constellation
        .slots
        .iter()
        .find(|slot| slot.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} slot"))
}

fn assert_populated(constellation: &WorldMemoryConstellation, kind: WorldMemorySlotKind) {
    assert_eq!(
        slot(constellation, kind).state,
        WorldMemorySlotState::Populated
    );
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1209", label)
}
