use std::error::Error;

use litd_engine::game_runtime::{
    AuthoredUnit, EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredTrigger,
    LoweredUpkeepTier, MatchBlueprint, MatchRuntimeSession, PlayerController, PlayerSlot,
    RuntimeDatasetsManifest, SessionOrder, TriggerAction, TriggerEvent, VictoryCondition,
    lower_runtime_constants, lower_trigger_graph,
};

pub fn run() -> Result<u64, Box<dyn Error>> {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 0,
            income_numerator: 1,
            income_denominator: 1,
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 1,
            cell_z: 0,
            amount_total: 100,
            yield_per_trip: 10,
        })
        .lower()?;
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
    let mut blueprint = MatchBlueprint::new("routine_economy", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    for (unit, owner) in [(1, 0), (2, 1)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure: true,
            capabilities: Vec::new(),
        });
    }
    let triggers = lower_trigger_graph(
        Vec::new(),
        vec![LoweredTrigger {
            trigger_id: "routine_economy_start".to_string(),
            enabled: true,
            initially_on: true,
            events: vec![TriggerEvent::MapInitialization],
            conditions: Vec::new(),
            actions: vec![
                TriggerAction::GrantResource {
                    key: "gold".to_string(),
                    amount: 250,
                },
                TriggerAction::SpendResource {
                    key: "gold".to_string(),
                    amount: 100,
                },
                TriggerAction::AddWorker {
                    worker_id: 77,
                    node_id: "gold_mine".to_string(),
                },
            ],
        }],
    )?;
    let manifest = RuntimeDatasetsManifest::assemble_with_triggers(
        economy,
        blueprint.lower()?,
        lower_runtime_constants(4, 200, 6_000, 500, 1_000, 16, 16, 0x804)?,
        Vec::new(),
        Some(triggers),
    )?;
    let mut session = MatchRuntimeSession::from_manifest(&manifest)?;
    session.apply(SessionOrder::Tick)?;
    if session.economy().accounts().balance("gold") != 1_150
        || session.economy().worker(77).is_none()
    {
        return Err("authored economy trigger readback mismatch".into());
    }
    session.apply(SessionOrder::Tick)?;
    session.apply(SessionOrder::Tick)?;
    session.apply(SessionOrder::Tick)?;
    if session.economy().accounts().balance("gold") != 1_160
        || session.economy().node_remaining("gold_mine") != 90
    {
        return Err("authored worker harvest readback mismatch".into());
    }
    Ok(session.combined_state_hash())
}
