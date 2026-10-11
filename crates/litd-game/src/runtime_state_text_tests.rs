use super::*;
use litd_engine::render::runtime_font::{
    RUNTIME_FONT_LICENSE, RUNTIME_FONT_LICENSE_NOTICE_SHA256, RUNTIME_FONT_SOURCE_BYTES,
    RUNTIME_FONT_SOURCE_ID, RUNTIME_FONT_SOURCE_SHA256, RUNTIME_FONT_SOURCE_VERSION,
};

#[test]
fn runtime_text_state_serializes_exact_font_plan_submission_and_error_evidence() {
    let mut state = RuntimeTextState::new(
        RUNTIME_FONT_SOURCE_ID.to_string(),
        RUNTIME_FONT_SOURCE_VERSION.to_string(),
        RUNTIME_FONT_SOURCE_BYTES,
        RUNTIME_FONT_SOURCE_SHA256.to_string(),
        RUNTIME_FONT_LICENSE.to_string(),
        RUNTIME_FONT_LICENSE_NOTICE_SHA256.to_string(),
        "atlas-fnv1a64".to_string(),
    );
    state.begin_frame(41, 3, 2);
    state.record_plan("plan-fnv1a64".to_string(), 4, 0, 1, 17);
    state.record_submission(17, 102);
    state.record_error(
        "gpu.submit",
        "validation rejected runtime text vertices".to_string(),
    );

    let bytes = serde_json::to_vec(&state).expect("serialize runtime text state");
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).expect("parse runtime text state JSON");
    assert_eq!(value["schema"], RUNTIME_TEXT_STATE_SCHEMA);
    assert_eq!(value["font_source_path"], RUNTIME_FONT_SOURCE_ID);
    assert_eq!(value["font_source_version"], RUNTIME_FONT_SOURCE_VERSION);
    assert_eq!(value["font_source_bytes"], RUNTIME_FONT_SOURCE_BYTES);
    assert_eq!(value["font_source_sha256"], RUNTIME_FONT_SOURCE_SHA256);
    assert_eq!(value["font_license"], RUNTIME_FONT_LICENSE);
    assert_eq!(
        value["font_license_notice_sha256"],
        RUNTIME_FONT_LICENSE_NOTICE_SHA256
    );
    assert_eq!(value["font_atlas_fnv1a64"], "atlas-fnv1a64");
    assert_eq!(value["plan_fnv1a64"], "plan-fnv1a64");
    assert_eq!(value["label_count"], 3);
    assert_eq!(value["damage_number_count"], 2);
    assert_eq!(value["planned_glyph_count"], 17);
    assert_eq!(value["submitted_glyph_count"], 17);
    assert_eq!(value["submitted_vertex_count"], 102);
    assert_eq!(value["errors"][0]["stage"], "gpu.submit");

    let readback: RuntimeTextState =
        serde_json::from_slice(&bytes).expect("deserialize runtime text state");
    assert_eq!(readback, state);
}
