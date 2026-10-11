use super::{
    TriggerEventPaletteKind, append_default_integer_greater_equal_condition, fire_unit_dies,
    issue733_sample_project, replace_default_trigger_event, set_integer_greater_equal_payload,
    set_time_event_seconds,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn time_event_payloads_edit_canonically_and_noop_exact_values() {
    let mut project = issue733_sample_project().expect("sample project");
    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::ElapsedGameTime,
    )
    .expect("author elapsed event");
    assert_eq!(project.graphs[0].events[0].args["elapsed_seconds"], "30");
    assert!(
        set_time_event_seconds(&mut project, TRIGGER_ID, "evt_elapsed_game_time", 45.5)
            .expect("edit elapsed time")
    );
    assert_eq!(project.graphs[0].events[0].args["elapsed_seconds"], "45.5");
    assert!(
        !set_time_event_seconds(&mut project, TRIGGER_ID, "evt_elapsed_game_time", 45.5)
            .expect("exact elapsed time is a no-op")
    );

    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::PeriodicTimer,
    )
    .expect("author periodic event");
    assert_eq!(project.graphs[0].events[0].args["period_seconds"], "1");
    assert!(
        set_time_event_seconds(&mut project, TRIGGER_ID, "evt_periodic_timer", 2.25)
            .expect("edit timer period")
    );
    assert_eq!(project.graphs[0].events[0].args["period_seconds"], "2.25");
}

#[test]
fn time_event_payloads_reject_invalid_values_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::ElapsedGameTime,
    )
    .expect("author elapsed event");
    let before = project.clone();
    for seconds in [-1.0, f32::NAN, f32::INFINITY] {
        let error =
            set_time_event_seconds(&mut project, TRIGGER_ID, "evt_elapsed_game_time", seconds)
                .expect_err("invalid elapsed time must fail");
        assert!(error.to_string().contains("must be finite"));
        assert_eq!(project, before);
    }

    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::PeriodicTimer,
    )
    .expect("author periodic event");
    let before = project.clone();
    let error = set_time_event_seconds(&mut project, TRIGGER_ID, "evt_periodic_timer", 0.0)
        .expect_err("zero period must fail");
    assert!(error.to_string().contains("timer period"));
    assert_eq!(project, before);
}

#[test]
fn integer_comparison_payload_changes_evaluated_result() {
    let mut project = issue733_sample_project().expect("sample project");
    let condition = append_default_integer_greater_equal_condition(&mut project, TRIGGER_ID)
        .expect("append integer comparison");
    assert_eq!(condition.node_id, "cond_integer_greater_equal_1");
    assert_eq!(condition.operation, "integer_greater_equal");
    assert_eq!(condition.args["left"], "0");
    assert_eq!(condition.args["right"], "0");
    assert!(
        set_integer_greater_equal_payload(&mut project, TRIGGER_ID, &condition.node_id, -1, 3)
            .expect("edit integer comparison")
    );
    assert!(
        !set_integer_greater_equal_payload(&mut project, TRIGGER_ID, &condition.node_id, -1, 3)
            .expect("exact comparison is a no-op")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("comparison evaluates");
    assert_eq!(report.condition_results, vec![true, false]);
    assert!(!report.fired);
}

#[test]
fn integer_comparison_rejects_wrong_condition_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = set_integer_greater_equal_payload(
        &mut project,
        TRIGGER_ID,
        "cond_reinforcements_enabled",
        10,
        5,
    )
    .expect_err("game flag is not an integer comparison");
    assert!(error.to_string().contains("expected integer_greater_equal"));
    assert_eq!(project, before);
}
