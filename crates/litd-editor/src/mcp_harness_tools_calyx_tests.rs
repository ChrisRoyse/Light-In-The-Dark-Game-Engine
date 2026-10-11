use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::mcp_harness_tools::{
    CALYX_SIGNALS_TOOL, READ_HARNESS_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
    read_mcp_harness_state,
};

#[test]
fn issue1176_calyx_pre_run_signals_persist_expected_verdicts() {
    let root = test_root("unit");
    clean_dir(&root);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    let before = state_summary(&state_path, &aster_dir);
    println!(
        "issue1176 before {}",
        serde_json::to_string(&before).unwrap()
    );

    let result = call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "signals-ledger",
            json!({
                "signal_batch_id": "issue1176-signals",
                "authored_content": [
                    authored_content("happy", "similar", 40, 8, 20, "poison", 700),
                    authored_content("similar", "happy", 42, 9, 22, "poison", 700),
                    authored_content("imbalanced", "happy", 999, 8, 20, "poison", 700),
                    authored_content("ood", "happy", 40, 8, 20, "poison", 700),
                    authored_content("insufficient", "happy", 40, 8, 20, "poison", 2_000)
                ]
            }),
        ),
    )
    .unwrap();

    let record = &result.result["calyx_signals"];
    assert_eq!(record["signal_count"], 30);
    assert_eq!(record["admitted"], false);
    assert_code(record, "happy", "LITD-HARNESS-ASSAY-BALANCED");
    assert_code(record, "happy", "LITD-HARNESS-ORACLE-ADMITTED");
    assert_code(record, "happy", "LITD-HARNESS-WARD-ADMITTED");
    assert_code(record, "happy", "LITD-HARNESS-LODESTAR-ANSWERED");
    assert_code(record, "happy", "LITD-HARNESS-SEXTANT-ADMITTED");
    assert_code(record, "happy", "LITD-HARNESS-LOOM-SIMILAR");
    assert_code(record, "imbalanced", "LITD-HARNESS-ASSAY-IMBALANCED");
    assert_code(record, "ood", "LITD-HARNESS-WARD-OOD");
    assert_code(record, "insufficient", "LITD-HARNESS-ORACLE-INSUFFICIENT");

    let read =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(
        read.state_readback
            .as_ref()
            .unwrap()
            .state
            .calyx_signals
            .len(),
        1
    );
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1176-signals-ledger")
        .unwrap()
        .expect("issue1176 Aster ledger row must exist");
    assert_eq!(
        aster.sha256,
        result.ledger_readback.as_ref().unwrap().sha256
    );
    assert!(aster.graph_rows >= 2);

    let after = state_summary(&state_path, &aster_dir);
    println!("issue1176 after {}", serde_json::to_string(&after).unwrap());
}

#[test]
fn issue1176_calyx_signal_edges_fail_without_partial_writes() {
    let root = test_root("edges");
    clean_dir(&root);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let empty_before = state_summary(&state_path, &aster_dir);
    let empty = call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "empty-ledger",
            json!({"signal_batch_id": "empty"}),
        ),
    )
    .unwrap_err()
    .to_string();
    let empty_after = state_summary(&state_path, &aster_dir);
    assert_eq!(empty_before, empty_after);
    println!(
        "issue1176 empty before={} after={} error={}",
        empty_before, empty_after, empty
    );

    let panel_before = state_summary(&state_path, &aster_dir);
    let mut bad = authored_content("bad-panel", "bad-panel", 40, 8, 20, "poison", 700);
    bad["panel"]["content_lens_count"] = json!(9);
    bad["panel"]["lens_names"] = json!(["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
    let panel = call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "bad-panel-ledger",
            json!({"signal_batch_id": "bad-panel", "authored_content": [bad]}),
        ),
    )
    .unwrap_err()
    .to_string();
    let panel_after = state_summary(&state_path, &aster_dir);
    assert_eq!(panel_before, panel_after);
    println!(
        "issue1176 bad_panel before={} after={} error={}",
        panel_before, panel_after, panel
    );
}

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value.as_object_mut().unwrap();
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1176-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1176-harness-state"));
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
    let prompt = match content_id {
        "ood" => "Create a spaceship laser railgun ability".to_string(),
        _ => "Create a Dawnward venom archer poison ability in the Mire of Venom".to_string(),
    };
    json!({
        "content_id": content_id,
        "request": {
            "request_id": format!("issue1176-{content_id}"),
            "prompt": prompt,
            "seed": 1176,
            "requested_by": "issue1176-test"
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
        "oracle": {
            "min_grounded_bits_milli": min_bits,
            "min_honesty_milli": 700
        },
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
    let hash = sha256_hex(&fs::read(report_path).unwrap());
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

fn assert_code(record: &Value, content_id: &str, code: &str) {
    let found = record["signals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|signal| signal["content_id"] == content_id && signal["code"] == code);
    assert!(found, "missing {content_id}/{code}");
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

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1176_calyx_signals", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}
