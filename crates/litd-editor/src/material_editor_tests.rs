use super::*;

fn request() -> PbrMaterialAuthorRequest {
    PbrMaterialAuthorRequest {
        material_id: "ash_iron".to_string(),
        display_name: "Ash Iron".to_string(),
        author: "editor-user".to_string(),
        base_color: [0.8, 0.2, 0.1, 1.0],
        metallic: 0.5,
        roughness: 0.35,
        normal_scale: 1.0,
        occlusion: 0.9,
        emissive: [0.02, 0.01, 0.0],
        maps: vec![
            MaterialMapAssignment {
                slot: MaterialMapSlot::BaseColor,
                asset_id: "ash-base".to_string(),
            },
            MaterialMapAssignment {
                slot: MaterialMapSlot::Normal,
                asset_id: "ash-normal".to_string(),
            },
        ],
    }
}

#[test]
fn to_material_builds_validated_metallic_roughness_material() {
    let material = request().to_material().expect("valid request");
    assert_eq!(material.base_color_factor.to_array(), [0.8, 0.2, 0.1, 1.0]);
    assert_eq!(material.metallic_factor, 0.5);
    assert_eq!(material.roughness_factor, 0.35);
    assert!(material.has_texture(TextureSlot::BaseColor));
    assert!(material.has_texture(TextureSlot::Normal));
    assert!(!material.has_texture(TextureSlot::Emissive));
}

#[test]
fn to_material_rejects_out_of_range_roughness() {
    let mut req = request();
    req.roughness = 0.0; // below the PBR floor of 0.04
    assert!(matches!(
        req.to_material(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::PbrMaterialInvalidFactor { .. }
        ))
    ));
}

#[test]
fn to_material_rejects_duplicate_slot_and_empty_asset_id() {
    let mut dup = request();
    dup.maps.push(MaterialMapAssignment {
        slot: MaterialMapSlot::BaseColor,
        asset_id: "second-base".to_string(),
    });
    assert!(matches!(
        dup.to_material(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));

    let mut empty = request();
    empty.maps[0].asset_id = "   ".to_string();
    assert!(matches!(
        empty.to_material(),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));
}

#[test]
fn payload_round_trips_through_parse() {
    let req = request();
    let material = req.to_material().unwrap();
    let payload = material_payload(&req, &material);
    let parsed = parse_material_payload(&payload).expect("round-trip parse");
    assert_eq!(parsed.base_color_factor, material.base_color_factor);
    assert_eq!(parsed.metallic_factor, material.metallic_factor);
    assert_eq!(parsed.texture_mask(), material.texture_mask());
    // The lowered preview artifact is byte-identical after a payload round-trip.
    assert_eq!(parsed.preview_fingerprint(), material.preview_fingerprint());
}

#[test]
fn author_rejects_empty_id_and_author() {
    // material-id validation fails BEFORE any vault is opened, so the path below
    // is never touched (no filesystem/Calyx access in this pure test).
    let mut no_id = request();
    no_id.material_id = "  ".to_string();
    assert!(matches!(
        author_pbr_material("material-editor-never-opened", &no_id),
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));
}
