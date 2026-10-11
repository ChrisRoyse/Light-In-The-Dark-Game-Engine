use std::{fs, io, path::Path};

use litd_editor::{
    asset_products::{AssetProduct, product_from_vault_record},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    gltf_asset_import::{GltfAssetImportReceipt, import_gltf_asset_receipt},
};
use serde_json::json;

const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const FOOTSTEP_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_GLTF_ASSET_IMPORT_FSV",
        "issue1099_gltf_import",
        "fsv.json",
    )?;
    let catalog = AssetVaultCatalog::load_default()?;
    let before = source_truth_state(&catalog, &path)?;

    let textured = import_gltf_asset_receipt(&real_product(&catalog, MOUNTAINSIDE_GLTF)?)?;
    let skinned = import_gltf_asset_receipt(&real_product(&catalog, STAN_GLTF)?)?;
    let glb = import_gltf_asset_receipt(&real_product(&catalog, GATE_GLB)?)?;
    let tampered_sha = edge_tampered_sha(&catalog, &path)?;
    let non_mesh = edge_non_mesh(&catalog, &path)?;
    let missing_file = edge_missing_file(&catalog, &path)?;

    let document = json!({
        "schema": "litd.issue1099.gltf_asset_import_fsv.v1",
        "source_of_truth": "local-asset-vault manifests + physical glTF/GLB/audio files + persisted FSV JSON",
        "artifact_path": path,
        "before": before,
        "happy_path": {
            "textured_gltf": summarize_receipt(&textured),
            "skinned_animated_gltf": summarize_receipt(&skinned),
            "animated_glb": summarize_receipt(&glb)
        },
        "edges": [
            tampered_sha,
            non_mesh,
            missing_file
        ],
    });

    write_document_with_final_state(&path, &catalog, document)?;
    let readback = fs::read_to_string(&path)?;
    let parsed: serde_json::Value = serde_json::from_str(&readback)?;
    if parsed["after"]["artifact"]["exists"] != true {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FSV readback missing after artifact",
        )
        .into());
    }
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote glTF asset import FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn summarize_receipt(receipt: &GltfAssetImportReceipt) -> serde_json::Value {
    json!({
        "source": receipt.source_relative_path,
        "sha256": receipt.verified_file_sha256,
        "extension": receipt.extension,
        "dependencies": receipt.dependency_count,
        "scenes": receipt.scene_count,
        "nodes": receipt.node_count,
        "meshes": receipt.mesh_count,
        "primitives": receipt.primitive_count,
        "materials": receipt.material_count,
        "textures": receipt.texture_count,
        "samplers": receipt.sampler_count,
        "images": receipt.image_count,
        "unique_material_texture_sets": receipt.unique_material_texture_set_count,
        "vertices": receipt.vertex_count,
        "indices": receipt.index_count,
        "triangles": receipt.triangle_count,
        "skins": receipt.skin_count,
        "joints": receipt.joint_count,
        "skinned_nodes": receipt.skinned_node_count,
        "skinned_vertices": receipt.skinned_vertex_count,
        "weighted_vertices": receipt.weighted_vertex_count,
        "inverse_bind_matrices": receipt.inverse_bind_matrix_count,
        "animations": receipt.animation_count,
        "animation_channels": receipt.animation_channel_count,
        "animation_samplers": receipt.animation_sampler_count,
        "morph_targets": receipt.morph_target_count,
        "morph_positions": receipt.morph_position_count,
        "first_material": receipt.materials.first(),
        "first_skin": receipt.skins.first(),
        "first_animation": receipt.animations.first(),
    })
}

fn edge_tampered_sha(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut product = real_product(catalog, GATE_GLB)?;
    let before = source_truth_state(catalog, artifact_path)?;
    let expected_sha = product.source_sha256.clone();
    product.source_sha256 = "0".repeat(64);
    let after = error_summary(
        import_gltf_asset_receipt(&product),
        "tampered sha unexpectedly imported",
    )?;
    let source_after = source_truth_state(catalog, artifact_path)?;
    Ok(json!({
        "edge": "tampered_manifest_sha",
        "before": before,
        "action": {"source": GATE_GLB, "manifest_sha": "0000000000000000000000000000000000000000000000000000000000000000"},
        "after": after,
        "source_after": source_after,
        "expected_real_sha": expected_sha,
        "source_unchanged": before == source_after
    }))
}

fn edge_non_mesh(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let product = real_product(catalog, FOOTSTEP_OGG)?;
    let before = source_truth_state(catalog, artifact_path)?;
    let after = error_summary(
        import_gltf_asset_receipt(&product),
        "non-mesh unexpectedly imported",
    )?;
    let source_after = source_truth_state(catalog, artifact_path)?;
    Ok(json!({
        "edge": "non_mesh_record",
        "before": before,
        "action": {"source": FOOTSTEP_OGG, "kind": format!("{:?}", product.product_kind)},
        "after": after,
        "source_after": source_after,
        "source_unchanged": before == source_after
    }))
}

fn edge_missing_file(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut product = real_product(catalog, GATE_GLB)?;
    let before = source_truth_state(catalog, artifact_path)?;
    product.source_absolute_path = artifact_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("missing-source.glb")
        .display()
        .to_string();
    let after = error_summary(
        import_gltf_asset_receipt(&product),
        "missing file unexpectedly imported",
    )?;
    let source_after = source_truth_state(catalog, artifact_path)?;
    Ok(json!({
        "edge": "missing_physical_file",
        "before": before,
        "action": {"source": GATE_GLB, "missing_path": product.source_absolute_path},
        "after": after,
        "source_after": source_after,
        "source_unchanged": before == source_after
    }))
}

fn error_summary(
    result: Result<GltfAssetImportReceipt, litd_engine::EngineError>,
    message: &'static str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    match result {
        Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, message).into()),
        Err(error) => Ok(json!({
            "ok": false,
            "error_code": error.code(),
            "message": error.to_string()
        })),
    }
}

fn real_product(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetProduct, Box<dyn std::error::Error>> {
    Ok(product_from_vault_record(&real_record(
        catalog,
        relative_path,
    )?)?)
}

fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("missing real vault record {relative_path}"),
            )
            .into()
        })
}

fn source_truth_state(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let coverage = catalog.coverage_summary()?;
    Ok(json!({
        "manifest_records": catalog.record_count(),
        "usable_records": catalog.usable_record_count(),
        "record_types": coverage.record_types,
        "asset_types": coverage.asset_types,
        "licenses": coverage.licenses,
        "textured": file_state(catalog, MOUNTAINSIDE_GLTF)?,
        "skinned": file_state(catalog, STAN_GLTF)?,
        "glb": file_state(catalog, GATE_GLB)?,
        "audio_edge": file_state(catalog, FOOTSTEP_OGG)?,
        "artifact": path_state(artifact_path)?,
    }))
}

fn file_state(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let record = real_record(catalog, relative_path)?;
    Ok(json!({
        "relative_path": record.relative_path,
        "bytes": record.bytes_on_disk,
        "manifest_sha256": record.sha256,
        "physical_sha256": file_sha256(&record.absolute_path)?,
        "exists": record.absolute_path.is_file()
    }))
}

fn path_state(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(match fs::metadata(path) {
        Ok(metadata) => json!({
            "exists": true,
            "bytes": metadata.len()
        }),
        Err(_) => json!({"exists": false}),
    })
}

fn write_document_with_final_state(
    path: &Path,
    catalog: &AssetVaultCatalog,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = source_truth_state(catalog, path)?;
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "FSV artifact write verification failed: embedded bytes={expected_len}, disk bytes={disk_len}"
                    ),
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "FSV artifact byte count did not stabilize",
    )
    .into())
}

fn file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
