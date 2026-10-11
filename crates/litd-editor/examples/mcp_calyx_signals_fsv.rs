use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::mcp_harness_tools::{
    CALYX_SIGNALS_TOOL, READ_HARNESS_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
    read_mcp_harness_ledger, read_mcp_harness_state,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let root = Path::new("target")
        .join("fsv")
        .join("issue1176_calyx_signals")
        .join("manual");
    clean_dir(&root)?;
    let state_path = root.join("harness-state.json");
    let ledger_path = root.join("signals-ledger.json");
    let aster_dir = root.join("aster");
    let before = state_summary(&state_path, &aster_dir);
    println!("manual issue1176 before {}", before);

    let result = call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "signals-ledger",
            json!({
                "signal_batch_id": "issue1176-manual-signals",
                "authored_content": [
                    authored_content("happy", "similar", 40, 8, 20, "poison", 700),
                    authored_content("similar", "happy", 42, 9, 22, "poison", 700),
                    authored_content("imbalanced", "happy", 999, 8, 20, "poison", 700),
                    authored_content("ood", "happy", 40, 8, 20, "poison", 700),
                    authored_content("insufficient", "happy", 40, 8, 20, "poison", 2_000)
                ]
            }),
        ),
    )?;

    let read_harness = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({"state_path": state_path}))?;
    let state_readback = read_mcp_harness_state(&state_path)?;
    let ledger = read_mcp_harness_ledger(&ledger_path)?;
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1176-signals-ledger")?
        .ok_or("missing issue1176 Aster ledger row")?;
    let state_bytes = fs::read(&state_path)?;
    if ledger.ledger.records[0].state_sha256 != sha256_hex(&state_bytes) {
        return Err("ledger state SHA did not match physical state file".into());
    }
    if aster.sha256 != ledger.sha256 {
        return Err("Aster row SHA did not match physical ledger file".into());
    }

    let empty_edge = fail_closed_edge(&root, &state_path, &aster_dir, "empty", json!({}))?;
    let mut bad_panel = authored_content("bad-panel", "bad-panel", 40, 8, 20, "poison", 700);
    bad_panel["panel"]["content_lens_count"] = json!(9);
    bad_panel["panel"]["lens_names"] = json!(["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
    let bad_panel_edge = fail_closed_edge(
        &root,
        &state_path,
        &aster_dir,
        "bad-panel",
        json!({"authored_content": [bad_panel]}),
    )?;
    let after = state_summary(&state_path, &aster_dir);
    println!("manual issue1176 after {}", after);

    let report = json!({
        "issue": 1176,
        "source_of_truth": "harness-state.json + signals-ledger.json + Aster agent_harness ledger row",
        "before": before,
        "after": after,
        "tool_result": result.result,
        "read_harness": read_harness.result,
        "state_readback": {
            "path": state_readback.path,
            "sha256": state_readback.sha256,
            "signal_batches": state_readback.state.calyx_signals.len(),
            "events": state_readback.event_count
        },
        "ledger": {
            "path": ledger.path,
            "sha256": ledger.sha256,
            "state_sha256": ledger.ledger.records[0].state_sha256,
            "affected_ids": ledger.ledger.records[0].event.affected_ids
        },
        "aster": aster,
        "expected_codes": {
            "happy": codes(&result.result["calyx_signals"], "happy"),
            "imbalanced": codes(&result.result["calyx_signals"], "imbalanced"),
            "ood": codes(&result.result["calyx_signals"], "ood"),
            "insufficient": codes(&result.result["calyx_signals"], "insufficient")
        },
        "fail_closed_edges": [empty_edge, bad_panel_edge]
    });
    let report_path = root.join("manual-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_bytes = fs::metadata(&report_path)?.len();
    println!(
        "manual issue1176 report={} bytes={} state_sha={} ledger_sha={} aster_sha={}",
        report_path.display(),
        report_bytes,
        sha256_hex(&state_bytes),
        ledger.sha256,
        aster.sha256
    );
    Ok(())
}

fn fail_closed_edge(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    name: &str,
    mut payload: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state_summary(state_path, aster_dir);
    payload["signal_batch_id"] = json!(format!("issue1176-{name}"));
    let error = call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        with_context(
            root,
            state_path,
            aster_dir,
            &format!("{name}-ledger"),
            payload,
        ),
    )
    .expect_err("edge must fail closed")
    .to_string();
    let after = state_summary(state_path, aster_dir);
    if before != after {
        return Err(format!("{name} edge wrote partial state").into());
    }
    Ok(json!({"edge": name, "before": before, "after": after, "error": error}))
}

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value
        .as_object_mut()
        .expect("context target must be object");
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1176-manual-fsv"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert(
        "state_id".to_string(),
        json!("issue1176-manual-harness-state"),
    );
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1176-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn authored_content(
    content_id: &str,
    similar_to: &str,
    damage: u32,
    cooldown: u32,
    mana: u32,
    damage_type: &str,
    min_bits: u32,
) -> Value {
    let prompt = if content_id == "ood" {
        "Create a spaceship laser railgun ability"
    } else {
        "Create a Dawnward venom archer poison ability in the Mire of Venom"
    };
    json!({
        "content_id": content_id,
        "request": {
            "request_id": format!("issue1176-{content_id}"),
            "prompt": prompt,
            "seed": 1176,
            "requested_by": "issue1176-manual-fsv"
        },
        "corpus": corpus(),
        "assay": assay(),
        "ward": ward(),
        "candidate": {
            "ability_id": format!("ability.issue1176.{content_id}"),
            "display_name": "Venom Archer Volley",
            "damage_type": damage_type,
            "damage": damage,
            "cooldown_seconds": cooldown,
            "mana_cost": mana,
            "summary": "Dawnward venom archer channels poison through the Mire of Venom",
            "tags": ["dawnward", "venom", "archer", "poison", "mire"]
        },
        "oracle": {"min_grounded_bits_milli": min_bits, "min_honesty_milli": 700},
        "panel": panel(),
        "search_calibration": {
            "calibration_id": "issue1176-search-calibration",
            "tau_milli": 300,
            "borderline_margin_milli": 50
        },
        "lodestar_question": "Why does poison belong to the venom archer in the mire?",
        "sextant_query": "venom archer poison mire",
        "similar_to_content_id": similar_to
    })
}

fn corpus() -> Value {
    json!({
        "world_id": "issue1176-world",
        "revision_id": "issue1176-rev",
        "region": "Mire of Venom",
        "factions": ["Dawnward", "Venom Circle"],
        "lore_terms": ["poison", "venom archer", "mire", "antidote", "Dawnward"],
        "anchors": [
            {
                "anchor_id": "anchor-venom",
                "source_ref": "Aster://issue1176/world/venom",
                "text": "Venom archers in the Mire of Venom use poison",
                "terms": ["poison", "venom archer", "mire"]
            },
            {
                "anchor_id": "anchor-antidote",
                "source_ref": "Aster://issue1176/items/antidote",
                "text": "Antidote cleanses poison stacks",
                "terms": ["antidote", "poison"]
            }
        ]
    })
}

fn assay() -> Value {
    json!({
        "assay_id": "assay-issue1176-balance",
        "profile_id": "profile-issue1176-poison",
        "min_damage": 10,
        "max_damage": 80,
        "min_cooldown_seconds": 4,
        "max_cooldown_seconds": 20,
        "max_mana_cost": 60,
        "allowed_damage_types": ["poison", "nature"]
    })
}

fn ward() -> Value {
    json!({
        "policy_id": "ward-issue1176",
        "min_source_anchors": 1,
        "min_grounded_terms": 3,
        "blocked_terms": ["spaceship", "laser", "railgun"]
    })
}

fn panel() -> Value {
    let report_path = repo_root().join("docs2").join("EMBEDDER_ROSTER_REPORT.md");
    let hash = sha256_hex(&fs::read(report_path).expect("roster report must exist"));
    json!({
        "template_name": "constellation-24",
        "template_status": "gate_passed",
        "content_lens_count": 10,
        "association_family_count": 4,
        "lens_names": [
            "a38_embeddinggemma_300m",
            "a38_qwen3_embedding_06b",
            "semantic-multilingual-e5-base-fastembed",
            "a38_bgem3_dense",
            "a38_bgem3_sparse",
            "a38_spladepp_v1_sparse",
            "a38_answerai_colbert_small_v1",
            "a38_jina_v2_base_code",
            "semantic-clip-vit-b32-text-fastembed",
            "a38_bgem3_colbert"
        ],
        "source_ref": "docs2/EMBEDDER_ROSTER_REPORT.md#constellation-24",
        "source_sha256": hash
    })
}

fn codes(record: &Value, content_id: &str) -> Vec<String> {
    let Some(signals) = record["signals"].as_array() else {
        return Vec::new();
    };
    signals
        .iter()
        .filter(|signal| signal["content_id"] == content_id)
        .filter_map(|signal| signal["code"].as_str().map(ToString::to_string))
        .collect()
}

fn state_summary(state_path: &Path, aster_dir: &Path) -> Value {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "signal_count": state.as_ref().map(|read| read.state.calyx_signals.len()).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
}

fn clean_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}
