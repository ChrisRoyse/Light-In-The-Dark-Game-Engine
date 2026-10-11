use std::{io, path::Path};

use litd_engine::{
    EngineError,
    animation::{
        AnimationSampleRequest, build_skinning_palette_upload, load_gltf_skeletal_animation_asset,
        sample_skeletal_animation,
    },
    fsv_artifacts::begin_issue_run,
    render::{
        shader::ShaderSystemSettings,
        skinning::{SKINNING_VERTEX_ATTRIBUTES, SKINNING_WGSL, build_skinning_shader_plan},
    },
};
use serde_json::{Value, json};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1105_skeletal_animation")?;
    let stan_path = workspace_asset_path(STAN_GLTF);
    let gate_path = workspace_asset_path(GATE_GLB);
    let asset = load_gltf_skeletal_animation_asset(&stan_path, STAN_GLTF)?;

    let start = build_skinning_palette_upload(&asset, &request(0))?;
    let mid = build_skinning_palette_upload(&asset, &request(375))?;
    let pose = sample_skeletal_animation(&asset, request(375))?;
    let shader = build_skinning_shader_plan(ShaderSystemSettings::default())?;
    let edges = edge_readbacks(&asset, &gate_path);

    let asset_hash = file_sha256(&stan_path)?;
    let gate_hash = file_sha256(&gate_path)?;
    let shader_hash = litd_hash::sha256_hex(SKINNING_WGSL.as_bytes());
    let start_hash = start.palette_hash();
    let mid_hash = mid.palette_hash();
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let asset_acceptance = asset.schema == "litd.skeletal_animation_asset.v1"
        && asset.source_relative_path == STAN_GLTF
        && asset.clip_count == 18
        && asset.skin_count == 1
        && asset.skins[0].joint_node_indices.len() == 43
        && asset
            .clips
            .iter()
            .any(|clip| clip.name == "SwordSlash" && clip.duration_ms == 750);
    let palette_acceptance = start.schema == "litd.skinning_palette_upload.v1"
        && mid.schema == "litd.skinning_palette_upload.v1"
        && mid.joint_count == 43
        && mid.byte_count == 43 * 64
        && mid.bind_group == 3
        && mid.binding == 0
        && mid.shader_id == "litd.skinning.vertex"
        && mid.matrices.iter().all(|matrix| matrix.is_finite())
        && start_hash != mid_hash
        && pose.joint_matrices.len() == mid.joint_count;
    let shader_acceptance = shader.id == "litd.skinning.vertex"
        && shader.bindings.len() == 1
        && shader.bindings[0].group == 3
        && shader.bindings[0].binding == 0
        && SKINNING_VERTEX_ATTRIBUTES[0].shader_location == 10
        && SKINNING_VERTEX_ATTRIBUTES[1].shader_location == 11
        && SKINNING_WGSL.contains("var<storage, read> litd_joint_palette");

    ensure(asset_acceptance, "skeletal asset acceptance failed")?;
    ensure(palette_acceptance, "skinning palette acceptance failed")?;
    ensure(shader_acceptance, "skinning shader acceptance failed")?;
    ensure(
        all_edges_matched,
        "skeletal animation edge acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1105.skeletal_animation_fsv.v1",
        "schema_version": 1,
        "issue": 1105,
        "research": [
            {
                "source": "Khronos glTF 2.0 specification",
                "url": "https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html",
                "applied": "skins provide ordered joints and inverse bind matrices; animations provide sampler channels for TRS properties"
            },
            {
                "source": "W3C WebGPU Shading Language specification",
                "url": "https://www.w3.org/TR/WGSL/",
                "applied": "vertex shader reads the joint palette through a WGSL read-only storage buffer"
            }
        ],
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "stan_gltf": stan_path.display().to_string(),
            "stan_gltf_sha256": asset_hash,
            "gate_glb": gate_path.display().to_string(),
            "gate_glb_sha256": gate_hash,
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, real glTF/GLB asset hashes, palette bytes/hashes, shader source hash, and edge errors"
        },
        "happy_path": {
            "asset": {
                "schema": asset.schema,
                "source_relative_path": asset.source_relative_path,
                "node_count": asset.node_count,
                "skin_count": asset.skin_count,
                "clip_count": asset.clip_count,
                "skin_joint_count": asset.skins[0].joint_node_indices.len(),
                "skin_name": asset.skins[0].name,
                "clip_names": asset.clips.iter().map(|clip| clip.name.clone()).collect::<Vec<_>>()
            },
            "start_palette": {
                "clip": start.clip_name,
                "playback_time_ms": start.playback_time_ms,
                "joint_count": start.joint_count,
                "byte_count": start.byte_count,
                "bind_group": start.bind_group,
                "binding": start.binding,
                "palette_hash": start_hash,
                "palette_bytes_sha256": litd_hash::sha256_hex(start.bytes())
            },
            "mid_palette": {
                "clip": mid.clip_name,
                "playback_time_ms": mid.playback_time_ms,
                "joint_count": mid.joint_count,
                "byte_count": mid.byte_count,
                "bind_group": mid.bind_group,
                "binding": mid.binding,
                "palette_hash": mid_hash,
                "palette_bytes_sha256": litd_hash::sha256_hex(mid.bytes())
            },
            "pose": {
                "clip": pose.clip_name,
                "playback_time_ms": pose.playback_time_ms,
                "node_local_transforms": pose.node_local_transforms.len(),
                "node_global_matrices": pose.node_global_matrices.len(),
                "joint_matrices": pose.joint_matrices.len()
            },
            "shader": {
                "id": shader.id,
                "source_hash": shader.source_hash,
                "wgsl_sha256": shader_hash,
                "bindings": shader.bindings,
                "joint_attribute_location": SKINNING_VERTEX_ATTRIBUTES[0].shader_location,
                "weight_attribute_location": SKINNING_VERTEX_ATTRIBUTES[1].shader_location
            }
        },
        "acceptance": {
            "asset_acceptance": asset_acceptance,
            "palette_acceptance": palette_acceptance,
            "shader_acceptance": shader_acceptance,
            "all_edges_matched": all_edges_matched,
            "clip_count": 18,
            "skin_count": 1,
            "joint_count": 43,
            "palette_byte_count": 2752,
            "start_palette_hash": start_hash,
            "mid_palette_hash": mid_hash,
            "shader_id": "litd.skinning.vertex",
            "bind_group": 3,
            "binding": 0,
            "edge_count": edges.len()
        },
        "edges": edges,
        "sufficiency_argument": "The Stan glTF has one real 43-joint skinned biped and 18 clips, so SwordSlash at 0ms and 375ms proves clip sampling changes the GPU joint palette while preserving finite joint matrices and the binding contract. The edge set covers missing clips, out-of-range playback, invalid skin index, unskinned GLB rejection, and wrong source extension; additional clips would duplicate the same TRS/palette path."
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE1105_SKELETAL_ANIMATION_FSV report={} sha256={} manifest={} run_id={} clips={} joints={} bytes={} start_hash={} mid_hash={} shader_hash={} edges={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.latest_manifest_path.display(),
        run.run_id(),
        18,
        mid.joint_count,
        mid.byte_count,
        start_hash,
        mid_hash,
        shader_hash,
        edges.len()
    );
    Ok(())
}

fn edge_readbacks(
    asset: &litd_engine::animation::SkeletalAnimationAsset,
    gate_path: &Path,
) -> Vec<Value> {
    vec![
        expect_invalid_input(
            "unknown_clip_rejected",
            sample_skeletal_animation(
                asset,
                AnimationSampleRequest {
                    clip_name: "missing_attack".to_string(),
                    skin_index: 0,
                    playback_time_ms: 0,
                },
            ),
            "animation.clip_name",
        ),
        expect_invalid_input(
            "out_of_range_time_rejected",
            sample_skeletal_animation(
                asset,
                AnimationSampleRequest {
                    clip_name: "SwordSlash".to_string(),
                    skin_index: 0,
                    playback_time_ms: 751,
                },
            ),
            "animation.playback_time_ms",
        ),
        expect_invalid_input(
            "bad_skin_index_rejected",
            sample_skeletal_animation(
                asset,
                AnimationSampleRequest {
                    clip_name: "SwordSlash".to_string(),
                    skin_index: 99,
                    playback_time_ms: 0,
                },
            ),
            "animation.skin_index",
        ),
        expect_invalid_input(
            "unskinned_gltf_rejected",
            load_gltf_skeletal_animation_asset(gate_path, GATE_GLB),
            "animation.skin_count",
        ),
        expect_invalid_input(
            "wrong_extension_rejected",
            load_gltf_skeletal_animation_asset(
                Path::new("not-an-animation.txt"),
                "not-an-animation.txt",
            ),
            "animation.source.extension",
        ),
    ]
}

fn request(playback_time_ms: u32) -> AnimationSampleRequest {
    AnimationSampleRequest {
        clip_name: "SwordSlash".to_string(),
        skin_index: 0,
        playback_time_ms,
    }
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineError>,
    expected_field: &str,
) -> Value {
    match result {
        Err(EngineError::AnimationInvalidConfig { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field,
            "field": field,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

// #1543: RUNTIME vault resolution; see `litd_engine::asset_vault_root`.
fn workspace_asset_path(relative_path: &str) -> std::path::PathBuf {
    litd_engine::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}

fn file_sha256(path: &Path) -> io::Result<String> {
    Ok(litd_hash::sha256_hex(&std::fs::read(path)?))
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
