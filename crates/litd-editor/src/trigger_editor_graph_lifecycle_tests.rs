use super::{
    append_default_trigger_graph, delete_trigger_graph, issue733_sample_project,
    read_trigger_editor_project, save_trigger_editor_project,
};

#[test]
fn graph_creation_is_valid_deterministic_and_persistent() {
    let mut project = issue733_sample_project().expect("sample project");

    let first = append_default_trigger_graph(&mut project).expect("first graph");
    let second = append_default_trigger_graph(&mut project).expect("second graph");

    assert_eq!(first.trigger_id, "trg_authored_1");
    assert_eq!(second.trigger_id, "trg_authored_2");
    assert_eq!(first.events[0].operation, "map_initialization");
    assert_eq!(first.actions[0].operation, "sleep");
    assert!(first.conditions.is_empty());
    project.validate().expect("authored graphs validate");

    let root = litd_engine::test_temp::TestTempDir::new("trigger-graph-lifecycle");
    let path = root.path().join("project.json");
    save_trigger_editor_project(&path, &project).expect("save");
    let reopened = read_trigger_editor_project(&path).expect("reopen");
    assert_eq!(reopened.project, project);
}

#[test]
fn graph_delete_returns_evidence_and_allows_safe_id_reuse() {
    let mut project = issue733_sample_project().expect("sample project");
    let created = append_default_trigger_graph(&mut project).expect("create");

    let removed =
        delete_trigger_graph(&mut project, &created.trigger_id).expect("delete authored graph");

    assert_eq!(removed, created);
    let replacement = append_default_trigger_graph(&mut project).expect("replace");
    assert_eq!(
        replacement.trigger_id, created.trigger_id,
        "no valid editor node can retain a deleted graph reference"
    );
}

#[test]
fn final_and_unknown_graph_deletes_fail_without_mutation() {
    let project = issue733_sample_project().expect("sample project");
    for (trigger_id, expected) in [
        (
            "trg_unit_death_reinforcement",
            "cannot delete final trigger graph",
        ),
        ("missing_trigger", "unknown trigger graph"),
    ] {
        let mut candidate = project.clone();
        let error =
            delete_trigger_graph(&mut candidate, trigger_id).expect_err("invalid delete must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(candidate, project);
    }
}

#[test]
fn project_validation_rejects_duplicate_trigger_ids() {
    let mut project = issue733_sample_project().expect("sample project");
    project.graphs.push(project.graphs[0].clone());
    let before = project.clone();

    let error = append_default_trigger_graph(&mut project)
        .expect_err("duplicate trigger IDs must fail before append");

    assert!(error.to_string().contains("duplicate trigger graph"));
    assert_eq!(project, before);
}
