use std::{env, fs, path::Path};

use litd_editor::{
    editor_intelligence_surfaces::{
        EditorIntelligenceObservation, build_editor_intelligence_surface_deck,
        parse_editor_intelligence_surface_deck, read_editor_intelligence_surface_deck,
        write_editor_intelligence_surface_deck,
    },
    history::stable_fingerprint,
    mcp_tools::{call_engine_calyx_mcp_tool, default_structured_intelligence_mcp_surface_manifest},
};
use litd_engine::calyx_paths::canonical_gameeditor_calyx_root;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue1420Fsv {
    issue: u32,
    source_of_truth: String,
    visual_fsv_note: String,
    manifest_fingerprint: String,
    happy_path: Value,
    deck: Value,
    store_readback: Value,
    edge_cases: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, fsv_root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1420_FSV_ROOT",
        "issue1420_editor_intelligence_surfaces",
        "manual",
    )?;
    let artifact_relative =
        env::var("LITD_ISSUE1420_FSV").unwrap_or_else(|_| "manual-fsv-report.json".to_string());
    let artifact_path = fsv_run.artifact_path(&artifact_relative)?;
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let calyx_home =
        canonical_gameeditor_calyx_root()?.join("tmp/fsv/issue1420-editor-intelligence-surfaces");
    let aster_dir = fsv_root.join("editor-intelligence-aster");
    let _ = fs::remove_dir_all(&calyx_home);
    let _ = fs::remove_dir_all(&aster_dir);
    fs::create_dir_all(&calyx_home)?;
    fs::create_dir_all(&aster_dir)?;
    let _home = CalyxHomeScope::set(&calyx_home);

    let manifest = default_structured_intelligence_mcp_surface_manifest()?;
    let happy = live_structured_payloads(&fsv_root)?;
    let observations = observations(&happy)?;
    let deck = build_editor_intelligence_surface_deck(
        "issue1420-editor-intelligence-surfaces",
        happy["vault_name"].as_str().ok_or("vault name missing")?,
        "loopback Calyx MCP payloads plus editor Aster deck readback",
        &manifest,
        observations,
    )?;
    let writeback = write_editor_intelligence_surface_deck(&aster_dir, &aster_dir, &deck)?;
    let loaded = read_editor_intelligence_surface_deck(&aster_dir, &deck.deck_id)?
        .ok_or("deck missing from editor intelligence Aster store")?;
    let parsed = parse_editor_intelligence_surface_deck(&loaded.bytes)?;
    if parsed != deck || loaded.bytes != writeback.bytes {
        return Err("editor intelligence deck Aster readback mismatch".into());
    }
    let edge_cases = vec![
        edge_missing_bits(&manifest, &happy)?,
        edge_empty_neighbors(&manifest, &happy)?,
        edge_invalid_deck_id(&manifest, &happy)?,
    ];

    let fsv = Issue1420Fsv {
        issue: 1420,
        source_of_truth:
            "Calyx MCP tools/call over structured records and direct editor Aster Graph CF deck readback"
                .to_string(),
        visual_fsv_note: env::var("LITD_ISSUE1420_VISUAL_FSV_NOTE").unwrap_or_else(|_| {
            "Aster/MCP source-of-truth report; Synapse/HWND visual FSV is recorded separately in the issue closeout."
                .to_string()
        }),
        manifest_fingerprint: stable_fingerprint(&manifest)?,
        happy_path: happy,
        deck: serde_json::to_value(&deck)?,
        store_readback: json!({
            "write_sha256": writeback.sha256,
            "read_sha256": loaded.sha256,
            "bytes_match": loaded.bytes == writeback.bytes,
            "seq": loaded.seq,
            "graph_rows": loaded.graph_rows,
            "surface_count": loaded.surface_count,
            "observation_count": loaded.observation_count,
        }),
        edge_cases,
    };
    write_json(&artifact_path, &fsv)?;
    let readback = read_json::<Issue1420Fsv>(&artifact_path)?;
    if readback != fsv {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let recorded = fsv_run.record_json_file(&artifact_path)?;
    drop(_home);
    fs::remove_dir_all(&calyx_home)?;
    println!(
        "{}",
        json!({
            "issue": 1420,
            "artifact_path": artifact_path,
            "artifact_sha256": recorded.sha256,
            "surface_count": fsv.deck["surface_count"],
            "observation_count": fsv.deck["observation_count"],
            "store_bytes_match": fsv.store_readback["bytes_match"],
            "edge_case_count": fsv.edge_cases.len(),
            "artifact_readback_matches": true,
        })
    );
    Ok(())
}

fn live_structured_payloads(fsv_root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let vault_name = "issue1420_structured";
    call_ok("calyx.create_vault", json!({ "name": vault_name }))?;
    let before_panel = call_ok("calyx.list_panel", json!({ "vault": vault_name }))?;
    for slot in before_panel["slots"]
        .as_array()
        .ok_or("panel slots missing")?
    {
        if slot["state"] == "active" {
            call_ok(
                "calyx.park_lens",
                json!({ "vault": vault_name, "slot": slot["slot"] }),
            )?;
        }
    }
    call_ok(
        "calyx.add_lens",
        json!({
            "vault": vault_name,
            "name": "summary.text_embedding",
            "runtime": "algorithmic",
            "modality": "text",
        }),
    )?;
    let panel = call_ok("calyx.list_panel", json!({ "vault": vault_name }))?;
    let active_slot = active_slot(&panel)?;
    let records = (0..60)
        .map(|idx| structured_record_text(idx, idx < 30))
        .collect::<Vec<_>>();
    let ingested = call_ok(
        "calyx.ingest",
        json!({ "vault": vault_name, "batch": records }),
    )?;
    let cx_ids = ingested["results"]
        .as_array()
        .ok_or("ingest results missing")?
        .iter()
        .map(|result| {
            result["cx_id"]
                .as_str()
                .ok_or("cx_id not string")
                .map(ToString::to_string)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, cx_id) in cx_ids.iter().enumerate() {
        call_ok(
            "calyx.anchor",
            json!({
                "vault": vault_name,
                "cx_id": cx_id,
                "kind": "test_pass",
                "value": index < 30,
                "source": "issue1420",
                "confidence": 1.0,
            }),
        )?;
    }
    let first_cx = cx_ids.first().ok_or("first cx_id missing")?;
    let abundance = call_ok("calyx.abundance", json!({ "vault": vault_name }))?;
    let bits = call_ok(
        "calyx.bits",
        json!({ "vault": vault_name, "anchor": "test_pass", "explain": true }),
    )?;
    let kernel = call_ok(
        "calyx.kernel",
        json!({ "vault": vault_name, "anchor": "test_pass", "rebuild": true }),
    )?;
    let neighbors = call_ok(
        "calyx.neighbors",
        json!({ "vault": vault_name, "cx_id": first_cx, "k": 4 }),
    )?;
    let answer = call_ok(
        "calyx.kernel_answer",
        json!({ "vault": vault_name, "query": "victory profile", "anchor": "test_pass" }),
    )?;
    let calibration = fsv_root.join("guard-calibration.jsonl");
    write_calibration(&calibration, active_slot)?;
    let guard_profile = call_ok(
        "calyx.guard.calibrate",
        json!({ "vault": vault_name, "domain": "issue1420", "set": calibration, "target_far": 0.01 }),
    )?;
    let guard_check = call_ok(
        "calyx.guard.check",
        json!({ "vault": vault_name, "cx_id": first_cx }),
    )?;
    Ok(json!({
        "vault_name": vault_name,
        "active_slot": active_slot,
        "ingested": {
            "input_count": ingested["input_count"],
            "cx_count": cx_ids.len(),
            "first_cx_id": first_cx,
            "result_count": ingested["results"].as_array().map(|items| items.len()),
            "anchor_count": cx_ids.len(),
        },
        "abundance": abundance,
        "bits": bits,
        "kernel": kernel,
        "neighbors": neighbors,
        "kernel_answer": answer,
        "guard_profile": guard_profile,
        "guard_check": guard_check,
    }))
}

fn observations(
    happy: &Value,
) -> Result<Vec<EditorIntelligenceObservation>, Box<dyn std::error::Error>> {
    [
        "abundance",
        "bits",
        "neighbors",
        "kernel",
        "kernel_answer",
        "guard_check",
    ]
    .into_iter()
    .map(|key| {
        let tool = match key {
            "guard_check" => "calyx.guard.check",
            "kernel_answer" => "calyx.kernel_answer",
            other => match other {
                "abundance" => "calyx.abundance",
                "bits" => "calyx.bits",
                "neighbors" => "calyx.neighbors",
                "kernel" => "calyx.kernel",
                _ => unreachable!(),
            },
        };
        let payload = happy[key].clone();
        let source_ref = format!(
            "mcp://loopback/{tool}/payload/{}",
            stable_fingerprint(&payload)?
        );
        Ok(EditorIntelligenceObservation::new(
            tool, source_ref, payload,
        ))
    })
    .collect()
}

fn edge_missing_bits(
    manifest: &litd_editor::mcp_tools::StructuredIntelligenceMcpSurfaceManifest,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let observations = observations(happy)?
        .into_iter()
        .filter(|observation| observation.tool_name != "calyx.bits")
        .collect::<Vec<_>>();
    let error = build_editor_intelligence_surface_deck(
        "edge-missing-bits",
        "v",
        "edge",
        manifest,
        observations,
    )
    .unwrap_err();
    Ok(edge("missing_bits", error))
}

fn edge_empty_neighbors(
    manifest: &litd_editor::mcp_tools::StructuredIntelligenceMcpSurfaceManifest,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let observations = observations(happy)?
        .into_iter()
        .map(|mut observation| {
            if observation.tool_name == "calyx.neighbors" {
                observation.payload = json!({"neighbors": []});
            }
            observation
        })
        .collect::<Vec<_>>();
    let error = build_editor_intelligence_surface_deck(
        "edge-empty-neighbors",
        "v",
        "edge",
        manifest,
        observations,
    )
    .unwrap_err();
    Ok(edge("empty_neighbors", error))
}

fn edge_invalid_deck_id(
    manifest: &litd_editor::mcp_tools::StructuredIntelligenceMcpSurfaceManifest,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let error = build_editor_intelligence_surface_deck(
        "bad\nid",
        "v",
        "edge",
        manifest,
        observations(happy)?,
    )
    .unwrap_err();
    Ok(edge("invalid_deck_id", error))
}

fn edge(name: &str, error: litd_engine::EngineError) -> Value {
    match error {
        litd_engine::EngineError::UiRenderInvalidConfig { field, reason } => {
            json!({ "name": name, "field": field, "reason": reason })
        }
        other => json!({ "name": name, "unexpected_error": format!("{other:?}") }),
    }
}

fn structured_record(index: usize, passed: bool) -> Value {
    json!({
        "entity_type": "game_unit",
        "fields": {
            "unit_id": {"text": format!("unit_{index:03}")},
            "summary": {"text": if passed {
                format!("victory profile durable frontline unit {index}")
            } else {
                format!("loss profile fragile scout unit {index}")
            }},
            "hit_points": {"f64": if passed { 120.0 } else { 30.0 }},
            "attack_damage": {"f64": if passed { 16.0 } else { 2.0 }},
            "outcome.anchor_kind": {"text": "test_pass"},
            "outcome.value": {"bool": passed},
            "outcome.source": {"text": "issue1420"},
            "outcome.confidence": {"f64": 1.0},
            "outcome.observed_at": {"u64": 142000},
        }
    })
}

fn structured_record_text(index: usize, passed: bool) -> String {
    let record = structured_record(index, passed);
    serde_json::to_string(&record).expect("structured record is JSON serializable")
}

fn call_ok(tool: &str, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let response = call_engine_calyx_mcp_tool(tool, arguments)?;
    if let Some(error) = response.error {
        return Err(format!("{tool} returned error: {error:?}").into());
    }
    let result = response.result.as_ref().ok_or("tool missing result")?;
    let text = result["content"][0]["text"]
        .as_str()
        .ok_or("tool text missing")?;
    Ok(serde_json::from_str(text)?)
}

fn active_slot(panel: &Value) -> Result<u16, Box<dyn std::error::Error>> {
    panel["slots"]
        .as_array()
        .ok_or("panel slots missing")?
        .iter()
        .find(|slot| slot["state"] == "active")
        .and_then(|slot| slot["slot"].as_u64())
        .map(|slot| slot as u16)
        .ok_or_else(|| "no active slot".into())
}

fn write_calibration(path: &Path, slot: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    for _ in 0..50 {
        text.push_str(&format!(r#"{{"slot":{slot},"score":0.99,"class":"good"}}"#));
        text.push('\n');
        text.push_str(&format!(
            r#"{{"slot":{slot},"score":0.10,"class":"injection"}}"#
        ));
        text.push('\n');
    }
    fs::write(path, text)?;
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

struct CalyxHomeScope {
    old: Option<std::ffi::OsString>,
}

impl CalyxHomeScope {
    fn set(home: &Path) -> Self {
        let old = env::var_os("CALYX_HOME");
        unsafe {
            env::set_var("CALYX_HOME", home);
        }
        Self { old }
    }
}

impl Drop for CalyxHomeScope {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => env::set_var("CALYX_HOME", value),
                None => env::remove_var("CALYX_HOME"),
            }
        }
    }
}
