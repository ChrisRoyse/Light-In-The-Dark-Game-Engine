use litd_engine::EngineError;

use crate::{
    scenario::MapRuleset,
    starter_templates::{StarterTemplateDefinition, StarterTemplateKind},
};

use super::common::*;

pub(super) fn tower_defense_template() -> Result<StarterTemplateDefinition, EngineError> {
    let kind = StarterTemplateKind::TowerDefense;
    let mut document = base_document(kind, 20, 12)?;
    document.upsert_region(region("td_lane_start", 1.0, 0.0, 5.0, 2.0))?;
    document.upsert_region(region("td_keep", 18.0, 0.0, 5.0, 2.0))?;
    for object in [
        unit(
            "td_base",
            "base",
            "player_one",
            "td_keep",
            18.0,
            5.0,
            &[("stat.health", "12")],
        ),
        unit(
            "td_tower_alpha",
            "tower",
            "player_one",
            "td_keep",
            7.0,
            3.0,
            &[
                ("stat.health", "20"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "5"),
            ],
        ),
        unit(
            "td_tower_beta",
            "tower",
            "player_one",
            "td_keep",
            11.0,
            7.0,
            &[
                ("stat.health", "20"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "5"),
            ],
        ),
        unit(
            "td_creep_01",
            "creep",
            "invaders",
            "td_lane_start",
            1.0,
            5.0,
            &[
                ("stat.health", "12"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
        unit(
            "td_creep_02",
            "creep",
            "invaders",
            "td_lane_start",
            2.0,
            5.0,
            &[
                ("stat.health", "12"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
        unit(
            "td_creep_03",
            "creep",
            "invaders",
            "td_lane_start",
            3.0,
            5.0,
            &[
                ("stat.health", "12"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
    ] {
        document.upsert_object(object)?;
    }
    add_quest(&mut document, "td_objective", "td_base", "td_creep_03")?;
    assign_models(&mut document)?;
    let graphs = vec![
        graph(
            "trg_td_map_init",
            "Tower Defense Setup",
            "Setup",
            vec![node("evt_init", "map_initialization", 1, &[])],
            vec![node(
                "cond_wave_ready",
                "game_flag_true",
                1,
                &[("flag", "td_wave_ready")],
            )],
            vec![sleep(
                "act_show_objective",
                1,
                "show tower defense objective",
            )],
        ),
        graph(
            "trg_td_creep_down",
            "Creep Bounty",
            "Combat/Rewards",
            vec![node(
                "evt_creep_down",
                "unit_dies",
                1,
                &[("unit_id", "td_creep_01")],
            )],
            vec![node(
                "cond_rewards",
                "game_flag_true",
                1,
                &[("flag", "td_rewards_enabled")],
            )],
            vec![create_unit(
                "act_spawn_guard",
                1,
                "td_bonus_guard",
                "td_guardian_type",
                "player_one",
                "td_keep",
                18.0,
                0.0,
                5.0,
            )],
        ),
        graph(
            "trg_td_victory",
            "Victory Banner",
            "Terminal",
            vec![node(
                "evt_victory",
                "game_victory",
                1,
                &[("player_id", "player_one")],
            )],
            vec![],
            vec![sleep("act_victory_banner", 1, "show victory banner")],
        ),
    ];
    finish(
        kind,
        document,
        scenario(kind, 1, 1, MapRuleset::Custom)?,
        graphs,
        vec![
            tip(
                "Terrain",
                "td_lane_start",
                "Move the lane start to reshape the opening creep path.",
                "Drag td_lane_start and replay.",
            ),
            tip(
                "Objects",
                "td_tower_alpha",
                "Change this tower's damage to learn balancing.",
                "Edit stat.damage from 4 to 5.",
            ),
            tip(
                "Triggers",
                "trg_td_creep_down",
                "Inspect the reward trigger before adding another wave.",
                "Duplicate the bounty action.",
            ),
        ],
    )
}
