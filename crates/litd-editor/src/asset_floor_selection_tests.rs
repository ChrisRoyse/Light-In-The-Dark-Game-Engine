use super::*;

fn entry(asset_id: &str, asset_type: &str, relative_path: &str, size: u64) -> CatalogEntry {
    CatalogEntry {
        source: "polyhaven".to_string(),
        asset_id: asset_id.to_string(),
        asset_type: asset_type.to_string(),
        title: asset_id.replace('_', " "),
        relative_path: relative_path.to_string(),
        declared_size: size,
    }
}

/// A small real-shaped catalog: names classify through the SAME Rust classifier the vault
/// coverage validator uses, so this exercises the real single-source-of-truth path.
fn sample_catalog() -> Vec<CatalogEntry> {
    vec![
        entry(
            "medieval_castle_gate",
            "model",
            "sources/ph/castle/castle_1k.gltf",
            4_000,
        ),
        entry(
            "stone_tower_ruin",
            "model",
            "sources/ph/tower/tower_1k.gltf",
            6_000,
        ),
        entry(
            "skeleton_warrior",
            "model",
            "sources/ph/skel/skel_1k.gltf",
            3_000,
        ),
        entry(
            "village_house",
            "model",
            "sources/ph/house/house_1k.gltf",
            5_000,
        ),
        entry(
            "rocky_ground",
            "material",
            "sources/acg/ground/ground_1k.jpg",
            1_500,
        ),
        entry("oak_tree", "model", "sources/ph/tree/tree_1k.gltf", 2_500),
        entry("stone_tomb", "model", "sources/ph/tomb/tomb_1k.gltf", 3_500),
        entry(
            "torch_flame",
            "model",
            "sources/ph/torch/torch_1k.gltf",
            900,
        ),
        entry(
            "footstep_grass",
            "audio",
            "extracted/kenney/audio/footstep.ogg",
            200,
        ),
    ]
}

#[test]
fn happy_path_meets_small_floors_and_stops() {
    let catalog = sample_catalog();
    let floors = parse_floor_overrides(&[
        "architecture=2".to_string(),
        "audio=1".to_string(),
        "vfx_light=1".to_string(),
    ])
    .unwrap();
    let selection = select_floor(&catalog, &floors).unwrap();
    println!(
        "happy selected={} bytes={} role_counts={:?} floors={:?}",
        selection.selected_count,
        selection.selected_declared_bytes,
        selection.role_counts,
        selection.floors
    );
    assert_eq!(*selection.role_counts.get("architecture").unwrap(), 2);
    assert!(*selection.role_counts.get("audio").unwrap() >= 1);
    assert!(*selection.role_counts.get("vfx_light").unwrap() >= 1);
    // cheapest-first: torch (900) and footstep (200) are picked before the 4-6k arch models.
    assert!(
        selection
            .selected
            .iter()
            .any(|e| e.asset_id == "torch_flame")
    );
    assert!(
        selection
            .selected
            .iter()
            .any(|e| e.asset_id == "footstep_grass")
    );
    // stops once floors met: does NOT drag in every architecture asset in the catalog.
    assert!(selection.selected_count < catalog.len());
}

#[test]
fn cheapest_first_order_is_deterministic() {
    let catalog = sample_catalog();
    let floors = parse_floor_overrides(&["architecture=1".to_string()]).unwrap();
    let first = select_floor(&catalog, &floors).unwrap();
    let second = select_floor(&catalog, &floors).unwrap();
    assert_eq!(first, second);
    // The single architecture pick is the cheapest architecture asset (village_house at 5k is
    // dearer than castle_gate 4k; only those two are pure-architecture below tower 6k).
    println!("arch pick={:?}", first.selected);
    assert_eq!(first.selected[0].asset_id, "medieval_castle_gate");
}

#[test]
fn unmet_floor_fails_loud_naming_the_role() {
    let catalog = sample_catalog();
    // Only one character asset exists; asking for 5 cannot be satisfied.
    let floors = parse_floor_overrides(&["character=5".to_string()]).unwrap();
    let error = select_floor(&catalog, &floors).unwrap_err().to_string();
    println!("unmet error={error}");
    assert!(error.contains("editor.asset_floor.select.unmet"));
    assert!(error.contains("character=1<5"));
}

#[test]
fn empty_catalog_fails_loud_rather_than_restoring_nothing() {
    let floors = parse_floor_overrides(&["audio=1".to_string()]).unwrap();
    let error = select_floor(&[], &floors).unwrap_err().to_string();
    println!("empty select error={error}");
    assert!(error.contains("editor.asset_floor.select.empty"));
}

#[test]
fn parse_catalog_empty_fails_loud() {
    let error = parse_catalog("   \n  ").unwrap_err().to_string();
    println!("empty catalog error={error}");
    assert!(error.contains("editor.asset_floor.catalog.empty"));
}

#[test]
fn parse_catalog_bad_json_fails_loud_with_line() {
    let text = "{\"source\":\"polyhaven\",\"asset_id\":\"a\",\"asset_type\":\"model\",\"relative_path\":\"p\",\"declared_size\":1}\n{not-json}\n";
    let error = parse_catalog(text).unwrap_err().to_string();
    println!("bad json error={error}");
    assert!(error.contains("editor.asset_floor.catalog.json"));
    assert!(error.contains("line 2"));
}

#[test]
fn parse_catalog_blank_fields_fails_loud() {
    let text = "{\"source\":\"\",\"asset_id\":\"a\",\"asset_type\":\"model\",\"relative_path\":\"p\",\"declared_size\":1}\n";
    let error = parse_catalog(text).unwrap_err().to_string();
    assert!(error.contains("editor.asset_floor.catalog.fields"));
}

#[test]
fn unknown_role_override_fails_loud() {
    let error = parse_floor_overrides(&["bogus_role=10".to_string()])
        .unwrap_err()
        .to_string();
    println!("unknown role error={error}");
    assert!(error.contains("editor.asset_floor.override.role"));
    assert!(error.contains("bogus_role"));
}

#[test]
fn zero_and_malformed_override_fails_loud() {
    assert!(
        parse_floor_overrides(&["audio=0".to_string()])
            .unwrap_err()
            .to_string()
            .contains("override.count")
    );
    assert!(
        parse_floor_overrides(&["audio".to_string()])
            .unwrap_err()
            .to_string()
            .contains("override.form")
    );
    assert!(
        parse_floor_overrides(&["audio=1".to_string(), "audio=2".to_string()])
            .unwrap_err()
            .to_string()
            .contains("override.duplicate")
    );
}

#[test]
fn no_overrides_uses_the_shared_required_floors() {
    let floors = parse_floor_overrides(&[]).unwrap();
    // Must exactly mirror the single-source-of-truth floor table, not a local copy.
    for (role, minimum) in required_role_floors() {
        assert_eq!(floors.get(*role).copied(), Some(*minimum));
    }
    assert_eq!(floors.len(), required_role_floors().len());
}

#[test]
fn parse_catalog_roundtrips_real_shaped_line() {
    let text = "{\"source\":\"ambientcg\",\"asset_id\":\"Rock035\",\"asset_type\":\"material\",\"title\":\"Rock 035\",\"relative_path\":\"sources/acg/Rock035/Rock035_1K.jpg\",\"declared_size\":123456}";
    let entries = parse_catalog(text).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].asset_id, "Rock035");
    assert_eq!(entries[0].declared_size, 123_456);
    // asset_type material => terrain_surface via the shared classifier.
    let roles = classify_entry(&entries[0]);
    assert!(roles.iter().any(|role| role == "terrain_surface"));
}
