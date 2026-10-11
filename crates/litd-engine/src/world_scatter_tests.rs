use super::*;

fn brush() -> DoodadScatterBrush {
    DoodadScatterBrush {
        brush_id: "pine_grove".to_string(),
        display_name: "Pine Grove".to_string(),
        variants: vec![
            DoodadVariant {
                variant_id: "pine_a".to_string(),
                weight: 3,
            },
            DoodadVariant {
                variant_id: "pine_b".to_string(),
                weight: 1,
            },
        ],
        density_percent: 60,
        min_scale_milli: 800,
        max_scale_milli: 1200,
        min_yaw_deg: 0,
        max_yaw_deg: 359,
        cells: (0..8)
            .flat_map(|x| {
                (0..8).map(move |z| ScatterCell {
                    tile_x: x,
                    tile_z: z,
                })
            })
            .collect(),
        seed: 0x1234_5678,
    }
}

#[test]
fn scatter_is_deterministic() {
    let brush = brush();
    let first = brush.scatter().expect("scatter");
    let second = brush.scatter().expect("scatter");
    assert_eq!(first, second);
    assert!(!first.is_empty());
    // Every placement sits on an authored footprint cell.
    for placement in &first {
        assert!(
            brush
                .cells
                .iter()
                .any(|cell| cell.tile_x == placement.tile_x && cell.tile_z == placement.tile_z)
        );
        assert!(placement.scale_milli >= 800 && placement.scale_milli <= 1200);
        assert!(placement.yaw_deg <= 359);
    }
}

#[test]
fn density_controls_placement_count_monotonically() {
    let mut low = brush();
    low.density_percent = 20;
    let mut high = brush();
    high.density_percent = 100;
    let low_count = low.scatter().expect("low").len();
    let high_count = high.scatter().expect("high").len();
    assert!(high_count >= low_count);
    assert_eq!(high_count, high.cells.len());
}

#[test]
fn variant_weights_bias_selection() {
    let placements = brush().scatter().expect("scatter");
    let pine_a = placements
        .iter()
        .filter(|p| p.variant_id == "pine_a")
        .count();
    let pine_b = placements
        .iter()
        .filter(|p| p.variant_id == "pine_b")
        .count();
    // pine_a has 3x the weight; with a full grid it must dominate.
    assert!(pine_a > pine_b, "pine_a={pine_a} pine_b={pine_b}");
}

#[test]
fn different_seed_changes_layout() {
    let a = brush();
    let mut b = brush();
    b.seed = 0x9999_0000;
    assert_ne!(a.scatter().unwrap(), b.scatter().unwrap());
}

#[test]
fn edge_zero_density_rejected() {
    let mut brush = brush();
    brush.density_percent = 0;
    let error = brush.validate().unwrap_err();
    assert_eq!(error.code(), "WORLD_SCATTER_DENSITY");
}

#[test]
fn edge_inverted_scale_rejected() {
    let mut brush = brush();
    brush.min_scale_milli = 1500;
    brush.max_scale_milli = 1000;
    assert_eq!(brush.validate().unwrap_err().code(), "WORLD_SCATTER_SCALE");
}

#[test]
fn edge_duplicate_cell_rejected() {
    let mut brush = brush();
    brush.cells = vec![
        ScatterCell {
            tile_x: 1,
            tile_z: 1,
        },
        ScatterCell {
            tile_x: 1,
            tile_z: 1,
        },
    ];
    assert_eq!(
        brush.validate().unwrap_err().code(),
        "WORLD_SCATTER_DUP_CELL"
    );
}

#[test]
fn content_record_is_integer_only() {
    let record = brush().to_content_record("scatter-test").expect("record");
    assert_eq!(record.kind, EngineContentKind::Terrain);
    assert!(
        record
            .source
            .path
            .starts_with("editor://terrain/doodad-scatter/")
    );
    assert_eq!(record.source.source_hash.len(), 64);
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
