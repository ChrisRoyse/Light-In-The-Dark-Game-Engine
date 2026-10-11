use super::{
    TriggerEditorUnit, TriggerEventPaletteKind, append_default_trigger_event, delete_trigger_event,
    fire_unit_dies, issue733_sample_project, replace_default_trigger_event,
    set_unit_dies_event_unit,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";
const ORIGINAL_EVENT_ID: &str = "evt_unit_dies";

#[test]
fn multiple_unit_events_match_with_authored_or_semantics() {
    let mut project = project_with_guard();
    let event =
        append_default_trigger_event(&mut project, TRIGGER_ID, TriggerEventPaletteKind::UnitDies)
            .expect("append second unit event");
    assert_eq!(event.node_id, "evt_unit_dies_1");
    assert_eq!(event.order, 2);
    assert!(
        set_unit_dies_event_unit(&mut project, TRIGGER_ID, &event.node_id, "unit_guard",)
            .expect("target guard")
    );

    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("first event matches")
            .fired
    );
    assert!(
        fire_unit_dies(&project, "unit_guard")
            .expect("second event matches")
            .fired
    );
}

#[test]
fn event_delete_returns_evidence_and_guards_final_event() {
    let mut project = issue733_sample_project().expect("sample project");
    let event = append_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::MapInitialization,
    )
    .expect("append map event");

    let removed = delete_trigger_event(&mut project, TRIGGER_ID, &event.node_id)
        .expect("delete second event");
    assert_eq!(removed, event);
    let before = project.clone();
    let error = delete_trigger_event(&mut project, TRIGGER_ID, ORIGINAL_EVENT_ID)
        .expect_err("final event is required");
    assert!(error.to_string().contains("cannot delete final event"));
    assert_eq!(project, before);
}

#[test]
fn invalid_event_lifecycle_edits_are_transactional() {
    let mut project = issue733_sample_project().expect("sample project");
    append_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::MapInitialization,
    )
    .expect("append event");
    let before = project.clone();
    let error = delete_trigger_event(&mut project, TRIGGER_ID, "missing_event")
        .expect_err("unknown event must fail");
    assert!(error.to_string().contains("unknown event node"));
    assert_eq!(project, before);

    project.graphs[0].events[1].order = project.graphs[0].events[0].order;
    let invalid_before = project.clone();
    let error = append_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::PeriodicTimer,
    )
    .expect_err("duplicate event order must fail");
    assert!(error.to_string().contains("duplicate event order"));
    assert_eq!(project, invalid_before);
}

#[test]
fn append_requires_a_complete_typed_default_before_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::MapInitialization,
    )
    .expect("remove unit event dependency");
    project.initial_units.clear();
    project.validate().expect("unit-free fixture is valid");
    let before = project.clone();

    let error =
        append_default_trigger_event(&mut project, TRIGGER_ID, TriggerEventPaletteKind::UnitDies)
            .expect_err("Unit dies requires an initial unit");

    assert!(error.to_string().contains("no initial unit"));
    assert_eq!(project, before);
}

fn project_with_guard() -> super::TriggerEditorProject {
    let mut project = issue733_sample_project().expect("sample project");
    let scout = project.initial_units["unit_scout"].clone();
    project.initial_units.insert(
        "unit_guard".to_string(),
        TriggerEditorUnit {
            unit_id: "unit_guard".to_string(),
            unit_type_id: "unit_guard_type".to_string(),
            owner_player_id: scout.owner_player_id,
            region_id: scout.region_id,
            alive: true,
            position: scout.position,
        },
    );
    project.validate().expect("guard fixture validates");
    project
}
