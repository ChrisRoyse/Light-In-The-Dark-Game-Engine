use litd_hash::sha256_hex;
use std::{fs, io, path::PathBuf};

use litd_editor::{
    asset_store::{AssetHandle, AudioAsset, ContentAddressedAssetStore, MeshAsset, TextureAsset},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};
use serde_json::json;

const MESH_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const TEXTURE_PNG: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";
const AUDIO_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_ASSET_STORE_FSV_ROOT",
        "issue1101_asset_store",
        "manual",
    )?;
    let artifact_path = root.join("asset_store_readback.json");
    let stats_path = root.join("stats.txt");

    let store = ContentAddressedAssetStore::load_default_cached()?;
    let catalog = store.catalog();
    let mesh = real_record(catalog, MESH_GLB)?;
    let texture = real_record(catalog, TEXTURE_PNG)?;
    let audio = real_record(catalog, AUDIO_OGG)?;
    let before_state = source_truth_state(catalog, &artifact_path)?;

    let mesh_handle = store.handle_for_record::<MeshAsset>(&mesh)?;
    let texture_handle = store.handle_for_record::<TextureAsset>(&texture)?;
    let audio_handle = store.handle_for_record::<AudioAsset>(&audio)?;

    let mesh_receipt = store.load(mesh_handle.clone())?;
    let texture_receipt = store.load_background(texture_handle.clone()).join()?;
    let audio_receipt = store.load_background(audio_handle.clone()).join()?;

    let after_happy_state = source_truth_state(catalog, &artifact_path)?;
    let wrong_type_edge = edge_wrong_type(catalog, &artifact_path, &store, &audio)?;
    let tampered_sha_edge = edge_tampered_sha(catalog, &artifact_path, &store, &audio)?;
    let missing_edge = edge_missing_record(catalog, &artifact_path, &store)?;

    let artifact = json!({
        "issue": 1101,
        "source_of_truth": "local-asset-vault manifests and physical files plus generated asset_store_readback.json",
        "source_before": before_state,
        "source_after_happy": after_happy_state,
        "mesh": receipt_json(&mesh, mesh_handle.handle_id(), &mesh_receipt.verified_sha256, mesh_receipt.bytes_read, mesh_receipt.loaded_on_background_thread),
        "texture": receipt_json(&texture, texture_handle.handle_id(), &texture_receipt.verified_sha256, texture_receipt.bytes_read, texture_receipt.loaded_on_background_thread),
        "audio": receipt_json(&audio, audio_handle.handle_id(), &audio_receipt.verified_sha256, audio_receipt.bytes_read, audio_receipt.loaded_on_background_thread),
        "edges": {
            "wrong_type": wrong_type_edge.to_json(),
            "tampered_sha": tampered_sha_edge.to_json(),
            "missing_record": missing_edge.to_json()
        }
    });
    write_json(&artifact_path, &artifact)?;
    let _artifact_readback = fs::read(&artifact_path)?;
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    let after_artifact_state = source_truth_state(catalog, &artifact_path)?;

    fs::write(
        &stats_path,
        format!(
            "issue=1101\n\
             source_of_truth=local-asset-vault manifests, physical asset bytes, and asset_store_readback.json\n\
             before_state={}\n\
             after_happy_state={}\n\
             after_artifact_state={}\n\
             artifact_path={}\n\
             artifact_sha256={}\n\
             mesh_handle={}\n\
             mesh_path={}\n\
             mesh_sha256={}\n\
             mesh_verified_sha256={}\n\
             mesh_bytes={}\n\
             mesh_background={}\n\
             texture_handle={}\n\
             texture_path={}\n\
             texture_sha256={}\n\
             texture_verified_sha256={}\n\
             texture_bytes={}\n\
             texture_background={}\n\
             audio_handle={}\n\
             audio_path={}\n\
             audio_sha256={}\n\
             audio_verified_sha256={}\n\
             audio_bytes={}\n\
             audio_background={}\n\
             edge_wrong_type_error={}\n\
             edge_wrong_type_before_state={}\n\
             edge_wrong_type_action={}\n\
             edge_wrong_type_after_state={}\n\
             edge_wrong_type_source_unchanged={}\n\
             edge_tampered_sha_error={}\n\
             edge_tampered_sha_before_state={}\n\
             edge_tampered_sha_action={}\n\
             edge_tampered_sha_after_state={}\n\
             edge_tampered_sha_source_unchanged={}\n\
             edge_missing_record_error={}\n\
             edge_missing_record_before_state={}\n\
             edge_missing_record_action={}\n\
             edge_missing_record_after_state={}\n\
             edge_missing_record_source_unchanged={}\n",
            before_state,
            after_happy_state,
            after_artifact_state,
            artifact_path.display(),
            report_readback.sha256,
            mesh_handle.handle_id(),
            mesh.relative_path,
            mesh.sha256,
            mesh_receipt.verified_sha256,
            mesh_receipt.bytes_read,
            mesh_receipt.loaded_on_background_thread,
            texture_handle.handle_id(),
            texture.relative_path,
            texture.sha256,
            texture_receipt.verified_sha256,
            texture_receipt.bytes_read,
            texture_receipt.loaded_on_background_thread,
            audio_handle.handle_id(),
            audio.relative_path,
            audio.sha256,
            audio_receipt.verified_sha256,
            audio_receipt.bytes_read,
            audio_receipt.loaded_on_background_thread,
            wrong_type_edge.error_code,
            wrong_type_edge.before_state,
            wrong_type_edge.action,
            wrong_type_edge.after_state,
            wrong_type_edge.source_unchanged(),
            tampered_sha_edge.error_code,
            tampered_sha_edge.before_state,
            tampered_sha_edge.action,
            tampered_sha_edge.after_state,
            tampered_sha_edge.source_unchanged(),
            missing_edge.error_code,
            missing_edge.before_state,
            missing_edge.action,
            missing_edge.after_state,
            missing_edge.source_unchanged(),
        ),
    )?;
    println!(
        "wrote issue1101 asset store FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

#[derive(Debug)]
struct EdgeAudit {
    error_code: String,
    before_state: String,
    action: String,
    after_state: String,
}

impl EdgeAudit {
    fn source_unchanged(&self) -> bool {
        self.before_state == self.after_state
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "error_code": self.error_code,
            "before_state": self.before_state,
            "action": self.action,
            "after_state": self.after_state,
            "source_unchanged": self.source_unchanged()
        })
    }
}

fn edge_wrong_type(
    catalog: &AssetVaultCatalog,
    artifact_path: &PathBuf,
    store: &ContentAddressedAssetStore,
    audio: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, artifact_path)?;
    let error_code = validation_error_code(
        store.handle_for_record::<TextureAsset>(audio).map(|_| ()),
        "audio record unexpectedly produced texture handle",
    )?;
    let after_state = source_truth_state(catalog, artifact_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("texture_handle_from_audio={}", audio.relative_path),
        after_state,
    })
}

fn edge_tampered_sha(
    catalog: &AssetVaultCatalog,
    artifact_path: &PathBuf,
    store: &ContentAddressedAssetStore,
    audio: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, artifact_path)?;
    let handle = AssetHandle::<AudioAsset>::new(
        audio.relative_path.clone(),
        "0".repeat(64),
        audio.bytes_on_disk,
    )?;
    let error_code = validation_error_code(
        store.load(handle).map(|_| ()),
        "tampered content-address handle unexpectedly loaded",
    )?;
    let after_state = source_truth_state(catalog, artifact_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("load_tampered_sha={}", audio.relative_path),
        after_state,
    })
}

fn edge_missing_record(
    catalog: &AssetVaultCatalog,
    artifact_path: &PathBuf,
    store: &ContentAddressedAssetStore,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, artifact_path)?;
    let handle = AssetHandle::<MeshAsset>::new("missing/never-there.glb", "a".repeat(64), 1)?;
    let error_code = validation_error_code(
        store.load(handle).map(|_| ()),
        "missing handle unexpectedly loaded",
    )?;
    let after_state = source_truth_state(catalog, artifact_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: "load_missing_record=missing/never-there.glb".to_string(),
        after_state,
    })
}

fn source_truth_state(
    catalog: &AssetVaultCatalog,
    artifact_path: &PathBuf,
) -> Result<String, Box<dyn std::error::Error>> {
    let mesh = real_record(catalog, MESH_GLB)?;
    let texture = real_record(catalog, TEXTURE_PNG)?;
    let audio = real_record(catalog, AUDIO_OGG)?;
    Ok(format!(
        "records={};usable={};mesh={}:{}:{};texture={}:{}:{};audio={}:{}:{};artifact={}:{}",
        catalog.record_count(),
        catalog.usable_record_count(),
        mesh.bytes_on_disk,
        mesh.sha256,
        file_sha256(&mesh.absolute_path)?,
        texture.bytes_on_disk,
        texture.sha256,
        file_sha256(&texture.absolute_path)?,
        audio.bytes_on_disk,
        audio.sha256,
        file_sha256(&audio.absolute_path)?,
        artifact_path.exists(),
        if artifact_path.exists() {
            file_sha256(artifact_path)?
        } else {
            "absent".to_string()
        }
    ))
}

fn receipt_json(
    record: &AssetVaultRecord,
    handle_id: &str,
    verified_sha256: &str,
    bytes_read: u64,
    loaded_on_background_thread: bool,
) -> serde_json::Value {
    json!({
        "handle_id": handle_id,
        "relative_path": record.relative_path,
        "manifest_sha256": record.sha256,
        "verified_sha256": verified_sha256,
        "manifest_bytes": record.bytes_on_disk,
        "bytes_read": bytes_read,
        "loaded_on_background_thread": loaded_on_background_thread
    })
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

fn validation_error_code(
    result: Result<(), litd_engine::EngineError>,
    message: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    match result {
        Ok(()) => Err(io::Error::new(io::ErrorKind::InvalidData, message).into()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn write_json(path: &PathBuf, value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&fs::read(path)?))
}
