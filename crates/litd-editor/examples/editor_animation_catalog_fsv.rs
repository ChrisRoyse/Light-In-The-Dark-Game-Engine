use std::{fs, path::PathBuf};

use litd_editor::{
    animation_catalog::{
        AnimationClipQuery, AnimationRigFilter, build_animation_clip_index_from_records,
        search_animation_clips,
    },
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};
use serde_json::json;

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_ANIMATION_CATALOG_FSV",
        "issue1235_animation_catalog",
        "fsv.json",
    )?;
    let before = path_state(&path);

    let stan = real_record(STAN_GLTF)?;
    let leela = real_record(LEELA_GLTF)?;
    let gate = real_record(GATE_GLB)?;
    let stan_index = build_animation_clip_index_from_records(std::slice::from_ref(&stan))?;
    let leela_index = build_animation_clip_index_from_records(std::slice::from_ref(&leela))?;
    let gate_index = build_animation_clip_index_from_records(std::slice::from_ref(&gate))?;
    let mixed_index = build_animation_clip_index_from_records(&[stan, leela, gate])?;
    let stan_rig = stan_index.rigs.first().expect("stan rig").clone();
    let leela_rig = leela_index.rigs.first().expect("leela rig").clone();
    let gate_rig = gate_index.rigs.first().expect("gate rig").clone();

    let happy = search_animation_clips(
        &stan_index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: stan_rig.compatibility_key.clone(),
                exact_rig_id: Some(stan_rig.rig_id.clone()),
                allow_retarget: false,
            }),
        },
    )?;
    let no_match = search_animation_clips(
        &mixed_index,
        AnimationClipQuery {
            description: "swimming crawl".to_string(),
            min_frame_count: None,
            max_frame_count: None,
            rig: None,
        },
    )?;
    let rig_mismatch = search_animation_clips(
        &stan_index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: leela_rig.compatibility_key.clone(),
                exact_rig_id: Some(leela_rig.rig_id.clone()),
                allow_retarget: false,
            }),
        },
    )?;
    let retarget = search_animation_clips(
        &stan_index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: leela_rig.compatibility_key.clone(),
                exact_rig_id: Some(leela_rig.rig_id.clone()),
                allow_retarget: true,
            }),
        },
    )?;

    let document = json!({
        "schema": "litd.issue1235.animation_catalog_fsv.v1",
        "source_of_truth": path,
        "before": before,
        "index": {
            "mixed_clip_count": mixed_index.clip_count,
            "mixed_rig_count": mixed_index.rig_count,
            "stan_clip_count": stan_index.clip_count,
            "stan_rig": stan_rig,
            "leela_rig": leela_rig,
            "gate_source": GATE_GLB,
            "gate_clip_count": gate_index.clip_count,
            "gate_rig": gate_rig,
        },
        "happy_path": summarize_result(&happy),
        "edges": [
            {
                "edge": "no_match_query",
                "before": {"clip_count": mixed_index.clip_count, "description": "swimming crawl"},
                "after": summarize_result(&no_match)
            },
            {
                "edge": "rig_mismatch_excluded",
                "before": {"source_rig": "stan", "requested_rig": "leela", "allow_retarget": false},
                "after": summarize_result(&rig_mismatch)
            },
            {
                "edge": "retargetable_same_family",
                "before": {"source_rig": "stan", "requested_rig": "leela", "allow_retarget": true},
                "after": summarize_result(&retarget)
            }
        ],
    });

    write_document_with_final_state(&path, document)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote animation catalog FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    Ok(())
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

fn summarize_result(
    result: &litd_editor::animation_catalog::AnimationClipSearchResult,
) -> serde_json::Value {
    let first = result.matches.first();
    json!({
        "match_count": result.match_count,
        "first_clip": first.map(|hit| hit.clip.name.clone()),
        "first_frame_count": first.map(|hit| hit.clip.frame_count),
        "first_duration_ms": first.map(|hit| hit.clip.duration_ms),
        "first_rig_kind": first.map(|hit| format!("{:?}", hit.clip.rig.rig_kind)),
        "first_joint_count": first.map(|hit| hit.clip.rig.joint_count),
        "first_retargetable": first.map(|hit| hit.retargetable),
        "first_matched_terms": first.map(|hit| hit.matched_terms.clone()),
    })
}

fn real_record(relative_path: &str) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    AssetVaultCatalog::load_default()?
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| format!("missing real vault record {relative_path}").into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}
