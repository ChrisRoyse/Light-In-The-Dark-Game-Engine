use std::{collections::BTreeMap, fs};

use litd_editor::trigger_condition_library::{
    REQUIRED_CONDITION_CATEGORIES, TriggerConditionCategory, TriggerConditionContext,
    TriggerGameState, TriggerPlayerState, TriggerUnitState, condition_catalog,
    conditions_by_category, evaluate_condition, find_condition, validate_condition_args,
    validate_condition_catalog_complete,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue831Fsv {
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
        "LITD_ISSUE831_FSV_ROOT",
        "issue831_condition_library",
        "manual",
    )?;
    let catalog_path = root.join("condition-catalog.json");
    let report_path = root.join("manual-fsv-report.json");
    let before = file_state(&catalog_path)?;

    validate_condition_catalog_complete()?;
    let catalog = condition_catalog();
    write_json(&catalog_path, &catalog)?;
    let catalog_readback: Vec<Value> = read_json(&catalog_path)?;
    let after = file_state(&catalog_path)?;

    let context = synthetic_context();
    let mut category_counts = BTreeMap::new();
    for category in REQUIRED_CONDITION_CATEGORIES {
        category_counts.insert(
            format!("{category:?}"),
            conditions_by_category(category).len(),
        );
    }
    let comparison_true = evaluate_condition(
        "integer_greater_equal",
        &args([("left", "125"), ("right", "100")]),
        &context,
    )?;
    let boolean_false = evaluate_condition(
        "boolean_and",
        &args([("left", "true"), ("right", "false")]),
        &context,
    )?;
    let unit_region_true = evaluate_condition(
        "unit_in_region",
        &args([
            ("unit_id", "unit_guard_01"),
            ("region_id", "region_gate_01"),
        ]),
        &context,
    )?;
    let player_resource_true = evaluate_condition(
        "player_resource_at_least",
        &args([
            ("player_id", "player_1"),
            ("resource_kind", "gold"),
            ("minimum", "100"),
        ]),
        &context,
    )?;
    let game_flag_true =
        evaluate_condition("game_flag_true", &args([("flag", "night")]), &context)?;
    let player_condition = find_condition("player_resource_at_least")?;
    let expected_observed = catalog_readback.len() == catalog.len()
        && catalog.len() >= 16
        && category_counts.values().all(|count| *count > 0)
        && player_condition.category == TriggerConditionCategory::PlayerState
        && comparison_true
        && !boolean_false
        && unit_region_true
        && player_resource_true
        && game_flag_true;
    if !expected_observed {
        return Err("issue831 condition expected values were not observed".into());
    }

    let edges = vec![
        edge_unknown_condition()?,
        edge_missing_arg()?,
        edge_unknown_arg()?,
        edge_bad_arg_type()?,
        edge_missing_state(&context)?,
    ];
    let report = canonical_json_roundtrip(&Issue831Fsv {
        issue: 831,
        source_of_truth: json!({
            "catalog_json": catalog_path,
            "report": report_path
        }),
        before,
        after,
        happy_path: json!({
            "catalog_count": catalog.len(),
            "category_counts": category_counts,
            "comparison_true": comparison_true,
            "boolean_false": boolean_false,
            "unit_region_true": unit_region_true,
            "player_resource_true": player_resource_true,
            "game_flag_true": game_flag_true,
            "player_condition": player_condition,
            "catalog_sha256": sha256_file(&catalog_path)?
        }),
        edges,
        all_expected_observed: expected_observed,
    })?;
    write_json(&report_path, &report)?;
    if read_json::<Issue831Fsv>(&report_path)? != report {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue831 report={} report_sha256={} catalog_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&catalog_path)?
    );
    Ok(())
}

fn edge_unknown_condition() -> Result<Value, Box<dyn std::error::Error>> {
    let error = find_condition("unit_is_secretly_a_tree")
        .unwrap_err()
        .to_string();
    if !error.contains("unknown trigger condition") {
        return Err(format!("unexpected unknown-condition error: {error}").into());
    }
    Ok(json!({ "name": "unknown_condition", "error": error }))
}

fn edge_missing_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let error = validate_condition_args("unit_alive", &BTreeMap::new())
        .unwrap_err()
        .to_string();
    if !error.contains("missing required arg unit_id") {
        return Err(format!("unexpected missing-arg error: {error}").into());
    }
    Ok(json!({ "name": "missing_required_arg", "error": error }))
}

fn edge_unknown_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let error = validate_condition_args(
        "unit_alive",
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
    let error = validate_condition_args(
        "real_less_than",
        &args([("left", "not_real"), ("right", "10.5")]),
    )
    .unwrap_err()
    .to_string();
    if !error.contains("not a finite real") {
        return Err(format!("unexpected bad-type error: {error}").into());
    }
    Ok(json!({ "name": "bad_arg_type", "error": error }))
}

fn edge_missing_state(
    context: &TriggerConditionContext,
) -> Result<Value, Box<dyn std::error::Error>> {
    let error = evaluate_condition(
        "unit_alive",
        &args([("unit_id", "unit_unknown_99")]),
        context,
    )
    .unwrap_err()
    .to_string();
    if !error.contains("unknown unit unit_unknown_99") {
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

fn synthetic_context() -> TriggerConditionContext {
    let mut resources = BTreeMap::new();
    resources.insert("gold".to_string(), 125);
    let mut player_flags = BTreeMap::new();
    player_flags.insert("quest_started".to_string(), true);
    let mut players = BTreeMap::new();
    players.insert(
        "player_1".to_string(),
        TriggerPlayerState {
            resources,
            flags: player_flags,
        },
    );
    let mut units = BTreeMap::new();
    units.insert(
        "unit_guard_01".to_string(),
        TriggerUnitState {
            alive: true,
            unit_type_id: "guard".to_string(),
            owner_player_id: "player_1".to_string(),
            region_id: "region_gate_01".to_string(),
        },
    );
    let mut game_flags = BTreeMap::new();
    game_flags.insert("night".to_string(), true);
    TriggerConditionContext {
        units,
        players,
        game: TriggerGameState {
            flags: game_flags,
            difficulty: "hard".to_string(),
        },
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
