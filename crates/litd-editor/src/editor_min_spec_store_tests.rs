//! Real-Aster round-trip tests for the editor min-spec store (#1793 / #1348).

use litd_engine::editor_min_spec::{AdapterClass, EditorDegradeSettings};
use litd_hash::sha256_hex;

use super::*;

fn store(label: &str) -> EditorMinSpecStore {
    EditorMinSpecStore::new(crate::test_scratch::issue_scratch_dir(
        "issue1793_editor_min_spec",
        label,
    ))
}

#[test]
fn save_then_load_round_trips_through_real_aster() {
    let store = store("round_trip");
    assert!(store.load_degrade_settings().unwrap().is_none());

    let settings = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    let readback = store.save_degrade_settings(&settings).unwrap();
    assert!(readback.seq > 0);
    assert!(readback.graph_rows >= 2, "expected cell + index rows");
    assert_eq!(
        readback.sha256,
        sha256_hex(&serde_json::to_vec_pretty(&settings).unwrap())
    );

    let loaded = store.load_degrade_settings().unwrap().unwrap();
    assert_eq!(loaded, settings);
}

#[test]
fn later_save_supersedes_earlier() {
    let store = store("supersede");
    let low = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    let discrete = EditorDegradeSettings::defaults_for(AdapterClass::Discrete);
    store.save_degrade_settings(&low).unwrap();
    store.save_degrade_settings(&discrete).unwrap();
    assert_eq!(store.load_degrade_settings().unwrap().unwrap(), discrete);
}

#[test]
fn save_rejects_invalid_settings_before_touching_aster() {
    let store = store("reject_invalid");
    let bad = EditorDegradeSettings {
        adapter_class: AdapterClass::Integrated,
        shadow_atlas_size: 1000, // not a power of two
        viewport_resolution_scale_percent: 75,
    };
    match store.save_degrade_settings(&bad) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert_eq!(field, "editor_min_spec.degrade.shadow_atlas_size");
        }
        other => panic!("expected fail-loud validation, got {other:?}"),
    }
    // Nothing was persisted.
    assert!(store.load_degrade_settings().unwrap().is_none());
}
