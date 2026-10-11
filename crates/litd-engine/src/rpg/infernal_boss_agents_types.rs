use serde::{Deserialize, Serialize};

use super::InfernalCircleId;

pub const RPG_INFERNAL_BOSS_AGENTS_SCHEMA: &str = "litd.rpg.infernal_boss_agents.v1";
pub const ASHEN_REACH_INFERNAL_BOSS_AGENTS_POLICY_ID: &str =
    "server.ashen_reach.infernal_boss_agents.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalBossAgentRole {
    CircleLord,
    ApexTyrant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossAgentPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub calyx_agent_runtime_required: bool,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub boss_agents: Vec<InfernalBossAgentSpec>,
}

impl InfernalBossAgentPolicy {
    pub fn ashen_reach_defaults() -> Self {
        Self {
            schema: RPG_INFERNAL_BOSS_AGENTS_SCHEMA.to_string(),
            server_policy_id: ASHEN_REACH_INFERNAL_BOSS_AGENTS_POLICY_ID.to_string(),
            source_issue: 1068,
            calyx_agent_runtime_required: true,
            hot_path_policy: "lowered_orders_only".to_string(),
            calyx_live_query_allowed_in_hot_path: false,
            boss_agents: vec![
                boss(BossArgs {
                    circle_id: InfernalCircleId::Threshold,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.threshold.ashgate_warden",
                    boss_id: "boss.infernal.threshold.ashgate_warden",
                    calyx_agent_id: "agent.infernal.threshold.ashgate_warden",
                    display_name: "Ashgate Warden",
                    target_body: 106801,
                    lethality_tier: 7,
                    trigger_terms: &["ashgate", "threshold", "intruder", "cindermark"],
                    ability_ids: &[
                        "ability.infernal.ashgate.brand_check",
                        "ability.infernal.ashgate.chain_sweep",
                        "ability.infernal.ashgate.cinder_alarm",
                    ],
                    order_velocity: [2, 0, 0],
                    min_strength_milli: 650,
                    dialogue_profile: dialogue(
                        "Your mark bought entry, not mercy.",
                        "The Gate remembers every unpaid step.",
                        &[
                            "Chain the weak to the threshold.",
                            "Burn their false courage into ash.",
                        ],
                        "The gate opens deeper, but it does not forgive.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::EmberWastes,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.ember_wastes.cinder_baron",
                    boss_id: "boss.infernal.ember_wastes.cinder_baron",
                    calyx_agent_id: "agent.infernal.ember_wastes.cinder_baron",
                    display_name: "Cinder Baron",
                    target_body: 106802,
                    lethality_tier: 7,
                    trigger_terms: &["ember", "lava", "behemoth", "heat"],
                    ability_ids: &[
                        "ability.infernal.ember_wastes.lava_surge",
                        "ability.infernal.ember_wastes.behemoth_call",
                        "ability.infernal.ember_wastes.cinder_maw",
                    ],
                    order_velocity: [3, 0, 1],
                    min_strength_milli: 700,
                    dialogue_profile: dialogue(
                        "The waste crowns only what survives the fire.",
                        "Every breath you steal belongs to my furnaces.",
                        &[
                            "Let the lava fields answer.",
                            "Feed the behemoths their shining bones.",
                        ],
                        "The embers remember my tithe.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::IronBastion,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.iron_bastion.iron_praetor",
                    boss_id: "boss.infernal.iron_bastion.iron_praetor",
                    calyx_agent_id: "agent.infernal.iron_bastion.iron_praetor",
                    display_name: "Iron Praetor",
                    target_body: 106803,
                    lethality_tier: 8,
                    trigger_terms: &["bastion", "legion", "siege", "chain"],
                    ability_ids: &[
                        "ability.infernal.iron_bastion.legion_command",
                        "ability.infernal.iron_bastion.siege_lock",
                        "ability.infernal.iron_bastion.furnace_march",
                    ],
                    order_velocity: [0, 0, 3],
                    min_strength_milli: 750,
                    dialogue_profile: dialogue(
                        "Order is the only mercy the Deeps allow.",
                        "Ranks close. Shields high. Let them break on law.",
                        &[
                            "Second cohort, bind the stairs.",
                            "Artillery, make their retreat a lesson.",
                        ],
                        "Discipline outlives its commander.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::HungeringDark,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.hungering_dark.the_maw",
                    boss_id: "boss.infernal.hungering_dark.the_maw",
                    calyx_agent_id: "agent.infernal.hungering_dark.the_maw",
                    display_name: "The Maw",
                    target_body: 106804,
                    lethality_tier: 8,
                    trigger_terms: &["dark", "hunger", "torch", "fear"],
                    ability_ids: &[
                        "ability.infernal.hungering_dark.light_eater",
                        "ability.infernal.hungering_dark.fear_tongue",
                        "ability.infernal.hungering_dark.maw_pull",
                    ],
                    order_velocity: [-2, 0, 2],
                    min_strength_milli: 800,
                    dialogue_profile: dialogue(
                        "Bring the light closer. It tastes better afraid.",
                        "Your names are warm in my teeth.",
                        &[
                            "Snuff the torches one by one.",
                            "Drag the loudest heart into silence.",
                        ],
                        "Hunger is older than victory.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::WailingTombs,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.wailing_tombs.choirmaster",
                    boss_id: "boss.infernal.wailing_tombs.choirmaster",
                    calyx_agent_id: "agent.infernal.wailing_tombs.choirmaster",
                    display_name: "The Choirmaster",
                    target_body: 106805,
                    lethality_tier: 9,
                    trigger_terms: &["dirge", "tombs", "bone", "soul"],
                    ability_ids: &[
                        "ability.infernal.wailing_tombs.soul_refrain",
                        "ability.infernal.wailing_tombs.bone_chorus",
                        "ability.infernal.wailing_tombs.withering_note",
                    ],
                    order_velocity: [0, 0, -3],
                    min_strength_milli: 850,
                    dialogue_profile: dialogue(
                        "Every death is a note. Yours arrives on time.",
                        "Choir, take the marrow and leave the memory.",
                        &[
                            "Raise the low bones.",
                            "Sing until their Constitution forgets itself.",
                        ],
                        "The song continues under your skin.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::MoltenThroneRoad,
                    role: InfernalBossAgentRole::CircleLord,
                    ruler_ref: "lord.infernal.molten_throne_road.throne_warden",
                    boss_id: "boss.infernal.molten_throne_road.throne_warden",
                    calyx_agent_id: "agent.infernal.molten_throne_road.throne_warden",
                    display_name: "Throne Warden",
                    target_body: 106806,
                    lethality_tier: 9,
                    trigger_terms: &["vault", "relic", "chain", "throne_road"],
                    ability_ids: &[
                        "ability.infernal.molten_throne_road.vault_seal",
                        "ability.infernal.molten_throne_road.relic_brand",
                        "ability.infernal.molten_throne_road.chain_judgment",
                    ],
                    order_velocity: [1, 0, -3],
                    min_strength_milli: 900,
                    dialogue_profile: dialogue(
                        "No hand reaches the vault while I still judge it.",
                        "Relics choose ruin before thieves.",
                        &[
                            "Lock the vault under molten law.",
                            "Brand the greedy before they touch the crown.",
                        ],
                        "The vault is closed by blood, not death.",
                    ),
                }),
                boss(BossArgs {
                    circle_id: InfernalCircleId::ObsidianThrone,
                    role: InfernalBossAgentRole::ApexTyrant,
                    ruler_ref: "lord.infernal.obsidian_throne.maldrithax",
                    boss_id: "boss.infernal.obsidian_throne.maldrithax",
                    calyx_agent_id: "agent.infernal.obsidian_throne.maldrithax",
                    display_name: "Maldrithax the Ashen Tyrant",
                    target_body: 106807,
                    lethality_tier: 10,
                    trigger_terms: &["apex", "obsidian", "tyrant", "name_burn"],
                    ability_ids: &[
                        "ability.infernal.obsidian_throne.name_burn",
                        "ability.infernal.obsidian_throne.ashen_decree",
                        "ability.infernal.obsidian_throne.throne_cleave",
                    ],
                    order_velocity: [0, 0, 4],
                    min_strength_milli: 950,
                    dialogue_profile: dialogue(
                        "Kneel, and I may leave enough name for mourning.",
                        "The throne does not bargain with surviving things.",
                        &[
                            "Burn their names from the Weave.",
                            "Let every oath kneel before ash.",
                        ],
                        "A tyrant falls only after the realm learns fear.",
                    ),
                }),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossAgentSpec {
    pub circle_id: InfernalCircleId,
    pub role: InfernalBossAgentRole,
    pub ruler_ref: String,
    pub boss_id: String,
    pub calyx_agent_id: String,
    pub display_name: String,
    pub target_body: u32,
    pub lethality_tier: u8,
    pub tactic_profile: InfernalBossTacticProfile,
    pub dialogue_profile: InfernalBossDialogueProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossTacticProfile {
    pub tactic_id: String,
    pub trigger_terms: Vec<String>,
    pub ability_ids: Vec<String>,
    pub order_velocity: [i32; 3],
    pub min_strength_milli: u16,
    pub prompt_template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossDialogueProfile {
    pub opening_line: String,
    pub aggro_line: String,
    pub phase_lines: Vec<String>,
    pub defeat_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossAgentHierarchyReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub boss_count: usize,
    pub calyx_agent_runtime_required: bool,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub built_in_map_required: bool,
    pub apex_agent_id: String,
    pub chain: Vec<InfernalBossAgentReadback>,
    pub policy_sha256: String,
    pub zone_policy_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalBossAgentReadback {
    pub depth: u8,
    pub circle_id: InfernalCircleId,
    pub zone_id: String,
    pub ruler_ref: String,
    pub boss_id: String,
    pub calyx_agent_id: String,
    pub display_name: String,
    pub role: InfernalBossAgentRole,
    pub lethality_tier: u8,
    pub tactic_id: String,
    pub trigger_term_count: usize,
    pub ability_count: usize,
    pub dialogue_line_count: usize,
    pub min_strength_milli: u16,
    pub target_body: u32,
}

struct BossArgs<'a> {
    circle_id: InfernalCircleId,
    role: InfernalBossAgentRole,
    ruler_ref: &'a str,
    boss_id: &'a str,
    calyx_agent_id: &'a str,
    display_name: &'a str,
    target_body: u32,
    lethality_tier: u8,
    trigger_terms: &'a [&'a str],
    ability_ids: &'a [&'a str],
    order_velocity: [i32; 3],
    min_strength_milli: u16,
    dialogue_profile: InfernalBossDialogueProfile,
}

fn boss(args: BossArgs<'_>) -> InfernalBossAgentSpec {
    let BossArgs {
        circle_id,
        role,
        ruler_ref,
        boss_id,
        calyx_agent_id,
        display_name,
        target_body,
        lethality_tier,
        trigger_terms,
        ability_ids,
        order_velocity,
        min_strength_milli,
        dialogue_profile,
    } = args;
    InfernalBossAgentSpec {
        circle_id,
        role,
        ruler_ref: ruler_ref.to_string(),
        boss_id: boss_id.to_string(),
        calyx_agent_id: calyx_agent_id.to_string(),
        display_name: display_name.to_string(),
        target_body,
        lethality_tier,
        tactic_profile: InfernalBossTacticProfile {
            tactic_id: format!("tactic.{}", boss_id),
            trigger_terms: trigger_terms.iter().map(|term| term.to_string()).collect(),
            ability_ids: ability_ids.iter().map(|id| id.to_string()).collect(),
            order_velocity,
            min_strength_milli,
            prompt_template: format!(
                "Infernal boss {{agent}} in {{world}} hears {{signal}}; terms={{terms}}; tactic=tactic.{boss_id}; answer in character and lower only deterministic combat intent."
            ),
        },
        dialogue_profile,
    }
}

fn dialogue(
    opening_line: &str,
    aggro_line: &str,
    phase_lines: &[&str],
    defeat_line: &str,
) -> InfernalBossDialogueProfile {
    InfernalBossDialogueProfile {
        opening_line: opening_line.to_string(),
        aggro_line: aggro_line.to_string(),
        phase_lines: phase_lines.iter().map(|line| line.to_string()).collect(),
        defeat_line: defeat_line.to_string(),
    }
}
