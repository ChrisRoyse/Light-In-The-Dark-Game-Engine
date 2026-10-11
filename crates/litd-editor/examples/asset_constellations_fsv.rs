use std::{fs, path::PathBuf};

use litd_editor::{
    asset_constellations::{
        AssetConstellationIndex, AssetModality, AssetMultimodalConstellation, AssetSlotState,
        build_asset_constellation_index, find_assets_like_this,
    },
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};
use serde_json::json;

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const KAYKIT_GLB: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/gltf/floor_dirt_large_rocky.gltf.glb";
const KAYKIT_TEXTURE: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";
const FOOTSTEP_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_ASSET_CONSTELLATIONS_FSV",
        "issue1233_asset_constellations",
        "fsv.json",
    )?;
    let before = path_state(&path);

    let records = real_records(&[
        STAN_GLTF,
        LEELA_GLTF,
        KAYKIT_GLB,
        KAYKIT_TEXTURE,
        FOOTSTEP_OGG,
    ])?;
    let index = build_asset_constellation_index(&records)?;
    let hits = find_assets_like_this(&index, STAN_GLTF, 3)?;
    let audio_only = build_asset_constellation_index(&real_records(&[FOOTSTEP_OGG])?)?;
    let missing_thumbnail = build_asset_constellation_index(&real_records(&[MOUNTAINSIDE_GLTF])?)?;
    let corrupt = corrupt_mesh_edge()?;

    let document = json!({
        "schema": "litd.issue1233.asset_constellations_fsv.v1",
        "source_of_truth": path,
        "before": before,
        "happy_path": {
            "before": {"records": [STAN_GLTF, LEELA_GLTF, KAYKIT_GLB, KAYKIT_TEXTURE, FOOTSTEP_OGG], "query": STAN_GLTF},
            "after": {
                "index": summarize_index(&index),
                "kaykit_slots": summarize_asset(&index, KAYKIT_GLB),
                "texture_slots": summarize_asset(&index, KAYKIT_TEXTURE),
                "audio_slots": summarize_asset(&index, FOOTSTEP_OGG),
                "top_neighbor": hits.first().map(|hit| hit.relative_path.clone()),
                "top_neighbor_score": hits.first().map(|hit| hit.score),
                "hits": hits
            }
        },
        "edges": [
            {
                "edge": "audio_only_asset",
                "before": {"records": [FOOTSTEP_OGG]},
                "after": summarize_asset(&audio_only, FOOTSTEP_OGG)
            },
            corrupt,
            {
                "edge": "missing_thumbnail",
                "before": {"records": [MOUNTAINSIDE_GLTF]},
                "after": summarize_asset(&missing_thumbnail, MOUNTAINSIDE_GLTF)
            }
        ],
    });

    write_document_with_final_state(&path, document)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote asset constellation FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn summarize_index(index: &AssetConstellationIndex) -> serde_json::Value {
    json!({
        "constellation_count": index.constellation_count,
        "text_count": index.modality_counts.get(&AssetModality::Text).copied().unwrap_or(0),
        "image_count": index.modality_counts.get(&AssetModality::Image).copied().unwrap_or(0),
        "document_image_count": index.modality_counts.get(&AssetModality::DocumentImage).copied().unwrap_or(0),
        "audio_count": index.modality_counts.get(&AssetModality::Audio).copied().unwrap_or(0),
        "mesh_structured_count": index.modality_counts.get(&AssetModality::MeshStructured).copied().unwrap_or(0),
    })
}

fn summarize_asset(index: &AssetConstellationIndex, path: &str) -> serde_json::Value {
    let asset = asset(index, path);
    json!({
        "relative_path": asset.relative_path,
        "roles": asset.roles,
        "tags": asset.tags,
        "text": slot_state(asset, AssetModality::Text),
        "image": slot_state(asset, AssetModality::Image),
        "document_image": slot_state(asset, AssetModality::DocumentImage),
        "audio": slot_state(asset, AssetModality::Audio),
        "mesh_structured": slot_state(asset, AssetModality::MeshStructured),
        "slot_evidence": asset.slots.iter().map(|slot| json!({
            "modality": format!("{:?}", slot.modality),
            "state": format!("{:?}", slot.state),
            "source_path": slot.source_path,
            "evidence": slot.evidence
        })).collect::<Vec<_>>()
    })
}

fn slot_state(asset: &AssetMultimodalConstellation, modality: AssetModality) -> String {
    asset
        .slot(modality)
        .map(|slot| format!("{:?}", slot.state))
        .unwrap_or_else(|| format!("{:?}", AssetSlotState::Missing))
}

fn asset<'a>(index: &'a AssetConstellationIndex, path: &str) -> &'a AssetMultimodalConstellation {
    index
        .constellations
        .iter()
        .find(|asset| asset.relative_path == path)
        .unwrap_or_else(|| panic!("missing asset {path}"))
}

fn corrupt_mesh_edge() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = real_record(GATE_GLB)?;
    // #1535: TestTempDir auto-cleans on Drop (guard lives to end of this fn, which owns the dir).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1233-fsv-corrupt-mesh");
    let dir = _temp_root.path().to_path_buf();
    fs::create_dir_all(&dir)?;
    let corrupt_path = dir.join("gate.glb");
    fs::write(&corrupt_path, vec![b'{'; record.bytes_on_disk as usize])?;
    let before = json!({
        "source_record": GATE_GLB,
        "corrupt_path": corrupt_path,
        "corrupt_exists": corrupt_path.exists(),
        "corrupt_bytes": fs::metadata(&corrupt_path)?.len()
    });
    record.absolute_path = corrupt_path;
    let after = match build_asset_constellation_index(&[record]) {
        Ok(index) => {
            json!({"unexpected_ok": true, "constellation_count": index.constellation_count})
        }
        Err(error) => {
            json!({"unexpected_ok": false, "error_code": error.code(), "message": error.to_string()})
        }
    };
    Ok(json!({"edge": "corrupt_mesh_rejected", "before": before, "after": after}))
}

fn real_records(paths: &[&str]) -> Result<Vec<AssetVaultRecord>, Box<dyn std::error::Error>> {
    let catalog = AssetVaultCatalog::load_default()?;
    paths
        .iter()
        .map(|path| {
            catalog
                .records()
                .iter()
                .find(|record| record.relative_path == *path)
                .cloned()
                .ok_or_else(|| format!("missing real vault record {path}").into())
        })
        .collect()
}

fn real_record(path: &str) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    let records = real_records(&[path])?;
    records
        .into_iter()
        .next()
        .ok_or_else(|| format!("missing real vault record {path}").into())
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }

    Err(format!(
        "FSV artifact byte-count stabilization failed for {} after 16 attempts",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}
