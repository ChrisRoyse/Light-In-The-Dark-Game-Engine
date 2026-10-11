use super::{issue733_sample_project, set_trigger_graph_metadata};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn graph_metadata_edit_and_clear_are_transactional() {
    let mut project = issue733_sample_project().expect("sample project");
    let before_fingerprint = project.stable_fingerprint().expect("before fingerprint");

    assert!(
        set_trigger_graph_metadata(
            &mut project,
            TRIGGER_ID,
            "Reinforcement Response",
            Some("Combat/Bosses"),
        )
        .expect("edit metadata")
    );
    assert_eq!(project.graphs[0].display_name, "Reinforcement Response");
    assert_eq!(
        project.graphs[0].folder_path.as_deref(),
        Some("Combat/Bosses")
    );
    assert_ne!(
        project.stable_fingerprint().expect("after fingerprint"),
        before_fingerprint
    );
    assert!(
        !set_trigger_graph_metadata(
            &mut project,
            TRIGGER_ID,
            "Reinforcement Response",
            Some("Combat/Bosses"),
        )
        .expect("exact metadata is a no-op")
    );
    assert!(
        set_trigger_graph_metadata(&mut project, TRIGGER_ID, "Reinforcement Response", None)
            .expect("clear folder")
    );
    assert_eq!(project.graphs[0].folder_path, None);
}

#[test]
fn invalid_graph_metadata_fails_without_mutation() {
    let project = issue733_sample_project().expect("sample project");
    for (trigger_id, display_name, folder, expected) in [
        (TRIGGER_ID, "", Some("Combat"), "bounded single-line"),
        (
            TRIGGER_ID,
            "Valid",
            Some("Combat//Bosses"),
            "slash-delimited",
        ),
        (
            TRIGGER_ID,
            "Valid",
            Some(" Combat/Bosses"),
            "slash-delimited",
        ),
        (
            "missing_trigger",
            "Valid",
            Some("Combat"),
            "unknown trigger graph",
        ),
    ] {
        let mut candidate = project.clone();
        let error = set_trigger_graph_metadata(&mut candidate, trigger_id, display_name, folder)
            .expect_err("invalid metadata must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(candidate, project);
    }
}

#[test]
fn project_validation_rejects_invalid_persisted_folder_paths() {
    let mut project = issue733_sample_project().expect("sample project");
    project.graphs[0].folder_path = Some("Combat\\Bosses".to_string());

    let error = project.validate().expect_err("backslash folder must fail");

    assert!(error.to_string().contains("slash-delimited"));
}
