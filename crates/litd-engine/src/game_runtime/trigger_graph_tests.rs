//! #1851: unit coverage for the lowered ECA trigger graph dataset -- lowering,
//! fingerprinting, structural validation, and the blueprint cross-reference checks.

use super::super::blueprint::{AuthoredUnit, MatchBlueprint};
use super::super::groups::{OrderTargetKind, UnitCapability};
use super::super::players::{PlayerController, PlayerSlot};
use super::super::victory::VictoryCondition;
use super::{
    CompareOp, FlagOperand, IntOperand, LoweredTrigger, TriggerAction, TriggerCondition,
    TriggerEvent, TriggerValue, TriggerVarInit, lower_trigger_graph,
    validate_trigger_graph_against_blueprint,
};

fn var(name: &str, value: TriggerValue) -> TriggerVarInit {
    TriggerVarInit {
        name: name.to_string(),
        value,
    }
}

fn trigger(id: &str, events: Vec<TriggerEvent>, actions: Vec<TriggerAction>) -> LoweredTrigger {
    LoweredTrigger {
        trigger_id: id.to_string(),
        enabled: true,
        initially_on: true,
        events,
        conditions: Vec::new(),
        actions,
    }
}

fn sample_blueprint() -> MatchBlueprint {
    let slots = vec![
        PlayerSlot {
            id: 0,
            controller: PlayerController::Human,
            force: 0,
            color: 0,
            handicap_pct: 100,
        },
        PlayerSlot {
            id: 1,
            controller: PlayerController::Computer,
            force: 1,
            color: 1,
            handicap_pct: 100,
        },
    ];
    let mut blueprint = MatchBlueprint::new("trigger_map", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: false,
        capabilities: vec![UnitCapability {
            ability: 1,
            target_kind: OrderTargetKind::Point,
        }],
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 11,
        owner: 1,
        is_structure: false,
        capabilities: Vec::new(),
    });
    blueprint
}

#[test]
fn issue1851_lower_validate_roundtrip_and_lookup() {
    let graph = lower_trigger_graph(
        vec![
            var("mission_started", TriggerValue::Flag(false)),
            var("kills", TriggerValue::Integer(0)),
        ],
        vec![trigger(
            "trg_init",
            vec![TriggerEvent::MapInitialization],
            vec![TriggerAction::SetVariable {
                name: "mission_started".to_string(),
                value: TriggerValue::Flag(true),
            }],
        )],
    )
    .expect("lower");
    graph.validate().expect("valid");
    assert_eq!(graph.variables.len(), 2);
    assert!(graph.variable("kills").is_some());
    assert!(graph.variable("absent").is_none());
    assert_eq!(graph.graph_fingerprint.len(), 16);
}

#[test]
fn issue1851_tampered_graph_fails_fingerprint() {
    let mut graph = lower_trigger_graph(
        vec![var("kills", TriggerValue::Integer(0))],
        vec![trigger(
            "trg_kill",
            vec![TriggerEvent::UnitDies { unit: 11 }],
            vec![TriggerAction::FireTrigger { trigger: 7 }],
        )],
    )
    .expect("lower");
    // Sabotage the fired victory trigger WITHOUT re-deriving the fingerprint.
    if let TriggerAction::FireTrigger { trigger } = &mut graph.triggers[0].actions[0] {
        *trigger = 99;
    }
    let error = graph.validate().expect_err("tamper must be rejected");
    assert_eq!(error.code(), "LITD-REFLECT-FINGERPRINT-MISMATCH");
}

#[test]
fn issue1851_duplicate_variable_rejected() {
    let error = lower_trigger_graph(
        vec![
            var("kills", TriggerValue::Integer(0)),
            var("kills", TriggerValue::Integer(1)),
        ],
        Vec::new(),
    )
    .expect_err("duplicate variable must be rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn issue1851_set_variable_type_mismatch_rejected() {
    let error = lower_trigger_graph(
        vec![var("kills", TriggerValue::Integer(0))],
        vec![trigger(
            "trg_bad",
            vec![TriggerEvent::MapInitialization],
            vec![TriggerAction::SetVariable {
                name: "kills".to_string(),
                value: TriggerValue::Flag(true),
            }],
        )],
    )
    .expect_err("type mismatch must be rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn issue1851_undeclared_condition_variable_rejected() {
    let mut bad = trigger(
        "trg_cond",
        vec![TriggerEvent::MapInitialization],
        vec![TriggerAction::FireTrigger { trigger: 1 }],
    );
    bad.conditions.push(TriggerCondition::IntegerCompare {
        op: CompareOp::Equal,
        left: IntOperand::Variable("ghost".to_string()),
        right: IntOperand::Literal(1),
    });
    let error =
        lower_trigger_graph(Vec::new(), vec![bad]).expect_err("undeclared variable rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");

    let mut bad_flag = trigger(
        "trg_flag",
        vec![TriggerEvent::MapInitialization],
        vec![TriggerAction::FireTrigger { trigger: 1 }],
    );
    bad_flag.conditions.push(TriggerCondition::FlagEquals {
        left: FlagOperand::Variable("kills".to_string()),
        right: FlagOperand::Literal(true),
    });
    // "kills" declared as integer -> flag operand type mismatch.
    let error = lower_trigger_graph(vec![var("kills", TriggerValue::Integer(0))], vec![bad_flag])
        .expect_err("flag operand on integer variable rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn issue1851_zero_elapsed_and_empty_nodes_rejected() {
    let error = lower_trigger_graph(
        Vec::new(),
        vec![trigger(
            "trg_zero",
            vec![TriggerEvent::ElapsedGameTime { elapsed_ticks: 0 }],
            vec![TriggerAction::FireTrigger { trigger: 1 }],
        )],
    )
    .expect_err("zero elapsed rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");

    let error = lower_trigger_graph(
        Vec::new(),
        vec![trigger("trg_empty", Vec::new(), Vec::new())],
    )
    .expect_err("event-less/action-less trigger rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}

#[test]
fn issue1851_blueprint_cross_checks_fail_loud() {
    let blueprint = sample_blueprint();
    // Unknown unit reference.
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![trigger(
            "trg_ghost",
            vec![TriggerEvent::UnitDies { unit: 999 }],
            vec![TriggerAction::FireTrigger { trigger: 1 }],
        )],
    )
    .expect("structurally valid");
    let error = validate_trigger_graph_against_blueprint(&graph, &blueprint)
        .expect_err("unknown unit rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");

    // move_unit on a structure.
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![trigger(
            "trg_move_struct",
            vec![TriggerEvent::MapInitialization],
            vec![TriggerAction::MoveUnit {
                unit: 1,
                cell_x: 3,
                cell_z: 3,
            }],
        )],
    )
    .expect("structurally valid");
    assert!(validate_trigger_graph_against_blueprint(&graph, &blueprint).is_err());

    // order_unit on a unit with no point ability.
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![trigger(
            "trg_order_no_pt",
            vec![TriggerEvent::MapInitialization],
            vec![TriggerAction::OrderUnitMove {
                unit: 11,
                cell_x: 3,
                cell_z: 3,
            }],
        )],
    )
    .expect("structurally valid");
    assert!(validate_trigger_graph_against_blueprint(&graph, &blueprint).is_err());

    // A fully-resolvable graph passes.
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![trigger(
            "trg_ok",
            vec![TriggerEvent::UnitDies { unit: 11 }],
            vec![
                TriggerAction::KillUnit { unit: 10 },
                TriggerAction::OrderUnitMove {
                    unit: 10,
                    cell_x: 4,
                    cell_z: 4,
                },
            ],
        )],
    )
    .expect("structurally valid");
    validate_trigger_graph_against_blueprint(&graph, &blueprint).expect("resolvable graph passes");
}
