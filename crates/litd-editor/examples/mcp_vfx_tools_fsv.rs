use std::fs;

use litd_editor::mcp_vfx_tools::{
    ATTACH_EFFECT_TOOL, DESCRIBE_EFFECT_TOOL, READ_VFX_TOOL, SET_PARTICLE_STACK_TOOL,
    call_mcp_vfx_tool, descriptor_from_sha,
};
use litd_engine::render::vfx::{VfxEffectDescriptor, read_effect_from_content_store};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue922Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_VFX_TOOLS_FSV_ROOT",
        "issue922_mcp_vfx_tools",
        "manual",
    )?;

    let store_root = root.join("store");
    let describe_ledger = root.join("describe-ledger.json");
    let stack_ledger = root.join("stack-ledger.json");
    let attach_ledger = root.join("attach-ledger.json");
    let report_path = root.join("mcp-vfx-tools-fsv-readback.json");
    let before = state(&store_root, &describe_ledger);

    let described = call_mcp_vfx_tool(
        DESCRIBE_EFFECT_TOOL,
        json!({
            "ledger_path": describe_ledger,
            "ledger_id": "issue922-describe-ledger",
            "store_root": store_root,
            "request_id": "issue922-describe-fireball",
            "prompt": "fireball explosion",
            "seed": 922,
            "requested_by": "litd-editor-issue922-fsv"
        }),
    )?;
    let descriptor: VfxEffectDescriptor =
        serde_json::from_value(described.result["descriptor"].clone())?;
    let read = call_mcp_vfx_tool(
        READ_VFX_TOOL,
        json!({
            "store_root": store_root,
            "descriptor": descriptor
        }),
    )?;
    let effect_readback = read_effect_from_content_store(&store_root, &descriptor)?;

    let mut edited = effect_readback.effect.clone();
    edited.effect_id = "issue922.stack.fireball".to_string();
    edited.display_name = "Issue 922 Stack Fireball".to_string();
    edited.emitters[0].max_particles = 4096;
    let stacked = call_mcp_vfx_tool(
        SET_PARTICLE_STACK_TOOL,
        json!({
            "ledger_path": stack_ledger,
            "ledger_id": "issue922-stack-ledger",
            "store_root": store_root,
            "request_id": "issue922-set-stack",
            "requested_by": "litd-editor-issue922-fsv",
            "effect": edited
        }),
    )?;
    let stack_descriptor: VfxEffectDescriptor =
        serde_json::from_value(stacked.result["descriptor"].clone())?;
    let attached = call_mcp_vfx_tool(
        ATTACH_EFFECT_TOOL,
        json!({
            "ledger_path": attach_ledger,
            "ledger_id": "issue922-attach-ledger",
            "store_root": store_root,
            "request_id": "issue922-attach-ability",
            "requested_by": "litd-editor-issue922-fsv",
            "cue_id": "cue.issue922.fireball.cast",
            "source_kind": "ability",
            "source_id": "ability.issue922.fireball",
            "descriptor": stack_descriptor
        }),
    )?;
    let after = state(&store_root, &describe_ledger);
    let edges = vec![
        edge_empty_prompt(&root, &store_root)?,
        edge_unknown_tool(&store_root, &describe_ledger),
        edge_missing_store(&root)?,
    ];

    let fsv = Issue922Fsv {
        issue: 922,
        source_of_truth: json!({
            "content_store": store_root,
            "describe_ledger": describe_ledger,
            "stack_ledger": stack_ledger,
            "attach_ledger": attach_ledger
        }),
        states: json!({
            "before": before,
            "after": after
        }),
        happy_path: json!({
            "describe": described,
            "read": read,
            "set_particle_stack": stacked,
            "attach_effect": attached,
            "describe_object_sha256": effect_readback.sha256,
            "describe_object_bytes": effect_readback.bytes
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    let readback = read_json::<Issue922Fsv>(&report_path)?;
    if readback != fsv {
        let original_path = root.join("mcp-vfx-tools-fsv-original-mismatch.json");
        let readback_path = root.join("mcp-vfx-tools-fsv-readback-mismatch.json");
        write_json(&original_path, &fsv)?;
        write_json(&readback_path, &readback)?;
        return Err(format!(
            "report readback mismatch at {}; original={} readback={}",
            report_path.display(),
            original_path.display(),
            readback_path.display()
        )
        .into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue922 mcp_vfx_tools_fsv report={} report_sha256={} store_objects={} describe_ledger_sha256={} stack_ledger_sha256={} attach_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        count_store_objects(&store_root),
        sha256_file(&describe_ledger)?,
        sha256_file(&stack_ledger)?,
        sha256_file(&attach_ledger)?,
    );
    Ok(())
}

fn edge_empty_prompt(
    root: &std::path::Path,
    store_root: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-empty-ledger.json");
    let before = state(store_root, &ledger);
    let error = call_mcp_vfx_tool(
        DESCRIBE_EFFECT_TOOL,
        json!({
            "ledger_path": ledger,
            "ledger_id": "issue922-empty-ledger",
            "store_root": store_root,
            "request_id": "issue922-empty",
            "prompt": "",
            "seed": 1,
            "requested_by": "litd-editor-issue922-fsv"
        }),
    )
    .unwrap_err();
    let after = state(store_root, &ledger);
    Ok(json!({
        "name": "empty_prompt",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn edge_unknown_tool(store_root: &std::path::Path, ledger: &std::path::Path) -> Value {
    let before = state(store_root, ledger);
    let error = call_mcp_vfx_tool("litd.engine.vfx.not_registered", json!({})).unwrap_err();
    let after = state(store_root, ledger);
    json!({
        "name": "unknown_tool",
        "before": before,
        "after": after,
        "error": error.to_string()
    })
}

fn edge_missing_store(root: &std::path::Path) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-ledger.json");
    let store_root = root.join("missing-store");
    let before = state(&store_root, &ledger);
    let descriptor = descriptor_from_sha(
        "issue922.missing",
        "6fe4ee04e4c1f938504a6622ca7f2fb62de3e11da045b5b967ca1c7086d824c2",
        3731,
    )?;
    let error = call_mcp_vfx_tool(
        ATTACH_EFFECT_TOOL,
        json!({
            "ledger_path": ledger,
            "ledger_id": "issue922-missing-ledger",
            "store_root": store_root,
            "request_id": "issue922-missing",
            "requested_by": "litd-editor-issue922-fsv",
            "cue_id": "cue.issue922.missing",
            "source_kind": "ability",
            "source_id": "ability.issue922.missing",
            "descriptor": descriptor
        }),
    )
    .unwrap_err();
    let after = state(&store_root, &ledger);
    Ok(json!({
        "name": "missing_store",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn state(store_root: &std::path::Path, ledger_path: &std::path::Path) -> Value {
    json!({
        "store_objects": count_store_objects(store_root),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn count_store_objects(store_root: &std::path::Path) -> usize {
    let particles = store_root.join("particles").join("sha256");
    if !particles.exists() {
        return 0;
    }
    fs::read_dir(particles)
        .unwrap()
        .filter_map(Result::ok)
        .flat_map(|prefix| fs::read_dir(prefix.path()).unwrap().filter_map(Result::ok))
        .filter(|entry| entry.path().is_file())
        .count()
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
