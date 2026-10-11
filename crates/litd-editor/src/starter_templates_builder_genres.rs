use litd_engine::EngineError;

use crate::{
    scenario::MapRuleset,
    starter_templates::{StarterTemplateDefinition, StarterTemplateKind},
};

use super::{common::*, genres_common::*};

pub(super) fn orpg_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena_like(
        StarterTemplateKind::Orpg,
        "orpg_silverleaf_grove",
        24,
        18,
        [
            hero("orpg_hero", "player_one", 8.0, 8.0, "38", "7", "6"),
            hero("orpg_companion", "player_one", 9.0, 8.0, "22", "4", "8"),
            monster("orpg_wisp_01", "wildwood", 13.0, 8.0, "10", "2", "12"),
            monster("orpg_wisp_02", "wildwood", 14.0, 9.0, "10", "2", "12"),
            boss("orpg_elder_shade", "wildwood", 16.0, 10.0, "22", "3", "10"),
        ],
        "orpg_story_quest",
        "orpg_elder_shade",
        tips("orpg_hero", "orpg_silverleaf_grove", "trg_orpg_victory"),
        (1, 4),
    )
}

pub(super) fn moba_aos_template() -> Result<StarterTemplateDefinition, EngineError> {
    let kind = StarterTemplateKind::MobaAos;
    let mut document = base_document(kind, 28, 18)?;
    document.upsert_region(region("moba_radiant_lane", 5.0, 0.0, 5.0, 3.0))?;
    document.upsert_region(region("moba_dire_lane", 22.0, 0.0, 12.0, 3.0))?;
    for object in [
        unit(
            "moba_hero_knight",
            "army",
            "player_one",
            "moba_radiant_lane",
            6.0,
            5.0,
            &[
                ("stat.health", "34"),
                ("stat.damage", "7"),
                ("stat.attack_period_ticks", "6"),
            ],
        ),
        unit(
            "moba_lane_creep_01",
            "army",
            "player_one",
            "moba_radiant_lane",
            7.0,
            6.0,
            &[
                ("stat.health", "14"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "9"),
            ],
        ),
        unit(
            "moba_enemy_hero",
            "army",
            "player_two",
            "moba_dire_lane",
            21.0,
            12.0,
            &[
                ("stat.health", "22"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "9"),
            ],
        ),
        unit(
            "moba_enemy_creep_01",
            "army",
            "player_two",
            "moba_dire_lane",
            20.0,
            11.0,
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
        "moba_lane_objective",
        "moba_hero_knight",
        "moba_enemy_hero",
    )?;
    assign_models(&mut document)?;
    finish(
        kind,
        document,
        scenario(kind, 2, 10, MapRuleset::Custom)?,
        graphs("moba", "moba_enemy_hero", "moba_radiant_lane"),
        vec![
            tip(
                "Objects",
                "moba_hero_knight",
                "Tune the hero's lane pressure.",
                "Raise stat.damage by 1.",
            ),
            tip(
                "Terrain",
                "moba_radiant_lane",
                "Move lane starts to reshape first contact.",
                "Shift moba_radiant_lane east.",
            ),
            tip(
                "Triggers",
                "trg_moba_victory",
                "Open the victory trigger before adding shops.",
                "Add a post-win reward action.",
            ),
        ],
    )
}

pub(super) fn hero_defense_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena_like(
        StarterTemplateKind::HeroDefenseSurvival,
        "defense_last_brazier",
        22,
        16,
        [
            hero("defense_paladin", "player_one", 9.0, 8.0, "40", "7", "6"),
            hero("defense_ranger", "player_one", 10.0, 9.0, "24", "5", "8"),
            monster("defense_ghoul_01", "horde", 14.0, 8.0, "12", "2", "12"),
            monster("defense_ghoul_02", "horde", 15.0, 9.0, "12", "2", "12"),
            boss("defense_bone_lord", "horde", 17.0, 9.0, "24", "3", "10"),
        ],
        "defense_holdout_objective",
        "defense_bone_lord",
        tips(
            "defense_paladin",
            "defense_last_brazier",
            "trg_hero_defense_survival_victory",
        ),
        (1, 5),
    )
}

pub(super) fn hero_arena_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena_like(
        StarterTemplateKind::HeroArena,
        "hero_arena_sunspire",
        18,
        18,
        [
            hero(
                "hero_arena_champion",
                "player_one",
                8.0,
                8.0,
                "36",
                "8",
                "6",
            ),
            monster(
                "hero_arena_duelist_01",
                "challengers",
                6.0,
                8.0,
                "10",
                "1",
                "16",
            ),
            monster(
                "hero_arena_duelist_02",
                "challengers",
                10.0,
                8.0,
                "10",
                "1",
                "16",
            ),
            monster(
                "hero_arena_duelist_03",
                "challengers",
                8.0,
                6.0,
                "10",
                "1",
                "16",
            ),
            boss(
                "hero_arena_grandmaster",
                "challengers",
                8.0,
                11.0,
                "20",
                "2",
                "14",
            ),
        ],
        "hero_arena_objective",
        "hero_arena_grandmaster",
        tips(
            "hero_arena_champion",
            "hero_arena_sunspire",
            "trg_hero_arena_victory",
        ),
        (2, 8),
    )
}

pub(super) fn survival_chaos_template() -> Result<StarterTemplateDefinition, EngineError> {
    let kind = StarterTemplateKind::SurvivalChaosLineWars;
    let mut document = base_document(kind, 24, 12)?;
    document.upsert_region(region("chaos_lane_start", 2.0, 0.0, 5.0, 2.0))?;
    document.upsert_region(region("chaos_bastion", 20.0, 0.0, 5.0, 3.0))?;
    for object in [
        unit(
            "chaos_core",
            "base",
            "player_one",
            "chaos_bastion",
            20.0,
            5.0,
            &[("stat.health", "16")],
        ),
        unit(
            "chaos_guard_alpha",
            "tower",
            "player_one",
            "chaos_bastion",
            11.0,
            4.0,
            &[
                ("stat.health", "18"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "5"),
            ],
        ),
        unit(
            "chaos_guard_beta",
            "tower",
            "player_one",
            "chaos_bastion",
            13.0,
            7.0,
            &[
                ("stat.health", "18"),
                ("stat.damage", "4"),
                ("stat.attack_period_ticks", "5"),
            ],
        ),
        unit(
            "chaos_line_01",
            "creep",
            "chaos",
            "chaos_lane_start",
            2.0,
            5.0,
            &[
                ("stat.health", "11"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
        unit(
            "chaos_line_02",
            "creep",
            "chaos",
            "chaos_lane_start",
            3.0,
            5.0,
            &[
                ("stat.health", "11"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
        unit(
            "chaos_line_03",
            "creep",
            "chaos",
            "chaos_lane_start",
            4.0,
            5.0,
            &[
                ("stat.health", "11"),
                ("stat.path_length", "80"),
                ("stat.speed_per_tick", "1"),
                ("stat.leak_damage", "4"),
                ("stat.bounty", "2"),
            ],
        ),
    ] {
        document.upsert_object(object)?;
    }
    add_quest(
        &mut document,
        "chaos_line_objective",
        "chaos_core",
        "chaos_line_03",
    )?;
    assign_models(&mut document)?;
    finish(
        kind,
        document,
        scenario(kind, 1, 4, MapRuleset::Custom)?,
        graphs("chaos", "chaos_line_01", "chaos_bastion"),
        tips("chaos_guard_alpha", "chaos_lane_start", "trg_chaos_victory"),
    )
}

pub(super) fn mini_game_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena_like(
        StarterTemplateKind::MiniGameParty,
        "party_crystal_dash",
        18,
        14,
        [
            hero("party_runner", "player_one", 5.0, 7.0, "30", "8", "5"),
            monster("party_crate_01", "obstacle", 8.0, 7.0, "8", "1", "20"),
            monster("party_crate_02", "obstacle", 10.0, 7.0, "8", "1", "20"),
            monster("party_crate_03", "obstacle", 12.0, 7.0, "8", "1", "20"),
            boss("party_finish_gate", "obstacle", 14.0, 7.0, "12", "1", "20"),
        ],
        "party_dash_objective",
        "party_finish_gate",
        tips(
            "party_runner",
            "party_crystal_dash",
            "trg_mini_game_party_victory",
        ),
        (1, 8),
    )
}

pub(super) fn maze_escape_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena_like(
        StarterTemplateKind::MazeEscape,
        "maze_gloomroot_path",
        20,
        20,
        [
            hero("maze_escape_hero", "player_one", 4.0, 4.0, "32", "7", "6"),
            monster("maze_trap_01", "maze", 8.0, 6.0, "8", "2", "14"),
            monster("maze_trap_02", "maze", 11.0, 9.0, "8", "2", "14"),
            monster("maze_trap_03", "maze", 14.0, 12.0, "8", "2", "14"),
            boss("maze_exit_warden", "maze", 17.0, 16.0, "18", "2", "12"),
        ],
        "maze_escape_objective",
        "maze_exit_warden",
        tips(
            "maze_escape_hero",
            "maze_gloomroot_path",
            "trg_maze_escape_victory",
        ),
        (1, 4),
    )
}
