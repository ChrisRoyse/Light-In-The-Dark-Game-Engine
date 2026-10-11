use litd_hash::sha256_hex;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use litd_editor::{
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    texture_import::{
        TextureAtlasSettings, TextureImportManifest, TextureRole, build_texture_atlas,
        import_texture_asset, validate_ktx2_texture_record, write_texture_import_artifacts,
    },
};
use serde_json::json;

const REAL_PNG: &str = "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.png";
const REAL_JPG: &str = "sources/quaternius/game-kit/junkfood/Blender/Textures/CupcakeTexture.jpg";
const REAL_NORMAL_PNG: &str = "extracted/quaternius/manual-game-kit/universalbasecharacters/Universal_Base_Characters_Standard/Universal_Base_Characters_Standard/Base_Characters/Textures/T_Eye_Normal.png";
const REAL_AUDIO: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_TEXTURE_IMPORT_FSV_ROOT",
        "issue1100_texture_import",
        "manual",
    )?;
    let fsv_json_path = root.join("texture_import_fsv.json");
    let stats_path = root.join("stats.txt");

    let catalog = AssetVaultCatalog::load_default_cached()?;
    let png_record = real_record(&catalog, REAL_PNG)?;
    let jpg_record = real_record(&catalog, REAL_JPG)?;
    let normal_record = real_record(&catalog, REAL_NORMAL_PNG)?;
    let audio_record = real_record(&catalog, REAL_AUDIO)?;
    let before_state = source_truth_state(
        &catalog,
        [&png_record, &jpg_record, &normal_record, &audio_record],
        &fsv_json_path,
    )?;

    let png = import_texture_asset(&png_record, TextureRole::UiSprite)?;
    let jpg = import_texture_asset(&jpg_record, TextureRole::BaseColor)?;
    let normal = import_texture_asset(&normal_record, TextureRole::Normal)?;
    let textures = vec![png, jpg, normal];
    let atlas = build_texture_atlas(
        &textures,
        TextureAtlasSettings {
            tile_size: 128,
            columns: 3,
            max_size: 512,
        },
    )?;
    let artifact_receipt = write_texture_import_artifacts(&root, &textures, &atlas)?;
    let manifest_bytes = fs::read(&artifact_receipt.manifest_path)?;
    let manifest: TextureImportManifest = serde_json::from_slice(&manifest_bytes)?;
    let atlas_bytes = fs::read(&artifact_receipt.atlas_path)?;
    let after_happy_state = source_truth_state(
        &catalog,
        [&png_record, &jpg_record, &normal_record, &audio_record],
        &fsv_json_path,
    )?;

    let edge_audio = edge_audio_rejected(&catalog, &fsv_json_path, &audio_record)?;
    let edge_bad_ktx2 = edge_png_bytes_rejected_as_ktx2(&catalog, &fsv_json_path, &png_record)?;
    let edge_empty_atlas = edge_empty_atlas_rejected(
        &catalog,
        [&png_record, &jpg_record, &normal_record, &audio_record],
        &fsv_json_path,
    )?;
    let optional_ktx2 = optional_ktx2_readback(&root)?;

    let fsv = json!({
        "issue": 1100,
        "source_of_truth": "local-asset-vault physical files plus immutable texture-import FSV run artifacts read back from disk",
        "before_state": before_state,
        "after_happy_state": after_happy_state,
        "textures": manifest.textures,
        "atlas": manifest.atlas,
        "artifact_receipt": artifact_receipt,
        "manifest_readback": {
            "path": artifact_receipt.manifest_path,
            "bytes": manifest_bytes.len(),
            "sha256": sha256_hex(&manifest_bytes),
            "texture_count": manifest.textures.len(),
            "texture_files": manifest.texture_files.len()
        },
        "atlas_readback": {
            "path": artifact_receipt.atlas_path,
            "bytes": atlas_bytes.len(),
            "sha256": sha256_hex(&atlas_bytes)
        },
        "edges": {
            "audio_rejected": edge_audio.to_json(),
            "png_bytes_rejected_as_ktx2": edge_bad_ktx2.to_json(),
            "empty_atlas_rejected": edge_empty_atlas.to_json()
        },
        "optional_ktx2": optional_ktx2
    });
    fs::write(&fsv_json_path, serde_json::to_vec_pretty(&fsv)?)?;
    let fsv_bytes = fs::read(&fsv_json_path)?;
    let report_readback = fsv_run.record_json_file(&fsv_json_path)?;
    let after_artifact_state = source_truth_state(
        &catalog,
        [&png_record, &jpg_record, &normal_record, &audio_record],
        &fsv_json_path,
    )?;

    fs::write(
        &stats_path,
        format!(
            "issue=1100\n\
             source_of_truth=local-asset-vault physical texture files and generated texture import artifacts\n\
             before_state={}\n\
             after_happy_state={}\n\
             after_artifact_state={}\n\
             fsv_json_path={}\n\
             fsv_json_bytes={}\n\
             fsv_json_sha256={}\n\
             manifest_path={}\n\
             manifest_bytes={}\n\
             manifest_sha256={}\n\
             atlas_path={}\n\
             atlas_bytes={}\n\
             atlas_sha256={}\n\
             texture_count={}\n\
             texture_files={}\n\
             png_source={}\n\
             png_dims={}x{}\n\
             png_basis_bytes={}\n\
             png_bc7_bytes={}\n\
             jpg_source={}\n\
             jpg_dims={}x{}\n\
             jpg_basis_bytes={}\n\
             jpg_bc7_bytes={}\n\
             normal_source={}\n\
             normal_dims={}x{}\n\
             normal_basis_format={}\n\
             normal_basis_bytes={}\n\
             normal_bc7_bytes={}\n\
             edge_audio_error={}\n\
             edge_audio_before={}\n\
             edge_audio_after={}\n\
             edge_audio_source_unchanged={}\n\
             edge_bad_ktx2_error={}\n\
             edge_bad_ktx2_before={}\n\
             edge_bad_ktx2_after={}\n\
             edge_bad_ktx2_source_unchanged={}\n\
             edge_empty_atlas_error={}\n\
             edge_empty_atlas_before={}\n\
             edge_empty_atlas_after={}\n\
             edge_empty_atlas_source_unchanged={}\n",
            before_state,
            after_happy_state,
            after_artifact_state,
            fsv_json_path.display(),
            fsv_bytes.len(),
            report_readback.sha256,
            artifact_receipt.manifest_path,
            manifest_bytes.len(),
            sha256_hex(&manifest_bytes),
            artifact_receipt.atlas_path,
            atlas_bytes.len(),
            sha256_hex(&atlas_bytes),
            manifest.textures.len(),
            manifest.texture_files.len(),
            manifest.textures[0].source_relative_path,
            manifest.textures[0].width,
            manifest.textures[0].height,
            manifest.textures[0].basis_bytes,
            manifest.textures[0].gpu_transcode_bytes,
            manifest.textures[1].source_relative_path,
            manifest.textures[1].width,
            manifest.textures[1].height,
            manifest.textures[1].basis_bytes,
            manifest.textures[1].gpu_transcode_bytes,
            manifest.textures[2].source_relative_path,
            manifest.textures[2].width,
            manifest.textures[2].height,
            manifest.textures[2].basis_container_format,
            manifest.textures[2].basis_bytes,
            manifest.textures[2].gpu_transcode_bytes,
            edge_audio.error,
            edge_audio.before_state,
            edge_audio.after_state,
            edge_audio.source_unchanged(),
            edge_bad_ktx2.error,
            edge_bad_ktx2.before_state,
            edge_bad_ktx2.after_state,
            edge_bad_ktx2.source_unchanged(),
            edge_empty_atlas.error,
            edge_empty_atlas.before_state,
            edge_empty_atlas.after_state,
            edge_empty_atlas.source_unchanged(),
        ),
    )?;
    println!(
        "wrote issue1100 texture import FSV to {} and {}",
        fsv_json_path.display(),
        stats_path.display()
    );
    Ok(())
}

#[derive(Debug)]
struct EdgeAudit {
    error: String,
    before_state: String,
    after_state: String,
}

impl EdgeAudit {
    fn source_unchanged(&self) -> bool {
        self.before_state == self.after_state
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "error": self.error,
            "before_state": self.before_state,
            "after_state": self.after_state,
            "source_unchanged": self.source_unchanged()
        })
    }
}

fn edge_audio_rejected(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
    audio: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, [audio], artifact_path)?;
    let error = import_texture_asset(audio, TextureRole::BaseColor)
        .map(|_| ())
        .expect_err("audio record unexpectedly imported as texture")
        .to_string();
    let after_state = source_truth_state(catalog, [audio], artifact_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

fn edge_png_bytes_rejected_as_ktx2(
    catalog: &AssetVaultCatalog,
    artifact_path: &Path,
    png: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, [png], artifact_path)?;
    let mut png_as_ktx2 = png.clone();
    png_as_ktx2.relative_path =
        "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.ktx2".to_string();
    let error = validate_ktx2_texture_record(&png_as_ktx2)
        .map(|_| ())
        .expect_err("PNG bytes unexpectedly validated as KTX2")
        .to_string();
    let after_state = source_truth_state(catalog, [png], artifact_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

fn edge_empty_atlas_rejected<'a>(
    catalog: &AssetVaultCatalog,
    records: impl IntoIterator<Item = &'a AssetVaultRecord>,
    artifact_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let records = records.into_iter().collect::<Vec<_>>();
    let before_state = source_truth_state(catalog, records.iter().copied(), artifact_path)?;
    let error = build_texture_atlas(&[], TextureAtlasSettings::default())
        .map(|_| ())
        .expect_err("empty atlas unexpectedly built")
        .to_string();
    let after_state = source_truth_state(catalog, records.iter().copied(), artifact_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

fn optional_ktx2_readback(root: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let Ok(path) = env::var("LITD_TEXTURE_IMPORT_KTX2_SAMPLE") else {
        return Ok(json!({
            "status": "not_provided",
            "reason": "no real permissively licensed KTX2 sample is present in the local asset vault"
        }));
    };
    let path = PathBuf::from(path);
    let bytes = fs::read(&path)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("external.ktx2");
    let record = AssetVaultRecord {
        record_type: "external-fsv".to_string(),
        source: "khronos".to_string(),
        asset_id: "ktx2-sample".to_string(),
        asset_type: "texture".to_string(),
        title: "KTX2 FSV sample".to_string(),
        license: "Apache-2.0-compatible".to_string(),
        license_url: "https://github.com/KhronosGroup/KTX-Software/blob/main/REUSE.toml"
            .to_string(),
        source_url: "https://github.com/KhronosGroup/KTX-Software".to_string(),
        relative_path: format!("immutable-fsv-run/{file_name}"),
        absolute_path: path.clone(),
        bytes_on_disk: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        status: Some("downloaded-for-fsv".to_string()),
    };
    let receipt = validate_ktx2_texture_record(&record)?;
    let copy_path = root.join(file_name);
    fs::copy(&path, &copy_path)?;
    Ok(json!({
        "status": "validated",
        "source_path": path,
        "copied_path": copy_path,
        "receipt": receipt
    }))
}

fn source_truth_state<'a>(
    catalog: &AssetVaultCatalog,
    records: impl IntoIterator<Item = &'a AssetVaultRecord>,
    artifact_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut parts = vec![
        format!("records={}", catalog.record_count()),
        format!("usable={}", catalog.usable_record_count()),
    ];
    for record in records {
        parts.push(format!(
            "{}:{}:{}:{}",
            record.relative_path,
            record.bytes_on_disk,
            record.sha256,
            file_sha256(&record.absolute_path)?
        ));
    }
    parts.push(format!(
        "artifact={}:{}",
        artifact_path.exists(),
        if artifact_path.exists() {
            file_sha256(artifact_path)?
        } else {
            "absent".to_string()
        }
    ));
    Ok(parts.join(";"))
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

fn file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&fs::read(path)?))
}
