//! #1732 catalog-gap stealth abilities: the five abilities the #1623 acquisition
//! matrix references but the original #1604 catalog omitted -- gentle walk,
//! heightened awareness, dark vision, mass invis, and vanish. CF-factual
//! (helpfiles cited per builder); authored through the shared `AbilityCatalog`
//! framework (#961). Split into its own file so `abilities_stealth.rs` and
//! `abilities_stealth_kits.rs` stay under the 500-line ratchet.

use super::abilities_stealth::{active, affect, cost, ids, passive, prereq, shift, state, target};
use super::abilities_types::{AbilityDefinition, AbilityKind, AbilityTargetMode};

// #1732 gentle walk (helpfile 1569, thief path L13): while walking gently there
// is a chance normally aggressive creatures will not notice and attack you.
pub(super) fn gentle_walk() -> AbilityDefinition {
    let mut a = active(
        "ability.gentle_walk",
        "Gentle Walk",
        AbilityKind::Skill,
        "class.thief",
        13,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Tread so softly that normally aggressive creatures may fail to notice you and hold their attack."
            .to_string();
    a.costs = vec![cost("movement", 4)];
    a.grants_tags = ids(&["stealth.gentle_step"]);
    a.effects = vec![state("effect.gentle_step", "aggression_dampen", 8)];
    a
}

// #1732 heightened awareness (helpfile 730, assassin L7): senses honed through
// years of study to detect even magically hidden foes.
pub(super) fn heightened_awareness() -> AbilityDefinition {
    let mut a = passive(
        "ability.heightened_awareness",
        "Heightened Awareness",
        "class.assassin",
        7,
    );
    a.description =
        "Hone your senses through years of study until even magically hidden foes cannot escape them."
            .to_string();
    a.effects = vec![affect(
        "effect.sense_magical_hidden",
        "sense_magical_hidden",
        0,
    )];
    a
}

// #1732 dark vision (helpfile 622, assassin L5, ranger L7): see well enough at
// night to travel without a light source or infravision.
pub(super) fn dark_vision() -> AbilityDefinition {
    let mut a = passive("ability.dark_vision", "Dark Vision", "class.assassin", 5);
    a.requirements.class_ids = ids(&["class.assassin", "class.ranger"]);
    a.description =
        "See well enough in darkness to travel without a light source or infravision.".to_string();
    a.effects = vec![affect("effect.night_sight", "night_sight", 0)];
    a
}

// #1732 mass invis (helpfile 433, transmuter L11): make the whole group,
// including the caster, invisible until each attacks or is attacked.
pub(super) fn mass_invis() -> AbilityDefinition {
    let mut a = active(
        "ability.mass_invis",
        "Mass Invisibility",
        AbilityKind::Spell,
        "class.transmuter",
        11,
        AbilityTargetMode::Area,
    );
    a.description =
        "Veil your whole group, yourself included; each veil breaks the moment that ally attacks or is attacked."
            .to_string();
    a.targeting = target(AbilityTargetMode::Area, 8, 0);
    a.costs = vec![cost("mana", 100)];
    a.grants_tags = ids(&["stealth.invisible"]);
    a.requirements.prerequisites = vec![prereq("ability.invisibility", 1)];
    a.effects = vec![affect("effect.mass_invisible", "invisible", 120)];
    a
}

// #1732 vanish (helpfile 521, assassin L16): burst a smoke globe and teleport to
// somewhere else within the same area.
pub(super) fn vanish() -> AbilityDefinition {
    let mut a = active(
        "ability.vanish",
        "Vanish",
        AbilityKind::Skill,
        "class.assassin",
        16,
        AbilityTargetMode::SelfOnly,
    );
    a.description =
        "Smash a smoke globe at your feet and reappear somewhere else within the same area."
            .to_string();
    a.timing.cooldown_ticks = 30;
    a.effects = vec![shift("effect.vanish_teleport", "area_teleport")];
    a
}
