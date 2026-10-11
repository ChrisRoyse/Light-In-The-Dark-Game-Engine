//! Canonical Ashen Reach stealth ability catalog (#1604-1622).
//!
//! CF-factual mechanics (helpfiles cited per builder) authored into the shared
//! `AbilityCatalog` framework (#961). Names/lore are originalized (#984); the
//! stealth mechanics themselves are transcribed faithfully. This is a library
//! module (not a test fixture) so unit tests and the #1604 FSV example read the
//! same source of truth. Wilderness/detection/arcane/reveal/assassin builders
//! live in `abilities_stealth_kits` to keep both files under the 500-line ratchet.

use super::abilities_stealth_extra::{
    dark_vision, gentle_walk, heightened_awareness, mass_invis, vanish,
};
use super::abilities_stealth_kits::{
    acute_vision, ambush, assassinate, blackjack, control_translucence, detect_hidden,
    detect_invisibility, duo_dimension, faerie_fire, faerie_fog, forest_haven,
    improved_invisibility, invisibility, observe, pass_door, smear_mud, stalk, track, treeform,
    waylay,
};
use super::abilities_types::{
    AbilityActivation, AbilityCatalog, AbilityCategory, AbilityCost, AbilityDefinition,
    AbilityEffect, AbilityEffectKind, AbilityKind, AbilityPrerequisite, AbilityRequirements,
    AbilityStackingPolicy, AbilityTargetMode, AbilityTargeting, AbilityTiming, RPG_ABILITY_SCHEMA,
};

pub const STEALTH_CATALOG_ID: &str = "catalog.ashen_reach.stealth";

/// The full stealth ability catalog covering issues #1604-1611 and #1613-1622.
pub fn stealth_ability_catalog() -> AbilityCatalog {
    AbilityCatalog {
        schema: RPG_ABILITY_SCHEMA.to_string(),
        catalog_id: STEALTH_CATALOG_ID.to_string(),
        abilities: vec![
            // Concealment & silent movement (#1604-1610).
            sneak(),
            quiet_movement(),
            hide(),
            improved_hide(),
            covert(),
            shadow_disappear(),
            camouflage(),
            creep(),
            // Ambush & wilderness hunter kit (#1611, #1613, #1620).
            ambush(),
            waylay(),
            observe(),
            smear_mud(),
            track(),
            // Detection / counter-sense suite (#1618).
            detect_hidden(),
            acute_vision(),
            detect_invisibility(),
            // Arcane concealment (#1615-1617).
            invisibility(),
            improved_invisibility(),
            duo_dimension(),
            pass_door(),
            control_translucence(),
            // Druid wild concealment (#1614).
            treeform(),
            forest_haven(),
            // Reveal / lockout tempo counters (#1619).
            faerie_fire(),
            faerie_fog(),
            // Assassin & thief verbs (#1621, #1622).
            stalk(),
            assassinate(),
            blackjack(),
            // #1732 catalog-gap abilities the #1623 acquisition matrix needs.
            gentle_walk(),
            heightened_awareness(),
            dark_vision(),
            mass_invis(),
            vanish(),
        ],
    }
}

// --- Concealment & silent movement -----------------------------------------

// #1604 sneak (helpfile 1169): move without being noticed entering/exiting an
// area; keeps a hidden mover hidden; VISIBLE cancels. Thief/Assassin L4.
fn sneak() -> AbilityDefinition {
    let mut a = active(
        "ability.sneak",
        "Sneak",
        AbilityKind::Skill,
        "class.thief",
        4,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Slip between areas without being noticed; entry and exit cues are masked. VISIBLE cancels."
            .to_string();
    a.costs = vec![cost("movement", 6)];
    a.grants_tags = ids(&["stealth.moving"]);
    a.effects = vec![state("effect.sneak_move", "stealth_move", 8)];
    a.ui_slot = Some("stealth.sneak".to_string());
    a
}

// #1605 quiet movement (helpfile 483): automatic; strips destination-direction
// cues on leaving a room. Ranger L5, Druid L13.
fn quiet_movement() -> AbilityDefinition {
    let mut a = base(
        "ability.quiet_movement",
        "Quiet Movement",
        AbilityKind::Skill,
        AbilityActivation::Automatic,
        "class.ranger",
    );
    a.description =
        "On leaving a room your heading is not revealed, denying pursuers a trail vector."
            .to_string();
    a.requirements.min_level = 5;
    a.targeting = target(AbilityTargetMode::SelfOnly, 1, 0);
    a.trigger_tags = ids(&["trigger.leave_room"]);
    a.effects = vec![state("effect.heading_masked", "heading_mask", 1)];
    a
}

// #1606 hide (helpfile 503): terrain-gated stationary concealment; no success
// feedback; move only via sneak; VISIBLE stops. Thief L1 (CF helpfile 503 / thief
// skill table); assassins gain it at L4 (tracked in the #1623 acquisition matrix).
fn hide() -> AbilityDefinition {
    let mut a = active(
        "ability.hide",
        "Hide",
        AbilityKind::Skill,
        "class.thief",
        1,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Press into terrain appropriate to your training and become unseen without detect hidden."
            .to_string();
    a.timing.lag_ticks = 2;
    a.timing.cooldown_ticks = 4;
    a.grants_tags = ids(&["stealth.hidden"]);
    a.effects = vec![affect("effect.concealed", "concealment", 0)];
    a.ui_slot = Some("stealth.hide".to_string());
    a
}

// #1607 improved hide (helpfile 1421, thief L27): communicate and use doors
// while staying hidden.
fn improved_hide() -> AbilityDefinition {
    let mut a = passive("ability.improved_hide", "Improved Hide", "class.thief", 27);
    a.description =
        "Whisper to allies and open or close doors while remaining hidden from failing observers."
            .to_string();
    a.requirements.prerequisites = vec![prereq("ability.hide", 1)];
    a.effects = vec![affect(
        "effect.hidden_doors_and_speech",
        "hidden_action_whitelist",
        0,
    )];
    a
}

// #1607 covert (helpfile 2185, thief L16): open/close doors AND containers
// silently and covertly (automatic).
fn covert() -> AbilityDefinition {
    let mut a = passive("ability.covert", "Covert", "class.thief", 16);
    a.description =
        "Open and close doors and containers with no broadcast sound or animation to non-perceivers."
            .to_string();
    a.effects = vec![affect("effect.silent_containers", "silent_interaction", 0)];
    a
}

// #1608 shadow disappear (helpfile 1425, thief L37): re-hide during combat.
fn shadow_disappear() -> AbilityDefinition {
    let mut a = active(
        "ability.shadow_disappear",
        "Shadow Disappear",
        AbilityKind::Skill,
        "class.thief",
        37,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Dash to the nearest cover mid-fight, drop enemy target locks, and begin hiding in the room."
            .to_string();
    a.timing.cooldown_ticks = 20;
    a.timing.cooldown_group = Some("cooldown.rehide".to_string());
    a.requirements.prerequisites = vec![prereq("ability.hide", 1)];
    a.effects = vec![state("effect.combat_rehide", "combat_rehide", 2)];
    a
}

// #1609 camouflage (helpfile 467, ranger L4): hide in field/forest/mountain;
// broken by any movement. Wood-elf affinity.
fn camouflage() -> AbilityDefinition {
    let mut a = active(
        "ability.camouflage",
        "Camouflage",
        AbilityKind::Skill,
        "class.ranger",
        4,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Blend into field, forest, or mountain terrain until you move; acute vision is the counter."
            .to_string();
    a.timing.lag_ticks = 3;
    a.grants_tags = ids(&["stealth.camouflaged"]);
    a.effects = vec![affect("effect.camouflaged", "camouflage", 0)];
    a.ui_slot = Some("stealth.camouflage".to_string());
    a
}

// #1610 creep (helpfile 470, ranger L19): move through wilderness while staying
// camouflaged; size and terrain-mismatch penalties.
fn creep() -> AbilityDefinition {
    let mut a = active(
        "ability.creep",
        "Creep",
        AbilityKind::Skill,
        "class.ranger",
        19,
        AbilityTargetMode::Direction,
    );
    a.description =
        "Crouch-walk through wilderness without breaking camouflage; larger frames struggle."
            .to_string();
    a.targeting.range_tiles = 1;
    a.costs = vec![cost("movement", 8)];
    a.requirements.prerequisites = vec![prereq("ability.camouflage", 1)];
    a.effects = vec![state("effect.creep_move", "creep_move", 4)];
    a
}

// --- Builders / helpers (shared with abilities_stealth_kits) ----------------

pub(super) fn active(
    id: &str,
    name: &str,
    kind: AbilityKind,
    gate: &str,
    level: u16,
    mode: AbilityTargetMode,
) -> AbilityDefinition {
    let mut a = base(id, name, kind, AbilityActivation::Active, gate);
    a.requirements.min_level = level;
    a.targeting = target(mode, 1, 3);
    a
}

pub(super) fn passive(id: &str, name: &str, gate: &str, level: u16) -> AbilityDefinition {
    let mut a = base(
        id,
        name,
        AbilityKind::Skill,
        AbilityActivation::Passive,
        gate,
    );
    a.requirements.min_level = level;
    a
}

pub(super) fn base(
    id: &str,
    name: &str,
    kind: AbilityKind,
    activation: AbilityActivation,
    gate: &str,
) -> AbilityDefinition {
    AbilityDefinition {
        ability_id: id.to_string(),
        display_name: name.to_string(),
        description: format!("{name} stealth ability."),
        kind,
        activation,
        category: AbilityCategory {
            family_id: gate.to_string(),
            school_id: Some("school.stealth".to_string()),
            path_id: Some("path.concealment".to_string()),
            branch_id: Some("branch.shadow".to_string()),
            rank: 1,
            max_rank: 5,
            capstone: false,
        },
        requirements: gate_requirements(kind, gate),
        costs: Vec::new(),
        timing: AbilityTiming {
            cast_ticks: 0,
            round_ticks: 0,
            lag_ticks: 0,
            cooldown_ticks: 0,
            cooldown_group: None,
        },
        targeting: target(AbilityTargetMode::None, 0, 0),
        effects: Vec::new(),
        tags: ids(&["ability.stealth"]),
        grants_tags: Vec::new(),
        blocks_tags: Vec::new(),
        trigger_tags: Vec::new(),
        ui_slot: None,
    }
}

fn gate_requirements(kind: AbilityKind, gate: &str) -> AbilityRequirements {
    let mut req = requirements(1);
    match kind {
        AbilityKind::Skill | AbilityKind::Spell | AbilityKind::Song => {
            req.class_ids = ids(&[gate]);
        }
        AbilityKind::Supplication => req.deity_domains = ids(&[gate]),
        AbilityKind::CabalPower => req.cabal_ids = ids(&[gate]),
        AbilityKind::RacialInnate => req.race_ids = ids(&[gate]),
    }
    req
}

fn requirements(min_level: u16) -> AbilityRequirements {
    AbilityRequirements {
        min_level,
        class_ids: Vec::new(),
        race_ids: Vec::new(),
        cabal_ids: Vec::new(),
        alignments: Vec::new(),
        ethos: Vec::new(),
        deity_domains: Vec::new(),
        prerequisites: Vec::new(),
    }
}

pub(super) fn target(
    mode: AbilityTargetMode,
    max_targets: u16,
    range_tiles: u16,
) -> AbilityTargeting {
    AbilityTargeting {
        mode,
        max_targets,
        range_tiles,
        requires_line_of_sight: mode != AbilityTargetMode::None,
        allow_friendly_fire: false,
    }
}

fn eff(
    id: &str,
    kind: AbilityEffectKind,
    channel: &str,
    min: i32,
    max: i32,
    duration_ticks: u32,
) -> AbilityEffect {
    AbilityEffect {
        effect_id: id.to_string(),
        kind,
        channel: channel.to_string(),
        magnitude_min: min,
        magnitude_max: max,
        duration_ticks,
        tick_interval: 0,
        stacking_policy: AbilityStackingPolicy::None,
        stack_cap: 0,
    }
}

pub(super) fn affect(id: &str, channel: &str, duration_ticks: u32) -> AbilityEffect {
    eff(
        id,
        AbilityEffectKind::ApplyAffect,
        channel,
        1,
        1,
        duration_ticks,
    )
}

pub(super) fn state(id: &str, channel: &str, duration_ticks: u32) -> AbilityEffect {
    eff(
        id,
        AbilityEffectKind::StateTag,
        channel,
        1,
        1,
        duration_ticks,
    )
}

pub(super) fn hurt(id: &str, channel: &str, min: i32, max: i32) -> AbilityEffect {
    eff(id, AbilityEffectKind::Damage, channel, min, max, 0)
}

pub(super) fn shift(id: &str, channel: &str) -> AbilityEffect {
    eff(id, AbilityEffectKind::Movement, channel, 1, 1, 0)
}

pub(super) fn purge(id: &str, channel: &str) -> AbilityEffect {
    eff(id, AbilityEffectKind::Dispel, channel, 1, 1, 0)
}

pub(super) fn cost(resource_id: &str, amount: i32) -> AbilityCost {
    AbilityCost {
        resource_id: resource_id.to_string(),
        amount,
    }
}

pub(super) fn prereq(ability_id: &str, min_rank: u16) -> AbilityPrerequisite {
    AbilityPrerequisite {
        ability_id: ability_id.to_string(),
        min_rank,
    }
}

pub(super) fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
