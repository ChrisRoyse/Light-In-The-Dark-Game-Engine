use litd_hash::sha256_hex;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use litd_editor::{
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    texture_import::{Ktx2Rgba8EncodeReceipt, Ktx2TextureReceipt, validate_ktx2_texture_record},
};
use serde_json::{Value, json};

pub(super) const SOURCE_PNG: &str = "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.png";
pub(super) const FIXTURE_RELATIVE_PATH: &str =
    "sources/generated/ktx2-fixtures/tiny-dungeon-tile-0000-rgba8.ktx2";

pub(super) struct EdgeAudit {
    pub(super) error: String,
    pub(super) before_state: String,
    pub(super) after_state: String,
}

impl EdgeAudit {
    pub(super) fn to_json(&self) -> Value {
        json!({
            "error": self.error,
            "before_state": self.before_state,
            "after_state": self.after_state,
            "source_unchanged": self.before_state == self.after_state
        })
    }
}

pub(super) fn write_fixture_file(
    path: &Path,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("fixture path has no parent: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent)?;
    fs::write(path, bytes)?;
    Ok(())
}

pub(super) fn upsert_manifest_record(
    manifest_path: &Path,
    fixture_path: &Path,
    source_record: &AssetVaultRecord,
    fixture_bytes: u64,
    fixture_sha256: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before_text = fs::read_to_string(manifest_path)?;
    let before_sha256 = sha256_hex(before_text.as_bytes());
    let record = json!({
        "absolute_path": fixture_path.display().to_string(),
        "api_url": "generated-from-local-vault",
        "asset_id": "tiny-dungeon-ktx2-fixture",
        "asset_type": "texture",
        "bundle_key": "generated-ktx2-fixtures:tiny-dungeon:tile_0000:rgba8-ktx2",
        "bytes_on_disk": fixture_bytes,
        "declared_size": fixture_bytes,
        "download_url": source_record.absolute_path.display().to_string(),
        "extraction_key": null,
        "generated_at_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "generated_by": "litd-editor/examples/ktx2_fixture_fsv.rs",
        "generated_from_sha256": source_record.sha256,
        "generated_from_relative_path": source_record.relative_path,
        "license": source_record.license,
        "license_url": source_record.license_url,
        "md5_actual": null,
        "md5_expected": null,
        "record_type": "download",
        "relative_path": FIXTURE_RELATIVE_PATH,
        "sha256": fixture_sha256,
        "source": "generated-ktx2-fixtures",
        "source_url": source_record.source_url,
        "status": "existing_verified",
        "title": "Tiny Dungeon tile_0000 RGBA8 KTX2 fixture"
    });
    let record_line = serde_json::to_string(&record)?;
    let mut found = 0_usize;
    let mut output = Vec::new();
    for line in before_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)?;
        if value
            .get("relative_path")
            .and_then(Value::as_str)
            .is_some_and(|relative_path| relative_path == FIXTURE_RELATIVE_PATH)
        {
            found += 1;
            output.push(record_line.clone());
        } else {
            output.push(line.to_string());
        }
    }
    if found > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("manifest contains {found} duplicate KTX2 fixture records"),
        )
        .into());
    }
    let action = if found == 0 { "inserted" } else { "replaced" };
    if found == 0 {
        output.push(record_line);
    }
    let after_text = format!("{}\n", output.join("\n"));
    fs::write(manifest_path, after_text.as_bytes())?;
    let disk_text = fs::read_to_string(manifest_path)?;
    if disk_text != after_text {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "manifest readback does not match intended upsert text",
        )
        .into());
    }
    Ok(json!({
        "action": action,
        "manifest_path": manifest_path,
        "before_bytes": before_text.len(),
        "after_bytes": disk_text.len(),
        "before_sha256": before_sha256,
        "after_sha256": sha256_hex(disk_text.as_bytes()),
        "fixture_relative_path": FIXTURE_RELATIVE_PATH
    }))
}

pub(super) fn validate_fixture_receipt(
    parsed: &Ktx2TextureReceipt,
    generated: &Ktx2Rgba8EncodeReceipt,
) -> Result<(), Box<dyn std::error::Error>> {
    if parsed.width != generated.width
        || parsed.height != generated.height
        || parsed.vk_format != generated.vk_format
        || parsed.level_bytes != generated.level_byte_length
        || parsed.requires_basis_transcode
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("KTX2 fixture parsed receipt did not match generated receipt: parsed={parsed:?} generated={generated:?}"),
        )
        .into());
    }
    Ok(())
}

pub(super) fn edge_truncated_ktx2_rejected(
    root: &Path,
    valid_record: &AssetVaultRecord,
    fixture_path: &Path,
    manifest_path: &Path,
    fsv_json_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    let corrupted_path = root.join("truncated-fixture.ktx2");
    let source_bytes = fs::read(&valid_record.absolute_path)?;
    let truncated_len = HeaderLength::minimum().min(source_bytes.len());
    fs::write(&corrupted_path, &source_bytes[..truncated_len])?;
    let corrupted_bytes = fs::read(&corrupted_path)?;
    let corrupted_relative_path = corrupted_path.display().to_string();
    let corrupted = edge_record(
        valid_record,
        &corrupted_relative_path,
        corrupted_path,
        corrupted_bytes.len() as u64,
        sha256_hex(&corrupted_bytes),
    );
    let error = validate_ktx2_texture_record(&corrupted)
        .map(|_| ())
        .expect_err("truncated KTX2 unexpectedly validated")
        .to_string();
    let after_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

pub(super) fn edge_wrong_extension_rejected(
    valid_record: &AssetVaultRecord,
    fixture_path: &Path,
    manifest_path: &Path,
    fsv_json_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    let mut wrong_extension = valid_record.clone();
    wrong_extension.relative_path =
        "sources/generated/ktx2-fixtures/tiny-dungeon-tile-0000-rgba8.png".to_string();
    let error = validate_ktx2_texture_record(&wrong_extension)
        .map(|_| ())
        .expect_err("KTX2 bytes with wrong extension unexpectedly validated")
        .to_string();
    let after_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

pub(super) fn edge_manifest_sha_mismatch_rejected(
    valid_record: &AssetVaultRecord,
    fixture_path: &Path,
    manifest_path: &Path,
    fsv_json_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    let mut wrong_sha = valid_record.clone();
    wrong_sha.sha256 = "0".repeat(64);
    let error = validate_ktx2_texture_record(&wrong_sha)
        .map(|_| ())
        .expect_err("KTX2 record with wrong manifest sha unexpectedly validated")
        .to_string();
    let after_state = source_truth_state(fixture_path, manifest_path, fsv_json_path)?;
    Ok(EdgeAudit {
        error,
        before_state,
        after_state,
    })
}

struct HeaderLength;

impl HeaderLength {
    fn minimum() -> usize {
        64
    }
}

fn edge_record(
    valid_record: &AssetVaultRecord,
    relative_path: &str,
    absolute_path: PathBuf,
    bytes_on_disk: u64,
    sha256: String,
) -> AssetVaultRecord {
    AssetVaultRecord {
        record_type: valid_record.record_type.clone(),
        source: valid_record.source.clone(),
        asset_id: valid_record.asset_id.clone(),
        asset_type: valid_record.asset_type.clone(),
        title: valid_record.title.clone(),
        license: valid_record.license.clone(),
        license_url: valid_record.license_url.clone(),
        source_url: valid_record.source_url.clone(),
        relative_path: relative_path.to_string(),
        absolute_path,
        bytes_on_disk,
        sha256,
        status: valid_record.status.clone(),
    }
}

pub(super) fn source_truth_state(
    fixture_path: &Path,
    manifest_path: &Path,
    fsv_json_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let fixture_state = if fixture_path.exists() {
        let metadata = fs::metadata(fixture_path)?;
        format!(
            "fixture=true:{}:{}",
            metadata.len(),
            file_sha256(fixture_path)?
        )
    } else {
        "fixture=false:0:absent".to_string()
    };
    let manifest_text = fs::read_to_string(manifest_path)?;
    let manifest_record_count = manifest_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let manifest_has_fixture = manifest_text.contains(FIXTURE_RELATIVE_PATH);
    let fsv_state = if fsv_json_path.exists() {
        format!("fsv=true:{}", file_sha256(fsv_json_path)?)
    } else {
        "fsv=false:absent".to_string()
    };
    Ok(format!(
        "{fixture_state};manifest=true:{}:{}:{};{}",
        manifest_record_count,
        sha256_hex(manifest_text.as_bytes()),
        manifest_has_fixture,
        fsv_state
    ))
}

pub(super) fn record_json(record: &AssetVaultRecord) -> Value {
    json!({
        "record_type": record.record_type,
        "source": record.source,
        "asset_id": record.asset_id,
        "asset_type": record.asset_type,
        "title": record.title,
        "license": record.license,
        "license_url": record.license_url,
        "source_url": record.source_url,
        "relative_path": record.relative_path,
        "absolute_path": record.absolute_path,
        "bytes_on_disk": record.bytes_on_disk,
        "sha256": record.sha256,
        "status": record.status
    })
}

pub(super) fn real_record(
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

// #1543: RUNTIME vault resolution; see `litd_engine::asset_vault_root`.
pub(super) fn vault_root() -> PathBuf {
    litd_engine::asset_vault_root::local_asset_vault_root().unwrap()
}
