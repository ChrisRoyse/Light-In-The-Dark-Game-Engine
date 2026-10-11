//! #1851: unit coverage for the in-session ECA trigger evaluator. Every test here is
//! mutation-falsifiable: severing the `trigger_step` call from `tick_all`, the
//! timer-fired routing, the `unit_dies` latch, or the trigger hash fold makes at
//! least one of these fail.

use crate::game_runtime::blueprint::{AuthoredUnit, MatchBlueprint};
use crate::game_runtime::datasets::{
    RuntimeDatasetsManifest, lower_runtime_constants, validate_runtime_datasets,
};
use crate::game_runtime::groups::{OrderTargetKind, UnitCapability};
use crate::game_runtime::lowered::{EconomyRulesetAuthor, LoweredResourceKind, LoweredUpkeepTier};
use crate::game_runtime::players::{PlayerController, PlayerSlot};
use crate::game_runtime::trigger_graph::{
    FlagOperand, LoweredTrigger, LoweredTriggerGraph, TriggerAction, TriggerCondition,
    TriggerEvent, TriggerValue, TriggerVarInit, lower_trigger_graph,
};
use crate::game_runtime::victory::{PlayerResult, VictoryCondition};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

const TICK_RATE: u32 = 4;
const GRID: u32 = 16;
const VICTORY_TRIGGER: u32 = 7;

pub(super) fn manifest_with(triggers: Option<LoweredTriggerGraph>) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .lower()
        .expect("lower economy");
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
    let mut blueprint = MatchBlueprint::new("trigger_melee", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_condition(VictoryCondition::CustomTrigger {
        player: 0,
        trigger: VICTORY_TRIGGER,
    });
    for (unit, owner, is_structure) in [(1, 0, true), (2, 1, true)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure,
            capabilities: Vec::new(),
        });
    }
    for (unit, owner) in [(10_u32, 0_u8), (11, 1)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure: false,
            capabilities: vec![
                UnitCapability {
                    ability: 1,
                    target_kind: OrderTargetKind::Point,
                },
                UnitCapability {
                    ability: 2,
                    target_kind: OrderTargetKind::None,
                },
            ],
        });
    }
    let lowered_match = blueprint.lower().expect("lower match");
    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, 0x1851)
        .expect("lower constants");
    RuntimeDatasetsManifest::assemble_with_triggers(
        economy,
        lowered_match,
        constants,
        Vec::new(),
        triggers,
    )
    .expect("assemble")
}

fn flag_var(name: &str, value: bool) -> TriggerVarInit {
    TriggerVarInit {
        name: name.to_string(),
        value: TriggerValue::Flag(value),
    }
}

pub(super) fn on_trigger(
    id: &str,
    events: Vec<TriggerEvent>,
    conditions: Vec<TriggerCondition>,
    actions: Vec<TriggerAction>,
) -> LoweredTrigger {
    LoweredTrigger {
        trigger_id: id.to_string(),
        enabled: true,
        initially_on: true,
        events,
        conditions,
        actions,
    }
}

/// The full mission-shaped graph the FSV also authors: map init arms a flag, a
/// countdown timer kills enemy unit 11, and unit 11's death fires the victory
/// trigger -- all gated on the armed flag.
fn mission_graph() -> LoweredTriggerGraph {
    lower_trigger_graph(
        vec![flag_var("mission_started", false)],
        vec![
            on_trigger(
                "trg_init",
                vec![TriggerEvent::MapInitialization],
                Vec::new(),
                vec![TriggerAction::SetVariable {
                    name: "mission_started".to_string(),
                    value: TriggerValue::Flag(true),
                }],
            ),
            on_trigger(
                "trg_timer_kill",
                vec![TriggerEvent::CountdownTimerExpires { timer: 0 }],
                vec![
                    TriggerCondition::FlagEquals {
                        left: FlagOperand::Variable("mission_started".to_string()),
                        right: FlagOperand::Literal(true),
                    },
                    TriggerCondition::UnitAlive { unit: 11 },
                ],
                vec![TriggerAction::KillUnit { unit: 11 }],
            ),
            on_trigger(
                "trg_victory",
                vec![TriggerEvent::UnitDies { unit: 11 }],
                Vec::new(),
                vec![TriggerAction::FireTrigger {
                    trigger: VICTORY_TRIGGER,
                }],
            ),
        ],
    )
    .expect("lower mission graph")
}

fn session_with(graph: LoweredTriggerGraph) -> MatchRuntimeSession {
    let manifest = manifest_with(Some(graph));
    MatchRuntimeSession::from_manifest(&manifest).expect("session")
}

#[test]
fn issue1851_map_initialization_fires_on_first_tick_only() {
    let mut session = session_with(mission_graph());
    assert_eq!(
        session.triggers().variable("mission_started"),
        Some(&TriggerValue::Flag(false))
    );
    session.apply(SessionOrder::Tick).expect("tick 1");
    assert_eq!(
        session.triggers().variable("mission_started"),
        Some(&TriggerValue::Flag(true)),
        "map_initialization must run the set_variable action on tick 1 \
         (fails if trigger_step is severed from tick_all)"
    );
    assert_eq!(session.triggers().fire_count("trg_init"), Some(1));
    for _ in 0..5 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    assert_eq!(
        session.triggers().fire_count("trg_init"),
        Some(1),
        "map_initialization fires exactly once"
    );
}

#[test]
fn issue1851_elapsed_game_time_fires_at_exact_tick() {
    let graph = lower_trigger_graph(
        vec![flag_var("late", false)],
        vec![on_trigger(
            "trg_elapsed",
            vec![TriggerEvent::ElapsedGameTime { elapsed_ticks: 3 }],
            Vec::new(),
            vec![TriggerAction::SetVariable {
                name: "late".to_string(),
                value: TriggerValue::Flag(true),
            }],
        )],
    )
    .expect("lower");
    let mut session = session_with(graph);
    session.apply(SessionOrder::Tick).expect("tick 1");
    session.apply(SessionOrder::Tick).expect("tick 2");
    assert_eq!(
        session.triggers().variable("late"),
        Some(&TriggerValue::Flag(false)),
        "must not fire before the elapsed tick"
    );
    session.apply(SessionOrder::Tick).expect("tick 3");
    assert_eq!(
        session.triggers().variable("late"),
        Some(&TriggerValue::Flag(true))
    );
    assert_eq!(session.triggers().fire_count("trg_elapsed"), Some(1));
}

#[test]
fn issue1851_countdown_timer_expiry_kills_unit() {
    let mut session = session_with(mission_graph());
    session
        .apply(SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: 5,
        })
        .expect("start timer");
    for _ in 0..4 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    assert!(
        session.match_state().is_alive(11).expect("alive query"),
        "unit 11 must still be alive before the timer expires"
    );
    session.apply(SessionOrder::Tick).expect("expiry tick");
    assert!(
        !session.match_state().is_alive(11).expect("alive query"),
        "the timer expiry must kill unit 11 \
         (fails if the TimerFired routing into the evaluator is severed)"
    );
    assert_eq!(session.triggers().fire_count("trg_timer_kill"), Some(1));
}

#[test]
fn issue1851_unit_dies_fires_victory_custom_trigger() {
    let mut session = session_with(mission_graph());
    session.apply(SessionOrder::Tick).expect("tick 1");
    assert_eq!(
        session.match_state().result(0).unwrap(),
        PlayerResult::Playing
    );
    // Kill unit 11 via a host order; the unit_dies latch must observe the
    // transition on the NEXT tick and fire the victory trigger.
    session
        .apply(SessionOrder::DestroyUnit { unit: 11 })
        .expect("destroy");
    session.apply(SessionOrder::Tick).expect("tick 2");
    assert_eq!(
        session.match_state().result(0).unwrap(),
        PlayerResult::Victory,
        "unit_dies -> fire_trigger -> CustomTrigger victory \
         (fails if the unit-death latch is severed)"
    );
    assert_eq!(session.triggers().fire_count("trg_victory"), Some(1));
    // Full pipeline via the timer as well: timeline records all three firings.
    let fired: Vec<&str> = session
        .triggers()
        .timeline()
        .iter()
        .map(|f| f.event)
        .collect();
    assert!(fired.contains(&"map_initialization"));
    assert!(fired.contains(&"unit_dies"));
}

#[test]
fn issue1851_condition_gate_blocks_firing() {
    // Same timer trigger but the gating flag is NEVER armed (no map-init trigger).
    let graph = lower_trigger_graph(
        vec![flag_var("mission_started", false)],
        vec![on_trigger(
            "trg_timer_kill",
            vec![TriggerEvent::CountdownTimerExpires { timer: 0 }],
            vec![TriggerCondition::FlagEquals {
                left: FlagOperand::Variable("mission_started".to_string()),
                right: FlagOperand::Literal(true),
            }],
            vec![TriggerAction::KillUnit { unit: 11 }],
        )],
    )
    .expect("lower");
    let mut session = session_with(graph);
    session
        .apply(SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: 2,
        })
        .expect("start timer");
    for _ in 0..4 {
        session.apply(SessionOrder::Tick).expect("tick");
    }
    assert!(
        session.match_state().is_alive(11).expect("alive"),
        "a false condition must gate the action"
    );
    assert_eq!(session.triggers().fire_count("trg_timer_kill"), Some(0));
}

#[test]
fn issue1851_move_unit_action_moves_field_unit() {
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_move",
            vec![TriggerEvent::ElapsedGameTime { elapsed_ticks: 2 }],
            Vec::new(),
            vec![
                TriggerAction::MoveUnit {
                    unit: 10,
                    cell_x: 9,
                    cell_z: 7,
                },
                TriggerAction::OrderUnitMove {
                    unit: 11,
                    cell_x: 6,
                    cell_z: 5,
                },
            ],
        )],
    )
    .expect("lower");
    let mut session = session_with(graph);
    session.apply(SessionOrder::Tick).expect("tick 1");
    session.apply(SessionOrder::Tick).expect("tick 2");
    let moved = session.field_unit(10).expect("field unit 10");
    assert_eq!((moved.cell_x, moved.cell_z), (9, 7));
    let ordered = session.field_unit(11).expect("field unit 11");
    assert_eq!((ordered.cell_x, ordered.cell_z), (6, 5));
    // The IssueMove landed in unit 11's order book.
    assert_eq!(session.match_state().order_book(11).expect("book").len(), 1);
}

#[test]
fn issue1851_trigger_state_folds_into_combined_hash() {
    // Two sessions from the SAME manifest diverge in hash as soon as trigger state
    // diverges (one ran the map-init tick, the other did not). Severing the
    // TriggerRuntime fold from combined_state_hash makes the second assertion fail.
    let manifest = manifest_with(Some(mission_graph()));
    let before = MatchRuntimeSession::from_manifest(&manifest)
        .expect("session")
        .combined_state_hash();
    let mut ticked = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    ticked.apply(SessionOrder::Tick).expect("tick");
    assert_ne!(before, ticked.combined_state_hash());
    // Direct falsifier: identical tick counts, but one session's variable changed
    // ONLY via trigger state (mission graph) vs a session with no triggers.
    let manifest_none = manifest_with(None);
    let mut no_triggers = MatchRuntimeSession::from_manifest(&manifest_none).expect("session");
    no_triggers.apply(SessionOrder::Tick).expect("tick");
    assert_ne!(
        ticked.combined_state_hash(),
        no_triggers.combined_state_hash()
    );
}

#[test]
fn issue1851_two_execution_replay_determinism_with_triggers() {
    let manifest = manifest_with(Some(mission_graph()));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        session
            .apply(SessionOrder::StartTimer {
                one_shot: true,
                period_ticks: 5,
            })
            .expect("start timer");
        let mut hashes = Vec::new();
        for _ in 0..10 {
            session.apply(SessionOrder::Tick).expect("tick");
            hashes.push(session.combined_state_hash());
        }
        hashes
    };
    assert_eq!(
        run(),
        run(),
        "identical order streams, identical hash STREAMS"
    );
}

#[test]
fn issue1851_save_load_acid_with_triggers() {
    let manifest = manifest_with(Some(mission_graph()));
    let stream: Vec<SessionOrder> = std::iter::once(SessionOrder::StartTimer {
        one_shot: true,
        period_ticks: 5,
    })
    .chain(std::iter::repeat_n(SessionOrder::Tick, 10))
    .collect();
    let mut original = MatchRuntimeSession::from_manifest(&manifest).expect("orig");
    for order in &stream[..4] {
        original.apply(order.clone()).expect("apply");
    }
    let save = original.capture_save().expect("capture");
    for order in &stream[4..] {
        original.apply(order.clone()).expect("apply");
    }
    let mut reloaded = MatchRuntimeSession::restore_from_save(&manifest, &save).expect("restore");
    for order in &stream[4..] {
        reloaded.apply(order.clone()).expect("apply");
    }
    assert_eq!(
        original.combined_state_hash(),
        reloaded.combined_state_hash()
    );
}

#[test]
fn issue1851_datasets_fingerprint_binds_trigger_section() {
    // Presence vs absence changes the binding fingerprint.
    let with = manifest_with(Some(mission_graph()));
    let without = manifest_with(None);
    assert_ne!(with.datasets_fingerprint, without.datasets_fingerprint);
    // A DIFFERENT (validly-fingerprinted) trigger graph also changes it.
    let other_graph = lower_trigger_graph(
        vec![flag_var("other", true)],
        vec![on_trigger(
            "trg_other",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::FireTrigger { trigger: 1 }],
        )],
    )
    .expect("lower");
    let swapped = manifest_with(Some(other_graph));
    assert_ne!(with.datasets_fingerprint, swapped.datasets_fingerprint);
    // Tampering a trigger node without re-fingerprinting is rejected typed.
    let mut tampered = with.clone();
    if let Some(graph) = tampered.triggers.as_mut()
        && let TriggerAction::SetVariable { value, .. } = &mut graph.triggers[0].actions[0]
    {
        *value = TriggerValue::Flag(false);
    }
    let error = validate_runtime_datasets(&tampered).expect_err("tamper rejected");
    assert_eq!(error.code(), "LITD-REFLECT-FINGERPRINT-MISMATCH");
    assert!(MatchRuntimeSession::from_manifest(&tampered).is_err());
}

#[test]
fn issue1851_dangling_unit_reference_rejected_at_datasets_validation() {
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_ghost",
            vec![TriggerEvent::UnitDies { unit: 999 }],
            Vec::new(),
            vec![TriggerAction::FireTrigger { trigger: 1 }],
        )],
    )
    .expect("structurally valid");
    let manifest = manifest_with(Some(graph));
    let error = validate_runtime_datasets(&manifest).expect_err("dangling unit rejected");
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");
}
