use litd_engine::EngineError;

use crate::{
    scenario::MapRuleset,
    starter_templates::{StarterTemplateDefinition, StarterTemplateKind},
};

use super::common::*;

pub(super) fn rpg_arena_template() -> Result<StarterTemplateDefinition, EngineError> {
    let kind = StarterTemplateKind::RpgArena;
    let mut document = base_document(kind, 16, 16)?;
    document.upsert_region(region("arena_ring", 8.0, 0.0, 8.0, 5.0))?;
    for object in [
        unit(
            "arena_hero",
            "hero",
            "player_one",
            "arena_ring",
            8.0,
            8.0,
            &[
                ("stat.health", "34"),
                ("stat.damage", "7"),
                ("stat.attack_period_ticks", "6"),
            ],
        ),
        unit(
            "arena_skeleton_01",
            "monster",
            "undead",
            "arena_ring",
            6.0,
            8.0,
            &[
                ("stat.health", "8"),
                ("stat.damage", "2"),
                ("stat.attack_period_ticks", "12"),
            ],
        ),
        unit(
            "arena_skeleton_02",
            "monster",
            "undead",
            "arena_ring",
            10.0,
            8.0,
            &[
                ("stat.health", "8"),
                ("stat.damage", "2"),
                ("stat.attack_period_ticks", "12"),
            ],
        ),
        unit(
            "arena_boss",
            "boss",
            "undead",
            "arena_ring",
            8.0,
            11.0,
            &[
                ("stat.health", "18"),
                ("stat.damage", "3"),
                ("stat.attack_period_ticks", "10"),
            ],
        ),
        item("arena_healing_font", 8.0, 6.0),
    ] {
        document.upsert_object(object)?;
    }
    add_quest(&mut document, "arena_objective", "arena_hero", "arena_boss")?;
    assign_models(&mut document)?;
    let graphs = vec![
        graph(
            "trg_arena_init",
            "Arena Objective",
            "Setup",
            vec![node("evt_init", "map_initialization", 1, &[])],
            vec![node(
                "cond_arena_live",
                "game_flag_true",
                1,
                &[("flag", "arena_live")],
            )],
            vec![sleep("act_show_quest", 1, "show arena quest")],
        ),
        graph(
            "trg_arena_boss_down",
            "Boss Down Reward",
            "Combat",
            vec![node(
                "evt_boss_down",
                "unit_dies",
                1,
                &[("unit_id", "arena_boss")],
            )],
            vec![],
            vec![create_unit(
                "act_spawn_font_guard",
                1,
                "arena_font_guard",
                "arena_guardian_type",
                "player_one",
                "arena_ring",
                8.0,
                0.0,
                6.0,
            )],
        ),
        graph(
            "trg_arena_victory",
            "Arena Victory",
            "Terminal",
            vec![node(
                "evt_victory",
                "game_victory",
                1,
                &[("player_id", "player_one")],
            )],
            vec![],
            vec![sleep("act_victory", 1, "show arena victory")],
        ),
    ];
    finish(
        kind,
        document,
        scenario(kind, 1, 1, MapRuleset::Custom)?,
        graphs,
        vec![
            tip(
                "Objects",
                "arena_hero",
                "Raise or lower hero damage to change fight length.",
                "Edit stat.damage from 7 to 8.",
            ),
            tip(
                "Objects",
                "arena_healing_font",
                "Move the healing font to change arena risk.",
                "Drag arena_healing_font behind the hero.",
            ),
            tip(
                "Triggers",
                "trg_arena_boss_down",
                "Open the boss reward trigger before adding loot.",
                "Change the spawned guard position.",
            ),
        ],
    )
}
