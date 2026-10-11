use litd_engine::EngineError;

use crate::{
    history::EditorObject,
    mcp_trigger_tools::EcaGraph,
    scenario::MapRuleset,
    starter_templates::{GuidedFirstEditTooltip, StarterTemplateDefinition, StarterTemplateKind},
};

use super::common::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn arena_like<const N: usize>(
    kind: StarterTemplateKind,
    region_id: &str,
    width: u32,
    height: u32,
    units: [EditorObject; N],
    quest_id: &str,
    boss_id: &str,
    tips: Vec<GuidedFirstEditTooltip>,
    players: (u8, u8),
) -> Result<StarterTemplateDefinition, EngineError> {
    let mut document = base_document(kind, width, height)?;
    document.upsert_region(region(
        region_id,
        width as f32 / 2.0,
        0.0,
        height as f32 / 2.0,
        5.0,
    ))?;
    for mut object in units {
        object
            .properties
            .insert("template.region_id".to_string(), region_id.to_string());
        document.upsert_object(object)?;
    }
    let start_id = document.objects.keys().next().cloned().unwrap();
    add_quest(&mut document, quest_id, &start_id, boss_id)?;
    assign_models(&mut document)?;
    finish(
        kind,
        document,
        scenario(kind, players.0, players.1, MapRuleset::Custom)?,
        graphs(kind_prefix(kind), boss_id, region_id),
        tips,
    )
}

pub(super) fn hero(
    id: &str,
    owner: &str,
    x: f32,
    z: f32,
    hp: &str,
    damage: &str,
    period: &str,
) -> EditorObject {
    combat_unit(CombatUnitArgs {
        id,
        role: "hero",
        owner,
        x,
        z,
        hp,
        damage,
        period,
    })
}

pub(super) fn monster(
    id: &str,
    owner: &str,
    x: f32,
    z: f32,
    hp: &str,
    damage: &str,
    period: &str,
) -> EditorObject {
    combat_unit(CombatUnitArgs {
        id,
        role: "monster",
        owner,
        x,
        z,
        hp,
        damage,
        period,
    })
}

pub(super) fn boss(
    id: &str,
    owner: &str,
    x: f32,
    z: f32,
    hp: &str,
    damage: &str,
    period: &str,
) -> EditorObject {
    combat_unit(CombatUnitArgs {
        id,
        role: "boss",
        owner,
        x,
        z,
        hp,
        damage,
        period,
    })
}

pub(super) fn graphs(prefix: &str, unit_down_id: &str, spawn_region: &str) -> Vec<EcaGraph> {
    vec![
        graph(
            &format!("trg_{prefix}_init"),
            "Starter Setup",
            "Setup",
            vec![node("evt_init", "map_initialization", 1, &[])],
            vec![],
            vec![sleep("act_intro", 1, "show starter objective")],
        ),
        graph(
            &format!("trg_{prefix}_down"),
            "Objective Down Reward",
            "Combat",
            vec![node(
                "evt_down",
                "unit_dies",
                1,
                &[("unit_id", unit_down_id)],
            )],
            vec![],
            vec![create_unit(
                "act_reward",
                1,
                &format!("{prefix}_reward"),
                &format!("{prefix}_reward_type"),
                "player_one",
                spawn_region,
                1.0,
                0.0,
                1.0,
            )],
        ),
        graph(
            &format!("trg_{prefix}_victory"),
            "Starter Victory",
            "Terminal",
            vec![node(
                "evt_victory",
                "game_victory",
                1,
                &[("player_id", "player_one")],
            )],
            vec![],
            vec![sleep("act_victory", 1, "show starter victory")],
        ),
    ]
}

pub(super) fn tips(
    object_id: &str,
    region_id: &str,
    trigger_id: &str,
) -> Vec<GuidedFirstEditTooltip> {
    vec![
        tip(
            "Objects",
            object_id,
            "Tune the main unit to rebalance the starter.",
            "Raise stat.damage by 1.",
        ),
        tip(
            "Terrain",
            region_id,
            "Move the primary region to reshape the first encounter.",
            "Drag the region two tiles east.",
        ),
        tip(
            "Triggers",
            trigger_id,
            "Open the terminal trigger before adding rewards.",
            "Add a create-unit reward action.",
        ),
    ]
}

struct CombatUnitArgs<'a> {
    id: &'a str,
    role: &'a str,
    owner: &'a str,
    x: f32,
    z: f32,
    hp: &'a str,
    damage: &'a str,
    period: &'a str,
}

fn combat_unit(args: CombatUnitArgs<'_>) -> EditorObject {
    let CombatUnitArgs {
        id,
        role,
        owner,
        x,
        z,
        hp,
        damage,
        period,
    } = args;
    unit(
        id,
        role,
        owner,
        "template_main_region",
        x,
        z,
        &[
            ("stat.health", hp),
            ("stat.damage", damage),
            ("stat.attack_period_ticks", period),
        ],
    )
}

fn kind_prefix(kind: StarterTemplateKind) -> &'static str {
    match kind {
        StarterTemplateKind::Orpg => "orpg",
        StarterTemplateKind::HeroDefenseSurvival => "hero_defense_survival",
        StarterTemplateKind::HeroArena => "hero_arena",
        StarterTemplateKind::MiniGameParty => "mini_game_party",
        StarterTemplateKind::MazeEscape => "maze_escape",
        _ => "starter",
    }
}
