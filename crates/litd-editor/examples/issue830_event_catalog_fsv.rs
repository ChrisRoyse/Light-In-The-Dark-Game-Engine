use std::{collections::BTreeMap, fs};

use litd_editor::trigger_event_catalog::{
    REQUIRED_CATEGORIES, TriggerEventCategory, event_catalog, events_by_category, find_event,
    validate_catalog_complete, validate_event_args,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue830Fsv {
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
        "LITD_ISSUE830_FSV_ROOT",
        "issue830_event_catalog",
        "manual",
    )?;
    let catalog_path = root.join("event-catalog.json");
    let report_path = root.join("manual-fsv-report.json");
    let before = file_state(&catalog_path)?;

    validate_catalog_complete()?;
    let catalog = event_catalog();
    write_json(&catalog_path, &catalog)?;
    let catalog_readback: Vec<Value> = read_json(&catalog_path)?;
    let after = file_state(&catalog_path)?;

    let mut category_counts = BTreeMap::new();
    for category in REQUIRED_CATEGORIES {
        category_counts.insert(format!("{category:?}"), events_by_category(category).len());
    }
    let mut region_args = BTreeMap::new();
    region_args.insert("region_id".to_string(), "region_gate_01".to_string());
    region_args.insert("unit_id".to_string(), "unit_guard_01".to_string());
    let region_event = validate_event_args("unit_enters_region", &region_args)?;
    let unit_event = find_event("unit_dies")?;
    let expected_observed = catalog_readback.len() == catalog.len()
        && catalog.len() >= 24
        && category_counts.values().all(|count| *count > 0)
        && region_event.category == TriggerEventCategory::Region
        && unit_event.category == TriggerEventCategory::Unit;
    if !expected_observed {
        return Err("issue830 catalog expected values were not observed".into());
    }

    let edges = vec![
        edge_unknown_event()?,
        edge_missing_arg()?,
        edge_unknown_arg()?,
        edge_bad_arg_type()?,
    ];
    let report = canonical_json_roundtrip(&Issue830Fsv {
        issue: 830,
        source_of_truth: json!({
            "catalog_json": catalog_path,
            "report": report_path
        }),
        before,
        after,
        happy_path: json!({
            "catalog_count": catalog.len(),
            "category_counts": category_counts,
            "unit_event": unit_event,
            "region_event": region_event,
            "catalog_sha256": sha256_file(&catalog_path)?
        }),
        edges,
        all_expected_observed: expected_observed,
    })?;
    write_json(&report_path, &report)?;
    if read_json::<Issue830Fsv>(&report_path)? != report {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue830 report={} report_sha256={} catalog_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&catalog_path)?
    );
    Ok(())
}

fn edge_unknown_event() -> Result<Value, Box<dyn std::error::Error>> {
    let error = find_event("unit_goes_missing").unwrap_err().to_string();
    if !error.contains("unknown trigger event") {
        return Err(format!("unexpected unknown-event error: {error}").into());
    }
    Ok(json!({ "name": "unknown_event", "error": error }))
}

fn edge_missing_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let args = BTreeMap::new();
    let error = validate_event_args("unit_dies", &args)
        .unwrap_err()
        .to_string();
    if !error.contains("missing required arg unit_id") {
        return Err(format!("unexpected missing-arg error: {error}").into());
    }
    Ok(json!({ "name": "missing_required_arg", "error": error }))
}

fn edge_unknown_arg() -> Result<Value, Box<dyn std::error::Error>> {
    let mut args = BTreeMap::new();
    args.insert("unit_id".to_string(), "unit_guard_01".to_string());
    args.insert("surprise".to_string(), "nope".to_string());
    let error = validate_event_args("unit_dies", &args)
        .unwrap_err()
        .to_string();
    if !error.contains("unknown arg surprise") {
        return Err(format!("unexpected unknown-arg error: {error}").into());
    }
    Ok(json!({ "name": "unknown_arg", "error": error }))
}

fn edge_bad_arg_type() -> Result<Value, Box<dyn std::error::Error>> {
    let mut args = BTreeMap::new();
    args.insert("period_seconds".to_string(), "not_real".to_string());
    let error = validate_event_args("periodic_timer", &args)
        .unwrap_err()
        .to_string();
    if !error.contains("not a finite real") {
        return Err(format!("unexpected bad-type error: {error}").into());
    }
    Ok(json!({ "name": "bad_arg_type", "error": error }))
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
