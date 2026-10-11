use super::*;

#[test]
fn default_panel_lowers_to_a_valid_material() {
    let panel = MaterialInspectorPanel::default();
    let request = panel.to_request();
    assert!(request.to_material().is_ok(), "default form must be valid");
    // No map asset ids typed yet -> no bindings.
    assert!(request.maps.is_empty());
}

#[test]
fn to_request_keeps_only_filled_maps_and_trims() {
    let mut panel = MaterialInspectorPanel {
        material_id: "  slate  ".to_string(),
        ..Default::default()
    };
    panel.maps[0].asset_id = " slate-albedo ".to_string();
    panel.maps[2].asset_id = "slate-normal".to_string();
    let request = panel.to_request();
    assert_eq!(request.material_id, "slate");
    assert_eq!(request.maps.len(), 2);
    assert_eq!(request.maps[0].slot, MaterialMapSlot::BaseColor);
    assert_eq!(request.maps[0].asset_id, "slate-albedo");
    assert_eq!(request.maps[1].slot, MaterialMapSlot::Normal);
}

#[test]
fn editing_a_factor_changes_the_lowered_fingerprint() {
    let baseline = MaterialInspectorPanel::default()
        .to_request()
        .to_material()
        .unwrap()
        .preview_fingerprint();
    let panel = MaterialInspectorPanel {
        roughness: 0.2,
        ..Default::default()
    };
    let edited = panel
        .to_request()
        .to_material()
        .unwrap()
        .preview_fingerprint();
    assert_ne!(baseline, edited);
}
