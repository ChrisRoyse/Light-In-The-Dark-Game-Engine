use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[test]
fn event_catalog_covers_wc3_style_categories() {
    validate_catalog_complete().unwrap();
    let categories = event_catalog()
        .iter()
        .map(|event| event.category)
        .collect::<BTreeSet<_>>();
    assert_eq!(categories.len(), REQUIRED_CATEGORIES.len());
    assert!(event_catalog().len() >= 24);
    assert!(find_event("unit_dies").is_ok());
    assert!(find_event("destructible_in_region_dies").is_ok());
}

#[test]
fn event_arg_validation_accepts_known_region_event() {
    let mut args = BTreeMap::new();
    args.insert("region_id".to_string(), "region_gate_01".to_string());
    args.insert("unit_id".to_string(), "unit_guard_01".to_string());
    let event = validate_event_args("unit_enters_region", &args).unwrap();
    assert_eq!(event.category, TriggerEventCategory::Region);
}

#[test]
fn event_arg_validation_fails_closed_for_unknown_event() {
    let error = find_event("unit_goes_missing").unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("unknown trigger event"));
}

#[test]
fn event_arg_validation_fails_closed_for_missing_required_arg() {
    let args = BTreeMap::new();
    let error = validate_event_args("unit_dies", &args).unwrap_err();
    assert!(error.to_string().contains("missing required arg unit_id"));
}

#[test]
fn event_arg_validation_fails_closed_for_unknown_arg() {
    let mut args = BTreeMap::new();
    args.insert("unit_id".to_string(), "unit_guard_01".to_string());
    args.insert("surprise".to_string(), "nope".to_string());
    let error = validate_event_args("unit_dies", &args).unwrap_err();
    assert!(error.to_string().contains("unknown arg surprise"));
}
