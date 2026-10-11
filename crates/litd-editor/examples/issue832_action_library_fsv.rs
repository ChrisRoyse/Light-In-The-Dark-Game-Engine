use std::{collections::BTreeMap, fs};

use litd_editor::trigger_action_library::{
    REQUIRED_ACTION_CATEGORIES, TriggerActionCategory, TriggerActionContext,
    TriggerActionUnitState, TriggerActionVariable, action_catalog, actions_by_category,
    execute_action, find_action, validate_action_args, validate_action_catalog_complete,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue832Fsv {
    issue: u32,
    source_of_truth: Value,
    before: Value,
    after: Value,
    happy_path: Value,
    edges: Vec<Value>,
    all_expected_observed: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE832_FSV_ROOT",
        "issue832_action_library",
        "manual",
    )?;
    let catalog_path = root.join("action-catalog.json");
    let report_path = root.join("manual-fsv-report.json");
    let before = file_state(&catalog_path)?;

    validate_action_catalog_complete()?;
    let catalog = action_catalog();
    write_json(&catalog_path, &catalog)?;
    let catalog_readback: Vec<Value> = read_json(&catalog_path)?;
    let after = file_state(&catalog_path)?;

    let mut context = synthetic_context();
    let outcomes = run_happy_path(&mut context)?;
    let mut category_counts = BTreeMap::new();
    for category in REQUIRED_ACTION_CATEGORIES {
        category_counts.insert(format!("{category:?}"), actions_by_category(category).len());
    }
    let flow_action = find_action("if_then_else")?;
    let expected_observed = catalog_readback.len() == catalog.len()
        && catalog.len() >= 12
        && category_counts.values().all(|count| *count > 0)
        && flow_action.category == TriggerActionCategory::Flow
        && context.units["unit_footman_02"].region_id == "region_bridge_01"
        && !context.units["unit_guard_01"].alive
        && context.variables["sum"] == TriggerActionVariable::Integer(5)
        && context.variables["label"] == TriggerActionVariable::String("gate_open".to_string())
        && context.waits_seconds == vec![1.5]
        && context.loop_records == vec!["pulse_gate x3".to_string()]
        && context.branch_records == vec!["open_gate".to_string()];
    if !expected_observed {
        return Err("issue832 action expected values were not observed".into());
    }

    let edges = vec![
        edge_unknown_action()?,
        edge_missing_arg()?,
        edge_unknown_arg()?,
        edge_bad_arg_type()?,
        edge_missing_state()?,
    ];
    let report = canonical_json_roundtrip(&Issue832Fsv {
        issue: 832,
        source_of_truth: json!({
            "catalog_json": catalog_path,
            "report": report_path
        }),
        before,
        after,
        happy_path: json!({
            "catalog_count": catalog.len(),
            "category_counts": category_counts,
            "flow_action": flow_action,
            "outcomes": outcomes,
            "context": context,
            "catalog_sha256": sha256_file(&catalog_path)?
        }),
        edges,
        all_expected_observed: expected_observed,
    })?;
    write_json(&report_path, &report)?;
    if read_json::<Issue832Fsv>(&report_path)? != report {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue832 report={} report_sha256={} catalog_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&catalog_path)?
    );
    Ok(())
}

fn run_happy_path(
    context: &mut TriggerActionContext,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let steps = [
        (
            "create_unit",
            args([
                ("unit_id", "unit_footman_02"),
                ("unit_type_id", "footman"),
                ("player_id", "player_1"),
                ("region_id", "region_gate_01"),
            ]),
        ),
        (
            "move_unit",
            args([
                ("unit_id", "unit_footman_02"),
                ("region_id", "region_bridge_01"),
            ]),
        ),
        ("kill_unit", args([("unit_id", "unit_guard_01")])),
        (
            "integer_add",
            args([("left", "2"), ("right", "3"), ("result_var", "sum")]),
        ),
        (
            "string_concat",
            args([
                ("left", "gate"),
                ("right", "_open"),
                ("result_var", "label"),
            ]),
        ),
        ("wait_seconds", args([("seconds", "1.5")])),
        (
            "repeat_loop",
            args([("count", "3"), ("action_label", "pulse_gate")]),
        ),
        (
            "if_then_else",
            args([
                ("condition_result", "true"),
                ("then_label", "open_gate"),
                ("else_label", "close_gate"),
            ]),
        ),
    ];
    let mut outcomes = Vec::new();
    for (operation, step_args) in steps {
        outcomes.push(json!(execute_action(operation, &step_args, context)?));
    }
    Ok(outcomes)
}

fn edge_unknown_action() -> Result<Value, Box<dyn std::error::Error>> {
    let error = find_action("summon_everything_forever")
        .unwrap_err()
        .to_string();
    if !error.contains("unknown trigger action") {
        return Err(format!("unexpected unknown-action error: {error}").into());
    }
    Ok(json!({ "name": "unknown_action", "error": error }))
}

fn edge_missing_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let error = validate_action_args("kill_unit", &BTreeMap::new())
        .unwrap_err()
        .to_string();
    if !error.contains("missing required arg unit_id") {
        return Err(format!("unexpected missing-arg error: {error}").into());
    }
    Ok(json!({ "name": "missing_required_arg", "error": error }))
}

fn edge_unknown_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let error = validate_action_args(
        "kill_unit",
        &args([("unit_id", "unit_guard_01"), ("surprise", "nope")]),
    )
    .unwrap_err()
    .to_string();
    if !error.contains("unknown arg surprise") {
        return Err(format!("unexpected unknown-arg error: {error}").into());
    }
    Ok(json!({ "name": "unknown_arg", "error": error }))
}

fn edge_bad_arg_type() -> Result<Value, Box<dyn std::error::Error>> {
    let error = validate_action_args("wait_seconds", &args([("seconds", "-1.0")]))
        .unwrap_err()
        .to_string();
    if !error.contains("not a non-negative real") {
        return Err(format!("unexpected bad-type error: {error}").into());
    }
    Ok(json!({ "name": "bad_arg_type", "error": error }))
}

fn edge_missing_state() -> Result<Value, Box<dyn std::error::Error>> {
    let error = execute_action(
        "kill_unit",
        &args([("unit_id", "unit_missing_99")]),
        &mut synthetic_context(),
    )
    .unwrap_err()
    .to_string();
    if !error.contains("unknown unit unit_missing_99") {
        return Err(format!("unexpected missing-state error: {error}").into());
    }
    Ok(json!({ "name": "missing_state", "error": error }))
}

fn args<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn synthetic_context() -> TriggerActionContext {
    let mut units = BTreeMap::new();
    units.insert(
        "unit_guard_01".to_string(),
        TriggerActionUnitState {
            alive: true,
            unit_type_id: "guard".to_string(),
            owner_player_id: "player_1".to_string(),
            region_id: "region_gate_01".to_string(),
            current_order: None,
        },
    );
    TriggerActionContext {
        units,
        variables: BTreeMap::new(),
        waits_seconds: Vec::new(),
        loop_records: Vec::new(),
        branch_records: Vec::new(),
    }
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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

fn file_state(path: &std::path::Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(json!({
        "exists": path.exists(),
        "bytes": fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0),
        "sha256": if path.exists() { sha256_file(path)? } else { String::new() }
    }))
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
