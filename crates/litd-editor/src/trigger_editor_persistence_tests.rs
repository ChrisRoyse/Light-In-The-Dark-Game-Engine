use std::fs;

use serde_json::Value;

use super::{
    TRIGGER_EDITOR_PERSISTENCE_SCHEMA, TriggerEditorVariable, TriggerEditorVariableType,
    issue733_sample_project, read_trigger_editor_project, save_trigger_editor_project,
};
use crate::test_scratch::issue_scratch_dir;

#[test]
fn save_and_reopen_preserves_exact_graphs_scalars_and_arrays() {
    let path = project_path("roundtrip");
    let mut project = issue733_sample_project().expect("sample project");
    project.variables.insert(
        "wave_count".to_string(),
        TriggerEditorVariable {
            name: "wave_count".to_string(),
            value_type: TriggerEditorVariableType::Integer,
            initial_value: "7".to_string(),
            array_values: Vec::new(),
        },
    );
    project.variables.insert(
        "lane_enabled".to_string(),
        TriggerEditorVariable {
            name: "lane_enabled".to_string(),
            value_type: TriggerEditorVariableType::Boolean,
            initial_value: String::new(),
            array_values: vec!["true".to_string(), "false".to_string(), "true".to_string()],
        },
    );
    project.validate().expect("authored project validates");
    let expected_graphs = project.graphs.clone();
    let expected_fingerprint = project.stable_fingerprint().expect("fingerprint");

    let saved = save_trigger_editor_project(&path, &project).expect("save project");
    let reopened = read_trigger_editor_project(&path).expect("reopen project");

    assert_eq!(reopened.project, project);
    assert_eq!(reopened.project.graphs, expected_graphs);
    assert_eq!(reopened.project.variables["wave_count"].initial_value, "7");
    assert_eq!(
        reopened.project.variables["lane_enabled"].array_values,
        ["true", "false", "true"]
    );
    assert_eq!(saved.sha256, reopened.sha256);
    assert_eq!(saved.project_fingerprint, expected_fingerprint);
    assert_eq!(reopened.project_fingerprint, expected_fingerprint);
}

#[test]
fn persistence_schema_mismatch_fails_loudly() {
    let path = project_path("schema-mismatch");
    save_sample(&path);
    rewrite_json(&path, |json| {
        json["schema"] = Value::String("litd.editor.trigger_editor.persistence.v0".to_string());
    });

    let error = read_trigger_editor_project(&path)
        .expect_err("stale schema must fail")
        .to_string();

    assert!(error.contains("persistence.schema"), "{error}");
    assert!(error.contains(TRIGGER_EDITOR_PERSISTENCE_SCHEMA), "{error}");
}

#[test]
fn project_fingerprint_mismatch_fails_loudly() {
    let path = project_path("fingerprint-mismatch");
    save_sample(&path);
    rewrite_json(&path, |json| {
        json["project_fingerprint"] = Value::String("0".repeat(64));
    });

    let error = read_trigger_editor_project(&path)
        .expect_err("tampered fingerprint must fail")
        .to_string();

    assert!(error.contains("persistence.fingerprint"), "{error}");
    assert!(error.contains("does not match actual"), "{error}");
}

#[test]
fn corrupt_or_missing_project_never_falls_back_to_empty() {
    let corrupt_path = project_path("corrupt");
    fs::write(&corrupt_path, b"{ definitely not valid json").expect("write corrupt project bytes");

    let corrupt_error = read_trigger_editor_project(&corrupt_path)
        .expect_err("corrupt project must fail")
        .to_string();
    let missing_error = read_trigger_editor_project(&project_path("missing"))
        .expect_err("missing project must fail")
        .to_string();

    assert!(
        corrupt_error.contains("persistence.decode"),
        "{corrupt_error}"
    );
    assert!(
        missing_error.contains("persistence.read"),
        "{missing_error}"
    );
}

fn save_sample(path: &std::path::Path) {
    save_trigger_editor_project(path, &issue733_sample_project().expect("sample project"))
        .expect("save sample project");
}

fn rewrite_json(path: &std::path::Path, mutate: impl FnOnce(&mut Value)) {
    let mut json: Value =
        serde_json::from_slice(&fs::read(path).expect("read project")).expect("decode project");
    mutate(&mut json);
    fs::write(
        path,
        serde_json::to_vec_pretty(&json).expect("encode project"),
    )
    .expect("rewrite project");
}

fn project_path(label: &str) -> std::path::PathBuf {
    issue_scratch_dir("issue803_trigger_editor_persistence", label)
        .join("trigger-editor-project.json")
}
