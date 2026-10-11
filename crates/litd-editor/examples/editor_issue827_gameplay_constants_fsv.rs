//! FSV for #827 (Gameplay Constants) + #829 (object-data lowering valve).
//!
//! Vertical slice, all through the REAL MCP surface: author the singleton `GameplayConstant`
//! record FROM the shipped base defaults with a few balance overrides; persist through the
//! object MCP tool into the Aster vault; INDEPENDENT fresh-handle vault readback matches the
//! authored fields; lower through the #829 valve MCP tool to a fingerprinted litd/data TOML on
//! disk (the FULL resolved constant set, fractional knobs as lossless IEEE-754 f64 bit
//! patterns); re-read the TOML (separate MCP tool) and verify every overridden + base-carried
//! field, the float round-trips, and the fingerprint.
//!
//! Edge cases (before/after state printed): (1) a non-monotonic XP-threshold override is typed-
//! rejected at persist time and NOTHING persists; (2) a zero-override copy preserves the full
//! base constant set; (3) re-lowering unchanged data yields an identical fingerprint. Issue-
//! specific: two GameplayConstant records fail the singleton-per-project rule at lowering.
//!
//! Negative control: corrupt the lowered artifact on disk and prove the readback rejects it.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_GAMEPLAY_CONSTANTS_TOOL, READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL,
        call_mcp_object_tool, read_mcp_object_document,
    },
    object_editor::gameplay_constants_base_properties,
};
use litd_engine::{EngineError, fsv_artifacts::begin_issue_run};
use serde_json::{Map, Value, json};

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

fn base_properties() -> Value {
    let mut map = Map::new();
    for (key, value) in gameplay_constants_base_properties() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
    Value::Object(map)
}

fn base_template() -> Value {
    json!({
        "base_object_id": "base.gameplay.constants",
        "kind": "GameplayConstant",
        "display_name": "Gameplay Constants",
        "properties": base_properties(),
    })
}

fn copy_constants(
    ctx: &Ctx,
    document_path: &std::path::Path,
    new_object_id: &str,
    rawcode: &str,
    overrides: Value,
    seed: Option<EditorDocument>,
) -> Result<Value, EngineError> {
    let initial = match seed {
        Some(doc) => serde_json::to_value(doc).unwrap(),
        None => Value::Null,
    };
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ctx.ledger_path,
            "ledger_id": "issue827-constants-ledger",
            "request_id": format!("issue827-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue827-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": base_template(),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower_constants(document_path: &std::path::Path, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_GAMEPLAY_CONSTANTS_TOOL,
        json!({ "document_path": document_path, "artifact_path": artifact_path }),
    )
    .expect("lower_gameplay_constants MCP tool must succeed")
    .result
}

fn read_lowered(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL,
        json!({ "artifact_path": artifact_path, "expected_fingerprint": fingerprint }),
    )
    .expect("read_lowered_gameplay_constants MCP tool must succeed")
    .result
}

fn overrides() -> Value {
    json!({
        "cycle.day_length_secs": "600",
        "hero.str_to_hp": "30.0",
        "item.drop_rate_pct": "20.0",
        "xp.level_thresholds": "0,300,700,1200,1800",
        "misc.max_hero_level": "5",
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue827_gameplay_constants")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("constants-document-aster"),
        ledger_path: vault.join("constants-ledger-aster"),
    };
    let toml = run.artifact_path("lowered/constants.toml")?;
    let toml_b = run.artifact_path("lowered/constants.redo.toml")?;
    let clone_toml = run.artifact_path("lowered/constants.clone.toml")?;

    // 1. Author the singleton GameplayConstant FROM base with balance overrides.
    let seed = EditorDocument::new("calyx://world/issue827-constants", "issue827-rev-1")?;
    copy_constants(
        &ctx,
        &ctx.document_path,
        "gameplay.constants",
        "gGc1",
        overrides(),
        Some(seed),
    )?;

    // 2. INDEPENDENT vault readback (fresh handle) confirms the persisted, merged record.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let record = persisted
        .document
        .objects
        .get("gameplay.constants")
        .ok_or("constants missing from independent vault readback")?;
    let vault_day = record.properties.get("cycle.day_length_secs").cloned();
    let vault_str_to_hp = record.properties.get("hero.str_to_hp").cloned();
    // Base-carried (non-overridden) field survives the copy.
    let vault_night = record.properties.get("cycle.night_length_secs").cloned();

    // 3. Lower through the #829 valve, then re-read the TOML from disk.
    let lowered = lower_constants(&ctx.document_path, &toml);
    let fingerprint = lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_constants produced no fingerprint")?
        .to_string();
    let disk = read_lowered(&toml, &fingerprint);
    let constants = &disk["lowered_artifact"]["constants"];

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let lowered_again = lower_constants(&ctx.document_path, &toml_b);
    let fingerprint_again = lowered_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: a non-monotonic XP-threshold override is rejected AND nothing persists.
    let bad_ctx = Ctx {
        document_path: vault.join("constants-bad-aster"),
        ledger_path: vault.join("constants-bad-ledger-aster"),
    };
    let bad_seed = EditorDocument::new("calyx://world/issue827-bad", "issue827-bad-1")?;
    let before_bad = bad_ctx.document_path.exists();
    let bad = copy_constants(
        &bad_ctx,
        &bad_ctx.document_path,
        "gameplay.bad",
        "gBd1",
        json!({ "xp.level_thresholds": "0,500,300" }),
        Some(bad_seed),
    );
    let after_bad = bad_ctx.document_path.exists();

    // Edge 2: zero-override copy preserves the FULL base constant set.
    let clone_ctx = Ctx {
        document_path: vault.join("constants-clone-aster"),
        ledger_path: vault.join("constants-clone-ledger-aster"),
    };
    let clone_seed = EditorDocument::new("calyx://world/issue827-clone", "issue827-clone-1")?;
    copy_constants(
        &clone_ctx,
        &clone_ctx.document_path,
        "gameplay.clone",
        "gCl1",
        json!({}),
        Some(clone_seed),
    )?;
    let clone_lowered = lower_constants(&clone_ctx.document_path, &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered(&clone_toml, &clone_fp);
    let clone_constants = &clone_disk["lowered_artifact"]["constants"];

    // Singleton rule: two GameplayConstant records in one document fail loud at lowering.
    let dual_ctx = Ctx {
        document_path: vault.join("constants-dual-aster"),
        ledger_path: vault.join("constants-dual-ledger-aster"),
    };
    let dual_seed = EditorDocument::new("calyx://world/issue827-dual", "issue827-dual-1")?;
    copy_constants(
        &dual_ctx,
        &dual_ctx.document_path,
        "gameplay.one",
        "gO01",
        json!({}),
        Some(dual_seed),
    )?;
    copy_constants(
        &dual_ctx,
        &dual_ctx.document_path,
        "gameplay.two",
        "gTw1",
        json!({}),
        None,
    )?;
    let dual = call_mcp_object_tool(
        LOWER_GAMEPLAY_CONSTANTS_TOOL,
        json!({
            "document_path": dual_ctx.document_path,
            "artifact_path": run.artifact_path("lowered/constants.dual.toml")?,
        }),
    );

    // Negative control: corrupt the lowered artifact on disk; the readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&toml)?;
    corrupt = corrupt.replace("day_length_secs = 600", "day_length_secs = 601");
    let corrupt_path = run.artifact_path("lowered/constants.corrupt.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": fingerprint }),
    )
    .is_err();
    let corrupt_written = corrupt != std::fs::read_to_string(&toml)?;

    let report = json!({
        "schema": "litd.issue827.gameplay_constants.v1",
        "issues": [827, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> fingerprinted litd/data gameplay-constants TOML re-read from disk",
        "float_encoding": constants["float_encoding"],
        "happy_path": {
            "fingerprint": fingerprint,
            "vault_readback": {
                "day_length_secs": vault_day,
                "str_to_hp": vault_str_to_hp,
                "base_carried_night_length_secs": vault_night,
            },
            "disk_constants": {
                "day_length_secs": constants["day_length_secs"],
                "night_length_secs": constants["night_length_secs"],
                "max_hero_level": constants["max_hero_level"],
                "xp_level_thresholds": constants["xp_level_thresholds"],
                "str_to_hp_bits": constants["str_to_hp_bits"],
                "int_to_mana_bits": constants["int_to_mana_bits"],
                "item_drop_rate_pct_bits": constants["item_drop_rate_pct_bits"],
            },
        },
        "edges": {
            "invalid_thresholds_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
                "error": bad.err().map(|e| e.to_string()),
            },
            "zero_override_full_base_set": {
                "clone_day_length_secs": clone_constants["day_length_secs"],
                "clone_str_to_hp_bits": clone_constants["str_to_hp_bits"],
                "clone_max_hero_level": clone_constants["max_hero_level"],
                "clone_thresholds_len": clone_constants["xp_level_thresholds"].as_array().map(|a| a.len()).unwrap_or(0),
            },
            "deterministic_relowering": {
                "fingerprint_a": fingerprint,
                "fingerprint_b": fingerprint_again,
            },
        },
        "singleton_rule": {
            "two_records_rejected": dual.is_err(),
            "error": dual.err().map(|e| e.to_string()),
        },
        "negative_control": {
            "corrupt_written": corrupt_written,
            "corrupt_readback_rejected": corrupt_rejected,
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #827 report hash mismatch".into());
    }
    println!(
        "wrote #827/#829 gameplay constants FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    need(
        report["float_encoding"] == json!("ieee754_f64_bits"),
        "float encoding tag mismatch",
    )?;
    let happy = &report["happy_path"];
    need(
        happy["vault_readback"]["day_length_secs"] == json!("600"),
        "vault readback day_length mismatch",
    )?;
    need(
        happy["vault_readback"]["base_carried_night_length_secs"] == json!("180"),
        "base-carried night_length did not survive the copy",
    )?;
    let disk = &happy["disk_constants"];
    need(
        disk["day_length_secs"] == json!(600),
        "disk day_length mismatch",
    )?;
    need(
        disk["night_length_secs"] == json!(180),
        "disk base-carried night_length mismatch",
    )?;
    need(
        disk["max_hero_level"] == json!(5),
        "disk max_hero_level mismatch",
    )?;
    need(
        disk["xp_level_thresholds"] == json!([0, 300, 700, 1200, 1800]),
        "disk xp thresholds mismatch",
    )?;
    // Fractional knobs round-trip losslessly through their IEEE-754 f64 bit patterns.
    need(
        disk["str_to_hp_bits"] == json!(30.0f64.to_bits()),
        "disk str_to_hp float bits mismatch",
    )?;
    need(
        disk["int_to_mana_bits"] == json!(15.0f64.to_bits()),
        "disk base-carried int_to_mana float bits mismatch",
    )?;
    need(
        disk["item_drop_rate_pct_bits"] == json!(20.0f64.to_bits()),
        "disk item_drop_rate float bits mismatch",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_thresholds_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid threshold override was NOT rejected without persistence",
    )?;
    let clone = &edges["zero_override_full_base_set"];
    need(
        clone["clone_day_length_secs"] == json!(480),
        "clone did not preserve base day_length",
    )?;
    need(
        clone["clone_str_to_hp_bits"] == json!(25.0f64.to_bits()),
        "clone did not preserve base str_to_hp",
    )?;
    need(
        clone["clone_max_hero_level"] == json!(10),
        "clone did not preserve base max_hero_level",
    )?;
    need(
        clone["clone_thresholds_len"] == json!(10),
        "clone did not preserve full base thresholds",
    )?;
    let det = &edges["deterministic_relowering"];
    need(
        det["fingerprint_a"] == det["fingerprint_b"]
            && !det["fingerprint_a"].as_str().unwrap_or_default().is_empty(),
        "re-lowering unchanged constants was not deterministic",
    )?;

    need(
        report["singleton_rule"]["two_records_rejected"] == json!(true),
        "two GameplayConstant records were NOT rejected by the singleton rule",
    )?;

    let neg = &report["negative_control"];
    need(
        neg["corrupt_written"] == json!(true) && neg["corrupt_readback_rejected"] == json!(true),
        "corrupt lowered constants artifact was NOT rejected on readback",
    )?;
    Ok(())
}
