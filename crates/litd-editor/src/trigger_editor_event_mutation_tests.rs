use super::{
    TriggerEventPaletteKind, fire_unit_dies, issue733_sample_project,
    replace_default_trigger_event, replace_trigger_event_operation,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn event_palette_replaces_event_and_restores_unit_dies_defaults() {
    let mut project = issue733_sample_project().expect("sample project");
    assert!(
        !replace_default_trigger_event(
            &mut project,
            TRIGGER_ID,
            TriggerEventPaletteKind::UnitDies,
        )
        .expect("current Unit dies is a no-op")
    );
    assert!(
        replace_default_trigger_event(
            &mut project,
            TRIGGER_ID,
            TriggerEventPaletteKind::MapInitialization,
        )
        .expect("switch to Map initialization")
    );
    assert_eq!(
        project.graphs[0].events[0].node_id,
        "evt_map_initialization"
    );
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect_err("Map initialization does not accept Unit dies")
            .to_string()
            .contains("missing enabled unit_dies graph")
    );

    assert!(
        replace_default_trigger_event(&mut project, TRIGGER_ID, TriggerEventPaletteKind::UnitDies,)
            .expect("switch back to Unit dies")
    );
    let event = &project.graphs[0].events[0];
    assert_eq!(event.node_id, "evt_unit_dies");
    assert_eq!(event.order, 1);
    assert_eq!(event.args["unit_id"], "unit_scout");
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("Unit dies fires")
            .fired
    );
}

#[test]
fn region_event_palette_authors_stable_project_references() {
    let mut project = issue733_sample_project().expect("sample project");
    assert!(
        replace_default_trigger_event(
            &mut project,
            TRIGGER_ID,
            TriggerEventPaletteKind::UnitEntersRegion,
        )
        .expect("switch to Unit enters region")
    );

    let event = &project.graphs[0].events[0];
    assert_eq!(event.node_id, "evt_unit_enters_region");
    assert_eq!(event.operation, "unit_enters_region");
    assert_eq!(event.args["unit_id"], "unit_scout");
    assert_eq!(event.args["region_id"], "reinforcement_point");
    assert!(event.variable_refs.is_empty());
    project.validate().expect("region event project validates");
    assert!(
        !replace_default_trigger_event(
            &mut project,
            TRIGGER_ID,
            TriggerEventPaletteKind::UnitEntersRegion,
        )
        .expect("exact region event is a no-op")
    );
}

#[test]
fn unsupported_or_unknown_event_input_does_not_mutate() {
    let project = issue733_sample_project().expect("sample project");
    for (trigger_id, operation, expected) in [
        (
            TRIGGER_ID,
            "unsupported_event",
            "unsupported live event operation unsupported_event",
        ),
        (
            "missing_trigger",
            "unit_dies",
            "unknown trigger graph missing_trigger",
        ),
    ] {
        let mut candidate = project.clone();
        let error = replace_trigger_event_operation(&mut candidate, trigger_id, operation)
            .expect_err("invalid replacement must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(candidate, project);
    }
}

#[test]
fn unit_dies_without_default_unit_fails_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::MapInitialization,
    )
    .expect("switch to event with no unit reference");
    project.initial_units.clear();
    project
        .validate()
        .expect("Map initialization permits no initial unit");
    let before = project.clone();

    let error =
        replace_default_trigger_event(&mut project, TRIGGER_ID, TriggerEventPaletteKind::UnitDies)
            .expect_err("Unit dies needs a default unit");
    assert!(error.to_string().contains("no initial unit is available"));
    assert_eq!(project, before);
}
