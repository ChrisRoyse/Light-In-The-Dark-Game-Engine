use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[test]
fn condition_catalog_covers_wc3_style_categories() {
    validate_condition_catalog_complete().unwrap();
    let categories = condition_catalog()
        .iter()
        .map(|condition| condition.category)
        .collect::<BTreeSet<_>>();
    assert_eq!(categories.len(), REQUIRED_CONDITION_CATEGORIES.len());
    assert!(condition_catalog().len() >= 16);
    assert!(find_condition("integer_greater_equal").is_ok());
    assert!(find_condition("player_resource_at_least").is_ok());
}

#[test]
fn comparison_and_boolean_conditions_evaluate_known_values() {
    let context = synthetic_context();
    assert!(
        evaluate_condition(
            "integer_greater_equal",
            &args([("left", "125"), ("right", "100")]),
            &context
        )
        .unwrap()
    );
    assert!(
        !evaluate_condition(
            "boolean_and",
            &args([("left", "true"), ("right", "false")]),
            &context
        )
        .unwrap()
    );
}

#[test]
fn state_conditions_evaluate_known_context() {
    let context = synthetic_context();
    assert!(
        evaluate_condition(
            "unit_alive",
            &args([("unit_id", "unit_guard_01")]),
            &context
        )
        .unwrap()
    );
    assert!(
        evaluate_condition(
            "unit_in_region",
            &args([
                ("unit_id", "unit_guard_01"),
                ("region_id", "region_gate_01")
            ]),
            &context
        )
        .unwrap()
    );
    assert!(
        evaluate_condition(
            "player_resource_at_least",
            &args([
                ("player_id", "player_1"),
                ("resource_kind", "gold"),
                ("minimum", "100")
            ]),
            &context
        )
        .unwrap()
    );
}

#[test]
fn condition_arg_validation_fails_closed_for_unknown_condition() {
    let error = find_condition("unit_is_secretly_a_tree").unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("unknown trigger condition"));
}

#[test]
fn condition_arg_validation_fails_closed_for_missing_required_arg() {
    let error = validate_condition_args("unit_alive", &BTreeMap::new()).unwrap_err();
    assert!(error.to_string().contains("missing required arg unit_id"));
}

#[test]
fn condition_arg_validation_fails_closed_for_unknown_arg() {
    let error = validate_condition_args(
        "unit_alive",
        &args([("unit_id", "unit_guard_01"), ("surprise", "nope")]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown arg surprise"));
}

#[test]
fn condition_evaluation_fails_closed_for_missing_state() {
    let error = evaluate_condition(
        "unit_alive",
        &args([("unit_id", "unit_unknown_99")]),
        &synthetic_context(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown unit unit_unknown_99"));
}

fn args<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn synthetic_context() -> TriggerConditionContext {
    let mut resources = BTreeMap::new();
    resources.insert("gold".to_string(), 125);
    let mut player_flags = BTreeMap::new();
    player_flags.insert("quest_started".to_string(), true);
    let mut players = BTreeMap::new();
    players.insert(
        "player_1".to_string(),
        TriggerPlayerState {
            resources,
            flags: player_flags,
        },
    );
    let mut units = BTreeMap::new();
    units.insert(
        "unit_guard_01".to_string(),
        TriggerUnitState {
            alive: true,
            unit_type_id: "guard".to_string(),
            owner_player_id: "player_1".to_string(),
            region_id: "region_gate_01".to_string(),
        },
    );
    let mut game_flags = BTreeMap::new();
    game_flags.insert("night".to_string(), true);
    TriggerConditionContext {
        units,
        players,
        game: TriggerGameState {
            flags: game_flags,
            difficulty: "hard".to_string(),
        },
    }
}
