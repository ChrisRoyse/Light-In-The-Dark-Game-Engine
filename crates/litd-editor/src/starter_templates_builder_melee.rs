use litd_engine::EngineError;

use crate::{
    scenario::MapRuleset,
    starter_templates::{StarterTemplateDefinition, StarterTemplateKind},
};

use super::common::*;

pub(super) fn melee_skirmish_template() -> Result<StarterTemplateDefinition, EngineError> {
    let kind = StarterTemplateKind::MeleeSkirmish;
    let mut document = base_document(kind, 18, 14)?;
    document.upsert_region(region("melee_north_start", 4.0, 0.0, 3.0, 2.0))?;
    document.upsert_region(region("melee_south_start", 14.0, 0.0, 10.0, 2.0))?;
    for object in [
        unit(
            "melee_blue_footman_01",
            "army",
            "player_one",
            "melee_north_start",
            4.0,
            3.0,
            &[
                ("stat.health", "18"),
                ("stat.damage", "5"),
                ("stat.attack_period_ticks", "8"),
            ],
        ),
        unit(
            "melee_blue_footman_02",
            "army",
            "player_one",
            "melee_north_start",
            5.0,
            3.0,
            &[
                ("stat.health", "18"),
                ("stat.damage", "5"),
                ("stat.attack_period_ticks", "8"),
            ],
        ),
        unit(
            "melee_red_grunt_01",
            "army",
            "player_two",
            "melee_south_start",
            13.0,
            10.0,
            &[
                ("stat.health", "12"),
                ("stat.damage", "3"),
                ("stat.attack_period_ticks", "10"),
            ],
        ),
        unit(
            "melee_red_grunt_02",
            "army",
            "player_two",
            "melee_south_start",
            14.0,
            10.0,
            &[
                ("stat.health", "12"),
                ("stat.damage", "3"),
                ("stat.attack_period_ticks", "10"),
            ],
        ),
    ] {
        document.upsert_object(object)?;
    }
    add_quest(
        &mut document,
        "melee_objective",
        "melee_blue_footman_01",
        "melee_red_grunt_02",
    )?;
    assign_models(&mut document)?;
    let graphs = vec![
        graph(
            "trg_melee_init",
            "Melee Opening",
            "Setup",
            vec![node("evt_init", "map_initialization", 1, &[])],
            vec![node(
                "cond_opening",
                "game_flag_true",
                1,
                &[("flag", "melee_opening_enabled")],
            )],
            vec![sleep("act_opening_orders", 1, "show opening orders")],
        ),
        graph(
            "trg_melee_grunt_down",
            "Melee Reinforcement Banner",
            "Combat",
            vec![node(
                "evt_grunt_down",
                "unit_dies",
                1,
                &[("unit_id", "melee_red_grunt_01")],
            )],
            vec![],
            vec![create_unit(
                "act_banner",
                1,
                "melee_banner",
                "melee_banner_type",
                "player_one",
                "melee_north_start",
                4.0,
                0.0,
                3.0,
            )],
        ),
        graph(
            "trg_melee_victory",
            "Melee Victory",
            "Terminal",
            vec![node(
                "evt_victory",
                "game_victory",
                1,
                &[("player_id", "player_one")],
            )],
            vec![],
            vec![sleep("act_victory", 1, "show melee victory")],
        ),
    ];
    finish(
        kind,
        document,
        scenario(kind, 2, 2, MapRuleset::Melee)?,
        graphs,
        vec![
            tip(
                "Objects",
                "melee_blue_footman_01",
                "Tune a unit's damage and replay the duel.",
                "Set stat.damage to 6.",
            ),
            tip(
                "Terrain",
                "melee_north_start",
                "Move a start region to change first contact.",
                "Shift melee_north_start one tile east.",
            ),
            tip(
                "Triggers",
                "trg_melee_init",
                "Read the opening trigger before adding tech goals.",
                "Add a second setup action.",
            ),
        ],
    )
}
