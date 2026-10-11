use super::*;

pub(crate) fn field() -> DestructibleField {
    DestructibleField {
        field_id: "gate_yard".to_string(),
        display_name: "Gate Yard".to_string(),
        destructibles: vec![
            DestructibleSpec {
                id: "oak_1".to_string(),
                kind: DestructibleKind::Tree,
                tile_x: 2,
                tile_z: 3,
                yaw_deg: 90,
                max_hp: 50,
                armor: 0,
                model_ref: "models/oak.glb".to_string(),
            },
            DestructibleSpec {
                id: "north_gate".to_string(),
                kind: DestructibleKind::Gate,
                tile_x: 4,
                tile_z: 0,
                yaw_deg: 0,
                max_hp: 400,
                armor: 10,
                model_ref: "models/gate.glb".to_string(),
            },
            DestructibleSpec {
                id: "wall_a".to_string(),
                kind: DestructibleKind::Wall,
                tile_x: 5,
                tile_z: 0,
                yaw_deg: 0,
                max_hp: 800,
                armor: 25,
                model_ref: "models/wall.glb".to_string(),
            },
        ],
    }
}

#[test]
fn valid_field_round_trips_through_payload() {
    let field = field();
    let payload = field.to_payload().expect("payload");
    assert_eq!(payload.destructible_count, 3);
    assert_eq!(payload.total_max_hp, 50 + 400 + 800);
    let reconstructed = DestructibleField::from_payload(&payload).expect("from payload");
    assert_eq!(reconstructed.destructibles.len(), 3);
}

#[test]
fn edge_armor_at_or_above_hp_rejected() {
    let mut field = field();
    field.destructibles[0].armor = 50;
    assert_eq!(
        field.validate().unwrap_err().code(),
        "WORLD_DESTRUCTIBLE_ARMOR"
    );
}

#[test]
fn edge_duplicate_id_rejected() {
    let mut field = field();
    field.destructibles[1].id = "oak_1".to_string();
    field.destructibles[1].tile_x = 9;
    assert_eq!(
        field.validate().unwrap_err().code(),
        "WORLD_DESTRUCTIBLE_DUP_ID"
    );
}

#[test]
fn edge_two_destructibles_same_tile_rejected() {
    let mut field = field();
    field.destructibles[1].tile_x = 2;
    field.destructibles[1].tile_z = 3;
    assert_eq!(
        field.validate().unwrap_err().code(),
        "WORLD_DESTRUCTIBLE_DUP_TILE"
    );
}

#[test]
fn content_record_is_integer_only() {
    let record = field().to_content_record("destr-test").expect("record");
    assert_eq!(record.kind, EngineContentKind::Terrain);
    assert!(
        record
            .source
            .path
            .starts_with("editor://terrain/destructibles/")
    );
    assert_no_floats(&record.payload);
}

fn assert_no_floats(value: &serde_json::Value) {
    match value {
        serde_json::Value::Number(number) => {
            assert!(number.is_i64() || number.is_u64(), "float leaked: {number}");
        }
        serde_json::Value::Array(items) => items.iter().for_each(assert_no_floats),
        serde_json::Value::Object(map) => map.values().for_each(assert_no_floats),
        _ => {}
    }
}
