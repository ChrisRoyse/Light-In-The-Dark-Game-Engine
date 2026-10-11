//! Wilderness hunter, detection, arcane concealment, druid wild-form, reveal,
//! and assassin/thief stealth abilities (#1611-1622). Split out of
//! `abilities_stealth` to keep both files under the 500-line ratchet; shares
//! that module's builder helpers.

use super::abilities_stealth::{
    active, affect, base, cost, hurt, ids, passive, prereq, purge, shift, state, target,
};
use super::abilities_types::{
    AbilityActivation, AbilityDefinition, AbilityKind, AbilityTargetMode,
};

// --- Ambush & wilderness hunter kit (#1611, #1613, #1620) -------------------

// #1611 ambush (helpfile 467, ranger L12): manual strike from camouflage with a
// positional unaware bonus.
pub(super) fn ambush() -> AbilityDefinition {
    let mut a = active(
        "ability.ambush",
        "Ambush",
        AbilityKind::Skill,
        "class.ranger",
        12,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "Strike from camouflage; targets caught in your front arc and unaware take a positional opener."
            .to_string();
    a.timing.lag_ticks = 4;
    a.timing.cooldown_ticks = 6;
    a.requirements.prerequisites = vec![prereq("ability.camouflage", 1)];
    a.effects = vec![hurt("effect.ambush_opener", "physical", 60, 90)];
    a
}

// #1611 waylay (helpfile 1591, ranger L32): armed tripwire that auto-ambushes a
// marked victim entering reach.
pub(super) fn waylay() -> AbilityDefinition {
    let mut a = active(
        "ability.waylay",
        "Waylay",
        AbilityKind::Skill,
        "class.ranger",
        32,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "Arm a lie-in-wait against a victim; if they enter your reach the ambush fires automatically."
            .to_string();
    a.timing.cooldown_ticks = 8;
    a.requirements.prerequisites = vec![prereq("ability.ambush", 1)];
    a.effects = vec![
        state("effect.waylay_armed", "armed_trap", 12),
        hurt("effect.waylay_strike", "physical", 70, 100),
    ];
    a
}

// #1613 observe (hunter kit, ranger L33): study a target from range to learn
// patterns and abilities.
pub(super) fn observe() -> AbilityDefinition {
    let mut a = active(
        "ability.observe",
        "Observe",
        AbilityKind::Skill,
        "class.ranger",
        33,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "Study a distant target through a spyglass sense, surfacing an intel card on their kit."
            .to_string();
    a.targeting.range_tiles = 20;
    a.timing.cooldown_ticks = 6;
    a.effects = vec![affect("effect.target_intel", "target_intel", 30)];
    a
}

// #1613 smear mud (helpfile 2318, savage ranger L28): mud coat that dampens the
// thermal contrast infravision reads.
pub(super) fn smear_mud() -> AbilityDefinition {
    let mut a = active(
        "ability.smear_mud",
        "Smear Mud",
        AbilityKind::Skill,
        "class.ranger",
        28,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Coat yourself in mud to foil grapples and mask your heat signature against infravision."
            .to_string();
    a.timing.cooldown_ticks = 30;
    a.effects = vec![affect("effect.heat_mask", "heat_mask", 60)];
    a
}

// #1620 track (helpfile 478, ranger L30): follow tell-tale wilderness signs;
// success depends on skill and weather; sneak/quiet movement suppress the trail.
pub(super) fn track() -> AbilityDefinition {
    let mut a = active(
        "ability.track",
        "Track",
        AbilityKind::Skill,
        "class.ranger",
        30,
        AbilityTargetMode::Direction,
    );
    a.description =
        "Read the decaying trail a quarry left through the wilderness; rain and rivers erase it."
            .to_string();
    a.targeting.range_tiles = 12;
    a.timing.cooldown_ticks = 12;
    a.effects = vec![affect("effect.trail_read", "trail_read", 30)];
    a
}

// --- Detection / counter-sense suite (#1618) --------------------------------

// #1618 detect hidden (helpfile 489, thief L10, duergar racial): see hidden
// beings; terrain-profiled.
pub(super) fn detect_hidden() -> AbilityDefinition {
    let mut a = passive("ability.detect_hidden", "Detect Hidden", "class.thief", 10);
    a.description =
        "Notice hidden beings; the sense is sharpest in cities and open roads, weaker in wilds."
            .to_string();
    a.effects = vec![affect("effect.sense_hidden", "sense_hidden", 0)];
    a
}

// #1618 acute vision (helpfile 467, ranger L7): detect camouflaged players and
// hidden people in forests.
pub(super) fn acute_vision() -> AbilityDefinition {
    let mut a = passive("ability.acute_vision", "Acute Vision", "class.ranger", 7);
    a.description =
        "Pierce camouflage and spot foes hidden in forest terrain, the wilderness counter-sense."
            .to_string();
    a.effects = vec![affect("effect.sense_camouflage", "sense_camouflage", 0)];
    a
}

// #1618 detect invisibility (helpfile 384, transmuter, castable on others):
// render invisible entities as a detected shimmer.
pub(super) fn detect_invisibility() -> AbilityDefinition {
    let mut a = active(
        "ability.detect_invisibility",
        "Detect Invisibility",
        AbilityKind::Spell,
        "class.transmuter",
        8,
        AbilityTargetMode::Ally,
    );
    a.description =
        "Grant an ally sight of invisible entities, rendered as a clear detection shimmer."
            .to_string();
    a.costs = vec![cost("mana", 30)];
    a.effects = vec![affect("effect.sense_invisible", "sense_invisible", 120)];
    a
}

// --- Arcane concealment (#1615-1617) ----------------------------------------

// #1615 invisibility (helpfile 433, transmuter L4): target unseen until they
// attack or are attacked.
pub(super) fn invisibility() -> AbilityDefinition {
    let mut a = active(
        "ability.invisibility",
        "Invisibility",
        AbilityKind::Spell,
        "class.transmuter",
        4,
        AbilityTargetMode::Ally,
    );
    a.description =
        "Make a target invisible; the veil breaks the moment they attack or are attacked."
            .to_string();
    a.costs = vec![cost("mana", 50)];
    a.grants_tags = ids(&["stealth.invisible"]);
    a.effects = vec![affect("effect.invisible", "invisible", 120)];
    a.ui_slot = Some("stealth.invisibility".to_string());
    a
}

// #1615 improved invisibility (helpfile 719, invoker air-path): resists
// detection tiers and degrades slower.
pub(super) fn improved_invisibility() -> AbilityDefinition {
    let mut a = active(
        "ability.improved_invisibility",
        "Improved Invisibility",
        AbilityKind::Spell,
        "class.invoker",
        18,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "A resilient invisibility that resists lower detection tiers and fades more slowly."
            .to_string();
    a.costs = vec![cost("mana", 90)];
    a.requirements.prerequisites = vec![prereq("ability.invisibility", 1)];
    a.effects = vec![affect("effect.invisible_improved", "invisible", 240)];
    a
}

// #1616 duo dimension (helpfile 391, transmuter L36): fold to two dimensions,
// undetectable, pass any door, avoid attacks; faerie fire/fog block it.
pub(super) fn duo_dimension() -> AbilityDefinition {
    let mut a = active(
        "ability.duo_dimension",
        "Duo Dimension",
        AbilityKind::Spell,
        "class.transmuter",
        36,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Fold to a paper-thin plane: undetectable edge-on, passing doors, untargetable by most attacks."
            .to_string();
    a.costs = vec![cost("mana", 150)];
    a.timing.cooldown_ticks = 30;
    a.effects = vec![affect("effect.flat_form", "flat_form", 90)];
    a
}

// #1617 pass door (helpfile 438, transmuter L24): pass through closed doors;
// transmuters may cast it on others.
pub(super) fn pass_door() -> AbilityDefinition {
    let mut a = active(
        "ability.pass_door",
        "Pass Door",
        AbilityKind::Spell,
        "class.transmuter",
        24,
        AbilityTargetMode::Ally,
    );
    a.description =
        "Phase a target through closed doors with a translucent ghost-walk; walls stay solid."
            .to_string();
    a.costs = vec![cost("mana", 40)];
    a.effects = vec![affect("effect.phasing", "phasing", 120)];
    a
}

// #1617 control translucence (transmuter L36): use pass-door phasing to avoid
// being bashed.
pub(super) fn control_translucence() -> AbilityDefinition {
    let mut a = active(
        "ability.control_translucence",
        "Control Translucence",
        AbilityKind::Spell,
        "class.transmuter",
        36,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Hold the phasing edge so charge, bash, and tackle attacks whiff harmlessly through you."
            .to_string();
    a.costs = vec![cost("mana", 120)];
    a.timing.cooldown_ticks = 20;
    a.requirements.prerequisites = vec![prereq("ability.pass_door", 1)];
    a.effects = vec![affect("effect.bash_immune", "bash_immune", 60)];
    a
}

// --- Druid wild concealment (#1614) -----------------------------------------

// #1614 treeform (helpfile 610, druid supplication L20): become a tree,
// undetectable in a forest; any action leaves the form.
pub(super) fn treeform() -> AbilityDefinition {
    let mut a = active(
        "ability.treeform",
        "Treeform",
        AbilityKind::Supplication,
        "domain.wild",
        20,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Take the shape of a biome-matched tree, undetectable in forest; any action ends the form."
            .to_string();
    a.costs = vec![cost("devotion", 40)];
    a.timing.cooldown_ticks = 15;
    a.effects = vec![affect("effect.treeform_conceal", "treeform_conceal", 0)];
    a
}

// #1614 forest haven (helpfile 621, druid L18): call the wilderness for asylum,
// transporting elsewhere in the forest.
pub(super) fn forest_haven() -> AbilityDefinition {
    let mut a = active(
        "ability.forest_haven",
        "Forest Haven",
        AbilityKind::Supplication,
        "domain.wild",
        18,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Channel the forest for asylum, slipping to another forest node in the same region."
            .to_string();
    a.costs = vec![cost("devotion", 55)];
    a.timing.cooldown_ticks = 60;
    a.effects = vec![shift("effect.forest_teleport", "forest_teleport")];
    a
}

// --- Reveal / lockout tempo counters (#1619) --------------------------------

// #1619 faerie fire (helpfile 400, dark-elf innate): outline the victim and lock
// out all concealment (sneak/invis/hide/flat/camouflage).
pub(super) fn faerie_fire() -> AbilityDefinition {
    let mut a = base(
        "ability.faerie_fire",
        "Faerie Fire",
        AbilityKind::RacialInnate,
        AbilityActivation::Active,
        "race.dark_elf",
    );
    a.description =
        "Wreathe a foe in a clinging outline that glows through cover and refuses every concealment."
            .to_string();
    a.targeting = target(AbilityTargetMode::Enemy, 1, 3);
    a.timing.cooldown_ticks = 8;
    a.effects = vec![
        affect("effect.faerie_marked", "faerie_marked", 60),
        state("effect.conceal_lockout", "conceal_lockout", 60),
    ];
    a
}

// #1619 faerie fog (helpfile 401, druid L19): reveal ALL concealed creatures in
// the room with a no-rehide timer.
pub(super) fn faerie_fog() -> AbilityDefinition {
    let mut a = active(
        "ability.faerie_fog",
        "Faerie Fog",
        AbilityKind::Supplication,
        "domain.wild",
        19,
        AbilityTargetMode::Room,
    );
    a.description =
        "Burst a rolling fog that force-reveals every hidden, invisible, flat, or camouflaged creature."
            .to_string();
    a.targeting.max_targets = 12;
    a.targeting.range_tiles = 6;
    a.costs = vec![cost("devotion", 45)];
    a.timing.cooldown_ticks = 12;
    a.effects = vec![
        purge("effect.reveal_all", "concealment"),
        state("effect.rehide_lockout", "rehide_lockout", 30),
    ];
    a
}

// --- Assassin & thief verbs (#1621, #1622) ----------------------------------

// #1621 stalk (helpfile 744, assassin L35): shadow the victim while unperceived
// to raise the assassinate chance.
pub(super) fn stalk() -> AbilityDefinition {
    let mut a = active(
        "ability.stalk",
        "Stalk",
        AbilityKind::Skill,
        "class.assassin",
        35,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "Shadow a mark while unperceived to fill a study meter that multiplies the assassinate chance."
            .to_string();
    a.targeting.range_tiles = 8;
    a.timing.cooldown_ticks = 6;
    a.effects = vec![affect("effect.study_meter", "study_meter", 120)];
    a
}

// #1621 assassinate (helpfile 469, assassin L35): kill in one hit from stalked,
// unperceived state; failure is crippling and reveals you.
pub(super) fn assassinate() -> AbilityDefinition {
    let mut a = active(
        "ability.assassinate",
        "Assassinate",
        AbilityKind::Skill,
        "class.assassin",
        35,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "From an unperceived, fully studied state, attempt a killing blow; failure cripples and reveals."
            .to_string();
    a.timing.lag_ticks = 6;
    a.timing.cooldown_ticks = 40;
    a.requirements.prerequisites = vec![prereq("ability.stalk", 1)];
    a.effects = vec![hurt("effect.assassinate_strike", "physical", 900, 1300)];
    a
}

// #1622 blackjack (helpfile 482, thief L17): knockout opener from stealth on an
// unaware target.
pub(super) fn blackjack() -> AbilityDefinition {
    let mut a = active(
        "ability.blackjack",
        "Blackjack",
        AbilityKind::Skill,
        "class.thief",
        17,
        AbilityTargetMode::Enemy,
    );
    a.description =
        "Strike an unaware target from stealth with a sap, knocking them out non-lethally."
            .to_string();
    a.timing.lag_ticks = 4;
    a.timing.cooldown_ticks = 8;
    a.effects = vec![affect("effect.knockout", "knockout", 20)];
    a
}
