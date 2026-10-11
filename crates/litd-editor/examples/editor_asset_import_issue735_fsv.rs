use std::{
    fs, io,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use litd_editor::{
    asset_manifest::{
        build_editor_asset_manifest, read_editor_asset_manifest, write_editor_asset_manifest,
    },
    asset_products::{
        MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY, MODEL_SOURCE_SHA256_PROPERTY,
        assign_vault_model_to_unit, product_from_vault_record,
    },
    asset_store::{AudioAsset, ContentAddressedAssetStore, MeshAsset, TextureAsset},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    gltf_asset_import::import_gltf_asset_receipt,
    gltf_validation::validate_gltf_source,
    history::{EditorDocument, EditorObject, stable_fingerprint, to_stable_ron},
    play::{PlayCameraRig, PlaySession},
    sound_editor::sound_library_entry_from_vault_record,
};

const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const TEXTURE_PNG: &str = "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.png";
const AUDIO_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";
const KHR_BASE: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, report_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_ISSUE735_ASSET_IMPORT_FSV",
        "issue735_asset_import_manager",
        "manual-fsv.json",
    )?;
    let root = report_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue735 report path has no parent")?;
    let stats_path = root.join("stats.txt");
    let document_path = root.join("editor_document.ron");
    let manifest_path = root.join("assets").join("MANIFEST");

    let store = ContentAddressedAssetStore::load_default_cached()?;
    let catalog = store.catalog();
    let mesh_record = real_record(catalog, GATE_GLB)?;
    let texture_record = real_record(catalog, TEXTURE_PNG)?;
    let audio_record = real_record(catalog, AUDIO_OGG)?;
    let required_extension_base = real_record(catalog, KHR_BASE)?;
    let before = source_truth_state(
        catalog,
        &mesh_record,
        &texture_record,
        &audio_record,
        &document_path,
        &manifest_path,
        &report_path,
    )?;

    let mesh_product = product_from_vault_record(&mesh_record)?;
    let texture_product = product_from_vault_record(&texture_record)?;
    let audio_product = product_from_vault_record(&audio_record)?;
    let gltf_import = import_gltf_asset_receipt(&mesh_product)?;
    let texture_metadata = texture_metadata(&texture_record)?;
    let audio_entry = sound_library_entry_from_vault_record(&audio_record)?;

    let mesh_handle = store.handle_for_record::<MeshAsset>(&mesh_record)?;
    let texture_handle = store.handle_for_record::<TextureAsset>(&texture_record)?;
    let audio_handle = store.handle_for_record::<AudioAsset>(&audio_record)?;
    let mesh_load = store.load(mesh_handle.clone())?;
    let texture_load = store.load_background(texture_handle.clone()).join()?;
    let audio_load = store.load_background(audio_handle.clone()).join()?;

    let mut document = fixture_document()?;
    let assignment = assign_vault_model_to_unit(&mut document, "unit_knight", &mesh_record)?;
    document
        .asset_products
        .insert(texture_product.product_id.clone(), texture_product.clone());
    document
        .asset_products
        .insert(audio_product.product_id.clone(), audio_product.clone());
    document.validate()?;
    write_text(&document_path, &to_stable_ron(&document)?)?;
    let document_readback_text = fs::read_to_string(&document_path)?;
    let document_readback: EditorDocument = ron::from_str(&document_readback_text)?;
    if document_readback != document {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "persisted editor document readback did not match",
        )
        .into());
    }

    let manifest = build_editor_asset_manifest(&document, MAX_ASSET_BYTES)?;
    let manifest_writeback =
        write_editor_asset_manifest(&manifest_path, &manifest, MAX_MANIFEST_BYTES)?;
    let manifest_readback = read_editor_asset_manifest(&manifest_path)?;
    if manifest_readback != manifest {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "assets/MANIFEST readback did not match",
        )
        .into());
    }

    let session = PlaySession::start(&document, PlayCameraRig::TopDown)?;
    let unit = document
        .objects
        .get("unit_knight")
        .ok_or("missing unit_knight")?;
    let presentation = &session.build.presentation.instances[0].render_instance;
    let after_happy = source_truth_state(
        catalog,
        &mesh_record,
        &texture_record,
        &audio_record,
        &document_path,
        &manifest_path,
        &report_path,
    )?;

    let edge_non_glb = edge_non_glb_rejected(
        catalog,
        &texture_product,
        &document_path,
        &manifest_path,
        &report_path,
    )?;
    let edge_required_extension = edge_required_extension_rejected(
        catalog,
        &required_extension_base,
        &root,
        &document_path,
        &manifest_path,
        &report_path,
    )?;
    let edge_oversized = edge_oversized_rejected(&mesh_record)?;
    let edge_missing = edge_missing_asset_rejected(&mesh_product, &root)?;

    let report = json!({
        "schema": "litd.issue735.asset_import_manager_fsv.v1",
        "issue": 735,
        "source_of_truth": "local-asset-vault manifests and physical files; content-addressed asset store readback; persisted EditorDocument; assets/MANIFEST; PlaySession presentation",
        "before": before,
        "happy_path": {
            "gltf_import": {
                "source": gltf_import.source_relative_path,
                "sha256": gltf_import.verified_file_sha256,
                "extension": gltf_import.extension,
                "meshes": gltf_import.mesh_count,
                "primitives": gltf_import.primitive_count,
                "vertices": gltf_import.vertex_count,
                "indices": gltf_import.index_count,
                "animations": gltf_import.animation_count
            },
            "texture_import": {
                "source": texture_record.relative_path,
                "sha256": texture_record.sha256,
                "product_id": texture_product.product_id,
                "metadata": texture_metadata
            },
            "audio_import": {
                "source": audio_entry.source_path,
                "sha256": audio_entry.sha256,
                "codec": format!("{:?}", audio_entry.codec),
                "length_ms": audio_entry.length_ms,
                "sample_rate_hz": audio_entry.sample_rate_hz,
                "channels": audio_entry.channels
            },
            "content_addressed_store": {
                "mesh": load_json(mesh_handle.handle_id(), &mesh_record, &mesh_load.verified_sha256, mesh_load.bytes_read),
                "texture": load_json(texture_handle.handle_id(), &texture_record, &texture_load.verified_sha256, texture_load.bytes_read),
                "audio": load_json(audio_handle.handle_id(), &audio_record, &audio_load.verified_sha256, audio_load.bytes_read)
            },
            "unit_assignment": {
                "receipt": assignment,
                "unit_row": {
                    "object_id": unit.id,
                    "kind": unit.kind,
                    "model_product_id": unit.properties.get(MODEL_PRODUCT_ID_PROPERTY),
                    "model_source_sha256": unit.properties.get(MODEL_SOURCE_SHA256_PROPERTY),
                    "model_source_path": unit.properties.get(MODEL_SOURCE_PATH_PROPERTY)
                },
                "asset_product_present": document.asset_products.contains_key(&mesh_product.product_id),
                "document_path": document_path.display().to_string(),
                "document_readback_matches": document_readback == document,
                "document_fingerprint": stable_fingerprint(&document)?
            },
            "asset_manifest": {
                "path": manifest_path.display().to_string(),
                "writeback": manifest_writeback,
                "readback_matches": manifest_readback == manifest,
                "asset_count": manifest.asset_count,
                "entries": manifest.assets
            },
            "play_presentation": {
                "presentation_mesh_id": presentation.mesh_id,
                "expected_product_id": mesh_product.product_id,
                "mesh_matches_unit_assignment": presentation.mesh_id == mesh_product.product_id,
                "visible_instance_ids": session.build.visible_instance_ids,
                "draw_batches": session.build.draw_batches,
                "screen_rect": {
                    "min": presentation.screen_rect.min,
                    "max": presentation.screen_rect.max
                }
            }
        },
        "after_happy": after_happy,
        "edges": {
            "non_glb_rejected": edge_non_glb,
            "required_khr_extension_rejected": edge_required_extension,
            "oversized_asset_rejected": edge_oversized,
            "missing_asset_rejected": edge_missing
        }
    });
    write_json(&report_path, &report)?;
    let report_bytes = fs::read(&report_path)?;
    let parsed: Value = serde_json::from_slice(&report_bytes)?;
    if parsed["happy_path"]["play_presentation"]["mesh_matches_unit_assignment"] != true {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "play mesh mismatch").into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    write_text(
        &stats_path,
        &format!(
            "issue=735\n\
             report_path={}\n\
             report_sha256={}\n\
             document_path={}\n\
             manifest_path={}\n\
             manifest_sha256={}\n\
             manifest_fingerprint={}\n\
             manifest_asset_count={}\n\
             mesh_product_id={}\n\
             unit_model_product_id={}\n\
             presentation_mesh_id={}\n\
             play_mesh_matches_unit_assignment={}\n\
             texture_source={}\n\
             audio_source={}\n\
             edge_non_glb_error={}\n\
             edge_required_extension_error={}\n\
             edge_oversized_error={}\n\
             edge_missing_asset_error={}\n",
            report_path.display(),
            report_readback.sha256,
            document_path.display(),
            manifest_path.display(),
            manifest_writeback.sha256,
            manifest.manifest_fingerprint,
            manifest.asset_count,
            mesh_product.product_id,
            unit.properties
                .get(MODEL_PRODUCT_ID_PROPERTY)
                .cloned()
                .unwrap_or_default(),
            presentation.mesh_id,
            presentation.mesh_id == mesh_product.product_id,
            texture_record.relative_path,
            audio_record.relative_path,
            edge_code(&edge_non_glb),
            edge_code(&edge_required_extension),
            edge_code(&edge_oversized),
            edge_code(&edge_missing),
        ),
    )?;
    println!(
        "wrote issue735 asset/import FSV to {} sha256={}",
        report_path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/issue735-asset-import", "rev-issue735")?;
    document.upsert_object(
        EditorObject::new(
            "unit_knight",
            "Unit",
            None,
            Transform::from_translation(Vec3::new(4.0, 0.0, -3.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    Ok(document)
}

fn edge_non_glb_rejected(
    catalog: &AssetVaultCatalog,
    texture_product: &litd_editor::asset_products::AssetProduct,
    document_path: &Path,
    manifest_path: &Path,
    report_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = artifact_state(document_path, manifest_path, report_path)?;
    let after = error_json(
        import_gltf_asset_receipt(texture_product),
        "texture product unexpectedly imported as glTF",
    )?;
    let source_after = artifact_state(document_path, manifest_path, report_path)?;
    Ok(json!({
        "action": "import_gltf_asset_receipt(texture_png_product)",
        "catalog_records": catalog.record_count(),
        "before": before,
        "after": after,
        "source_after": source_after,
        "source_unchanged": before == source_after
    }))
}

fn edge_required_extension_rejected(
    catalog: &AssetVaultCatalog,
    base: &AssetVaultRecord,
    root: &Path,
    document_path: &Path,
    manifest_path: &Path,
    report_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = artifact_state(document_path, manifest_path, report_path)?;
    let path = root.join("required-extension.gltf");
    let text = fs::read_to_string(&base.absolute_path)?.replacen(
        "\"asset\": {",
        "\"extensionsRequired\": [\"KHR_draco_mesh_compression\"],\n  \"asset\": {",
        1,
    );
    fs::write(&path, text)?;
    let after = error_json(
        validate_gltf_source(&path, KHR_BASE),
        "required KHR extension glTF unexpectedly validated",
    )?;
    let source_after = artifact_state(document_path, manifest_path, report_path)?;
    Ok(json!({
        "action": format!("validate_gltf_source({})", path.display()),
        "catalog_records": catalog.record_count(),
        "generated_source": path.display().to_string(),
        "before": before,
        "after": after,
        "source_after": source_after,
        "source_unchanged": before == source_after
    }))
}

fn edge_oversized_rejected(
    mesh_record: &AssetVaultRecord,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut document = fixture_document()?;
    assign_vault_model_to_unit(&mut document, "unit_knight", mesh_record)?;
    let max = mesh_record.bytes_on_disk.saturating_sub(1).max(1);
    let after = error_json(
        build_editor_asset_manifest(&document, max),
        "oversized manifest unexpectedly built",
    )?;
    Ok(json!({
        "action": "build_editor_asset_manifest(max_asset_bytes=mesh_bytes_minus_one)",
        "mesh_bytes": mesh_record.bytes_on_disk,
        "max_asset_bytes": max,
        "after": after,
    }))
}

fn edge_missing_asset_rejected(
    mesh_product: &litd_editor::asset_products::AssetProduct,
    root: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut missing = mesh_product.clone();
    missing.source_absolute_path = root.join("missing.glb").display().to_string();
    let after = error_json(
        import_gltf_asset_receipt(&missing),
        "missing GLB unexpectedly imported",
    )?;
    Ok(json!({
        "action": format!("import_gltf_asset_receipt({})", missing.source_absolute_path),
        "after": after,
    }))
}

fn load_json(handle: &str, record: &AssetVaultRecord, verified_sha256: &str, bytes: u64) -> Value {
    json!({
        "handle": handle,
        "relative_path": record.relative_path,
        "manifest_sha256": record.sha256,
        "verified_sha256": verified_sha256,
        "bytes": bytes,
        "matches_manifest": record.sha256 == verified_sha256 && record.bytes_on_disk == bytes
    })
}

fn edge_code(edge: &Value) -> &str {
    edge["after"]["error_code"].as_str().unwrap_or("missing")
}

fn texture_metadata(record: &AssetVaultRecord) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(&record.absolute_path)?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != record.sha256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("texture sha mismatch for {}", record.relative_path),
        )
        .into());
    }
    let format = image::guess_format(&bytes)?;
    let image = image::load_from_memory_with_format(&bytes, format)?;
    Ok(json!({
        "format": format!("{format:?}"),
        "width": image.width(),
        "height": image.height(),
        "bytes": bytes.len(),
        "sha256": sha256
    }))
}

fn source_truth_state(
    catalog: &AssetVaultCatalog,
    mesh: &AssetVaultRecord,
    texture: &AssetVaultRecord,
    audio: &AssetVaultRecord,
    document_path: &Path,
    manifest_path: &Path,
    report_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let coverage = catalog.coverage_summary()?;
    Ok(json!({
        "catalog": {
            "manifest_paths": catalog.manifest_summary_line(),
            "records": catalog.record_count(),
            "usable_records": catalog.usable_record_count(),
            "record_types": coverage.record_types,
            "asset_types": coverage.asset_types,
            "licenses": coverage.licenses
        },
        "mesh": record_state(mesh)?,
        "texture": record_state(texture)?,
        "audio": record_state(audio)?,
        "document": path_state(document_path)?,
        "assets_manifest": path_state(manifest_path)?,
        "report": path_state(report_path)?
    }))
}

fn artifact_state(
    document_path: &Path,
    manifest_path: &Path,
    report_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(json!({
        "document": path_state(document_path)?,
        "assets_manifest": path_state(manifest_path)?,
        "report": path_state(report_path)?
    }))
}

fn record_state(record: &AssetVaultRecord) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(json!({
        "relative_path": record.relative_path,
        "absolute_path": record.absolute_path.display().to_string(),
        "source": record.source,
        "asset_id": record.asset_id,
        "asset_type": record.asset_type,
        "license": record.license,
        "bytes": record.bytes_on_disk,
        "manifest_sha256": record.sha256,
        "physical_sha256": sha256_hex(&fs::read(&record.absolute_path)?),
        "exists": record.absolute_path.is_file()
    }))
}

fn path_state(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match fs::metadata(path) {
        Ok(metadata) => json!({
            "exists": true,
            "bytes": metadata.len(),
            "sha256": sha256_hex(&fs::read(path)?)
        }),
        Err(_) => json!({"exists": false}),
    })
}

include!("editor_asset_import_issue735_fsv/support.rs");
