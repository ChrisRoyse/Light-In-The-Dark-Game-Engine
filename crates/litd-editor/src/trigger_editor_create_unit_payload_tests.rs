use super::{
    TriggerActionPaletteKind, TriggerCreateUnitPayload, append_default_trigger_action,
    fire_unit_dies, issue733_sample_project, set_create_unit_action_payload,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn create_unit_payload_edit_is_exact_and_executable() {
    let mut project = issue733_sample_project().expect("sample project");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect("append create-unit action");
    let payload = TriggerCreateUnitPayload {
        unit_id: "unit_authored_scout".to_string(),
        unit_type_id: "unit_scout_type".to_string(),
        owner_player_id: "player_one".to_string(),
        region_id: "reinforcement_point".to_string(),
        x: 16.0,
        y: 1.5,
        z: -4.0,
    };

    assert!(
        set_create_unit_action_payload(&mut project, TRIGGER_ID, &action.node_id, &payload)
            .expect("edit create-unit payload")
    );
    let edited = project.graphs[0]
        .actions
        .iter()
        .find(|candidate| candidate.node_id == action.node_id)
        .expect("edited action");
    assert_eq!(edited.args["unit_id"], "unit_authored_scout");
    assert_eq!(edited.args["unit_type_id"], "unit_scout_type");
    assert_eq!(edited.args["x"], "16");
    assert_eq!(edited.args["y"], "1.5");
    assert_eq!(edited.args["z"], "-4");
    assert!(
        !set_create_unit_action_payload(&mut project, TRIGGER_ID, &action.node_id, &payload)
            .expect("exact edit is a no-op")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("trigger executes");
    assert!(
        report
            .created_unit_ids
            .contains(&"unit_authored_scout".to_string())
    );
}

#[test]
fn create_unit_payload_rejects_invalid_references_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect("append create-unit action");
    let valid = payload("unit_valid");
    let candidates = [
        (
            TriggerCreateUnitPayload {
                unit_type_id: "missing_type".to_string(),
                ..valid.clone()
            },
            "unknown unit type",
        ),
        (
            TriggerCreateUnitPayload {
                owner_player_id: "missing_owner".to_string(),
                ..valid.clone()
            },
            "unknown initial unit owner",
        ),
        (
            TriggerCreateUnitPayload {
                region_id: "missing_region".to_string(),
                ..valid.clone()
            },
            "unknown region",
        ),
        (
            TriggerCreateUnitPayload {
                unit_id: "unit_scout".to_string(),
                ..valid.clone()
            },
            "duplicate authored unit id",
        ),
        (
            TriggerCreateUnitPayload {
                x: f32::NAN,
                ..valid
            },
            "finite coordinates",
        ),
    ];
    let before = project.clone();
    for (candidate, expected) in candidates {
        let error =
            set_create_unit_action_payload(&mut project, TRIGGER_ID, &action.node_id, &candidate)
                .expect_err("invalid payload must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(project, before);
    }
}

#[test]
fn create_unit_payload_rejects_wrong_action_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = set_create_unit_action_payload(
        &mut project,
        TRIGGER_ID,
        "act_wait_for_death_animation",
        &payload("unit_valid"),
    )
    .expect_err("wait action is not create-unit");

    assert!(error.to_string().contains("expected create_unit_at_point"));
    assert_eq!(project, before);
}

fn payload(unit_id: &str) -> TriggerCreateUnitPayload {
    TriggerCreateUnitPayload {
        unit_id: unit_id.to_string(),
        unit_type_id: "unit_guard_type".to_string(),
        owner_player_id: "player_one".to_string(),
        region_id: "reinforcement_point".to_string(),
        x: 12.0,
        y: 0.0,
        z: 8.0,
    }
}
