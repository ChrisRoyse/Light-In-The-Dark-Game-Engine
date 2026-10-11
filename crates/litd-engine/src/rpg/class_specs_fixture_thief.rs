// Factual mechanics transcribed from wiki.carrionfields.net/class/thief (issue #1021 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data. Uses the
// `litd_engine` self-alias so this file works unmodified as both a `#[cfg(test)]` lib module and
// (via `#[path]`) the issue1021 FSV example's fixture include.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityAcquisitionCost, AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount,
    ClassLeveledAbility, ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract,
    PrimeStat, RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const THIEF_CLASS_ID: &str = "class.galdmere_shade";

/// All 20 authored race ids (#1001-#1018 batch + #999/#1000 Human/Elf), the roster the
/// restricted `race_access` gates below are validated against.
pub(crate) fn known_races() -> BTreeSet<String> {
    [
        "race.ashen_human",
        "race.elf",
        "race.half_elf",
        "race.ashkin_mixed",
        "race.ashkin_deep",
        "race.wood_elf",
        "race.dwarf",
        "race.onyx_kin",
        "race.gnome",
        "race.deep_kin",
        "race.orc",
        "race.goblin",
        "race.minotaur",
        "race.storm_giant",
        "race.fire_giant",
        "race.frost_giant",
        "race.cloud_giant",
        "race.sylphkin",
        "race.felisar",
        "race.saur",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

// CF race restriction (verbatim, #1021): "All races except for the Giant races, Orcs,
// Wood-Elves, Azure-touched, Dwarves, Minotaurs, Goblins, Saurians, and Centaurs may become
// thieves." Azure-touched/Centaur have no authored race analog in this roster (see
// implementation_notes). Remaining allowed set below.
pub(crate) fn thief_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: THIEF_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Shade".to_string(),
        player_facing_canon_name: "Shade".to_string(),
        source_issue: 1021,
        canon_issues: vec![986, 1021],
        mechanics_source_url: "https://wiki.carrionfields.net/class/thief".to_string(),
        source_history_label: "CF Thief factual mechanics source only".to_string(),
        concept_summary: "Galdmere-trained shadow guild members who trade in what a name is \
             worth on the crossroads: pickpockets, trappers, poisoners, and binders who narrow \
             their craft the deeper they specialize in one guild path."
            .to_string(),
        prime_stat: PrimeStat::Dexterity,
        stat_minimums: AttributeScores::new(0, 0, 0, 13, 0, 0),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Any,
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Martial,
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.onyx_kin",
                "race.gnome",
                "race.deep_kin",
                "race.sylphkin",
                "race.felisar",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "mace", "spear", "sword"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string()],
        leveled_abilities: thief_abilities(),
        specializations: thief_paths(),
        specialization_choice_levels: vec![1, 10, 12, 13, 34],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.thief.shadowcraft".to_string(),
            signature_vfx: vec![
                "vfx.thief.backstab_flash".to_string(),
                "vfx.thief.hide_shimmer".to_string(),
                "vfx.thief.poison_mist".to_string(),
                "vfx.thief.trap_spark".to_string(),
            ],
            palette_notes: "Charcoal and muted violet guild palette; no copied class iconography."
                .to_string(),
            animation_notes:
                "Path-specific finisher VFX (binder rope-work, trapper device arm/trigger, \
                 poisoner application) bound to the shared biped skeleton."
                    .to_string(),
        },
        implementation_notes: vec![
            "Azure-touched and Centaur are named in the CF race-exclusion text but have no \
             authored RaceSpec in this roster yet; they are simply absent from both the allow \
             and exclude accounting here, not silently included."
                .to_string(),
            "The CF thief-point economy (per-skill Cost/Cumulative Cost and prerequisite graph) \
             is now modeled via ClassLeveledAbility.acquisition (schema v2, gap 1, #1718). The \
             Binder path is migrated with its full verbatim CF cost chain (nimble fingers -> \
             ... -> shadow drag); the other seven paths' costs are transcribable the same way \
             and remain future work to keep this fixture under the line ratchet."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (DEX), the same convention as the \
             Warrior pilot (#1020); CF class pages do not publish exact numeric stat minimums."
                .to_string(),
            "specialization_choice_levels are the first unlock level of each of the 8 paths, not \
             a literal 'pick N paths at level X' gate; CF's path system is a continuous \
             point-buy, not a discrete specialization choice like Warrior's."
                .to_string(),
        ],
    }
}

fn thief_abilities() -> Vec<ClassLeveledAbility> {
    let mut abilities = Vec::new();
    abilities.extend(g(
        1,
        &[
            "Dagger", "Mace", "Spear", "Sword", "Backstab", "Dodge", "Trip", "Haggle", "Hide",
            "Peek", "Recall",
        ],
    ));
    abilities.extend(g(2, &["Aim"]));
    abilities.extend(g(3, &["Dirt Kicking"]));
    abilities.extend(g(4, &["Sneak"]));
    abilities.extend(g(5, &["Steal"]));
    abilities.extend(g(6, &["Evade"]));
    abilities.extend(g(7, &["Pick Lock"]));
    abilities.extend(g(8, &["Elbow"]));
    abilities.extend(g(9, &["Light Armor Use"]));
    abilities.extend(g(10, &["Detect Hidden", "Pen", "Relock"]));
    abilities.extend(g(11, &["Thieves Cant", "Knee", "Panhandle"]));
    abilities.extend(g(12, &["Disarm", "Second Attack"]));
    abilities.extend(g(13, &["Parry", "Lore"]));
    abilities.extend(g(14, &["Kick"]));
    abilities.extend(g(
        15,
        &["Hand to Hand", "Meditation", "Circle", "Bribe Mercenary"],
    ));
    abilities.extend(g(16, &["Fast Healing"]));
    abilities.extend(g(18, &["Pierce"]));
    abilities.extend(g(19, &["Rogues Awareness"]));
    abilities.extend(g(20, &["Precise Aim"]));
    abilities.extend(g(25, &["Enhanced Damage", "Dual Wield"]));
    abilities.extend(g(35, &["Dual Backstab"]));
    abilities.extend(g(42, &["Opportunity Strike"]));

    abilities.extend(p(13, "spec.thief.general", &["Gentle Walk"]));
    abilities.extend(p(14, "spec.thief.general", &["Knife"]));
    abilities.extend(p(16, "spec.thief.general", &["Covert"]));
    abilities.extend(p(17, "spec.thief.general", &["Blackjack"]));
    abilities.extend(p(19, "spec.thief.general", &["Disengage"]));
    abilities.extend(p(20, "spec.thief.general", &["Shield Block"]));
    abilities.extend(p(21, "spec.thief.general", &["Climb"]));
    abilities.extend(p(22, "spec.thief.general", &["Doubleback"]));
    abilities.extend(p(23, "spec.thief.general", &["Precision Strike"]));
    abilities.extend(p(24, "spec.thief.general", &["Thief Waylay"]));
    abilities.extend(p(25, "spec.thief.general", &["Cheap Shot"]));
    abilities.extend(p(26, "spec.thief.general", &["Weapon Trip"]));
    abilities.extend(p(27, "spec.thief.general", &["Improved Hide"]));
    abilities.extend(p(28, "spec.thief.general", &["Grease Weapon"]));
    abilities.extend(p(32, "spec.thief.general", &["Grapple"]));
    abilities.extend(p(33, "spec.thief.general", &["Isolate"]));
    abilities.extend(p(34, "spec.thief.general", &["Separate"]));
    abilities.extend(p(37, "spec.thief.general", &["Shadow Disappear"]));

    // Binder path point-buy economy (schema v2, gap 1), transcribed verbatim
    // from the CF thief wiki Binder Path table (Cost / Cumulative Cost /
    // Prerequisite columns). This is a single strictly-ordered prerequisite
    // chain; the validator checks cumulative == prereq.cumulative + cost.
    abilities.push(pc(12, "Nimble Fingers", 8, 8, None));
    abilities.push(pc(21, "Bind Legs", 6, 14, Some("Nimble Fingers")));
    abilities.push(pc(23, "Garrotte", 4, 18, Some("Bind Legs")));
    abilities.push(pc(26, "Gag", 6, 24, Some("Garrotte")));
    abilities.push(pc(28, "Blindfold", 6, 30, Some("Gag")));
    abilities.push(pc(29, "Bind Hands", 6, 36, Some("Blindfold")));
    abilities.push(pc(31, "Sleeping Disarm", 6, 42, Some("Bind Hands")));
    abilities.push(pc(33, "Tie", 6, 48, Some("Sleeping Disarm")));
    abilities.push(pc(34, "Truss", 6, 54, Some("Tie")));
    abilities.push(pc(36, "Drag", 2, 56, Some("Truss")));
    abilities.push(pc(38, "Shadow Drag", 2, 58, Some("Drag")));

    abilities.extend(p(34, "spec.thief.locksmith", &["Advanced Picklock"]));
    abilities.extend(p(40, "spec.thief.locksmith", &["Locksmith"]));

    abilities.extend(p(1, "spec.thief.mystic", &["Scrolls"]));
    abilities.extend(p(13, "spec.thief.mystic", &["Arcane Lore"]));
    abilities.extend(p(20, "spec.thief.mystic", &["Glimpse"]));
    abilities.extend(p(27, "spec.thief.mystic", &["Veil of Secrecy"]));
    abilities.extend(p(29, "spec.thief.mystic", &["Lore of the Ages"]));
    abilities.extend(p(32, "spec.thief.mystic", &["Veil of Souls"]));
    abilities.extend(p(35, "spec.thief.mystic", &["Infiltrate"]));
    abilities.extend(p(38, "spec.thief.mystic", &["Hypnotize"]));
    abilities.extend(p(41, "spec.thief.mystic", &["Arcane Vision"]));

    abilities.extend(p(12, "spec.thief.pickpocket", &["Plant"]));
    abilities.extend(p(15, "spec.thief.pickpocket", &["Discerning Gaze"]));
    abilities.extend(p(19, "spec.thief.pickpocket", &["Eye of Avarice"]));
    abilities.extend(p(21, "spec.thief.pickpocket", &["Deft Touch"]));
    abilities.extend(p(23, "spec.thief.pickpocket", &["Cutpurse"]));
    abilities.extend(p(
        25,
        "spec.thief.pickpocket",
        &["Appraise", "Combat Steal"],
    ));
    abilities.extend(p(26, "spec.thief.pickpocket", &["Warning Skill"]));
    abilities.extend(p(29, "spec.thief.pickpocket", &["Container Stealing"]));
    abilities.extend(p(33, "spec.thief.pickpocket", &["Acrobatics"]));
    abilities.extend(p(36, "spec.thief.pickpocket", &["Counterfeit"]));
    abilities.extend(p(38, "spec.thief.pickpocket", &["Fence"]));

    abilities.extend(p(12, "spec.thief.thug", &["Push"]));
    abilities.extend(p(14, "spec.thief.thug", &["Kidney Shot"]));
    abilities.extend(p(17, "spec.thief.thug", &["Gouge"]));
    abilities.extend(p(21, "spec.thief.thug", &["Weapon Butt Blackjack"]));
    abilities.extend(p(26, "spec.thief.thug", &["Parting Block"]));
    abilities.extend(p(28, "spec.thief.thug", &["Payment of the Coward"]));
    abilities.extend(p(29, "spec.thief.thug", &["Sucker Hit"]));
    abilities.extend(p(30, "spec.thief.thug", &["Gut Shot"]));
    abilities.extend(p(31, "spec.thief.thug", &["Death Strike"]));
    abilities.extend(p(35, "spec.thief.thug", &["Third Attack"]));
    abilities.extend(p(36, "spec.thief.thug", &["Strike on the Abandoned"]));
    abilities.extend(p(38, "spec.thief.thug", &["Earclap"]));
    abilities.extend(p(42, "spec.thief.thug", &["Prey on the Weak"]));

    abilities.extend(p(10, "spec.thief.trapper", &["Bag of Tricks"]));
    abilities.extend(p(13, "spec.thief.trapper", &["Smoke Bomb"]));
    abilities.extend(p(17, "spec.thief.trapper", &["Stench Cloud"]));
    abilities.extend(p(
        20,
        "spec.thief.trapper",
        &["Detect Traps", "Physical Trap", "Glowing Trap"],
    ));
    abilities.extend(p(
        26,
        "spec.thief.trapper",
        &["Knock-Out Trap", "Blister Agent"],
    ));
    abilities.extend(p(29, "spec.thief.trapper", &["Abrasive Powder"]));
    abilities.extend(p(31, "spec.thief.trapper", &["Direction Trap"]));
    abilities.extend(p(35, "spec.thief.trapper", &["Damage Trap"]));
    abilities.extend(p(39, "spec.thief.trapper", &["Burst Trap"]));
    abilities.extend(p(42, "spec.thief.trapper", &["Pepper Dust"]));

    abilities.extend(p(10, "spec.thief.poisoner", &["Envenom"]));
    abilities.extend(p(
        12,
        "spec.thief.poisoner",
        &["Apply", "Concoct Poison", "Food Poison", "Emetic Poison"],
    ));
    abilities.extend(p(
        14,
        "spec.thief.poisoner",
        &["Concoct Antidote", "Drink Poison"],
    ));
    abilities.extend(p(17, "spec.thief.poisoner", &["Inhaled Poison"]));
    abilities.extend(p(19, "spec.thief.poisoner", &["Knock-Out Poison"]));
    abilities.extend(p(
        23,
        "spec.thief.poisoner",
        &["Weapon Poison", "Neurological Poison"],
    ));
    abilities.extend(p(30, "spec.thief.poisoner", &["Fear Poison"]));
    abilities.extend(p(33, "spec.thief.poisoner", &["Contact Poison"]));
    abilities.extend(p(35, "spec.thief.poisoner", &["Weapon Nick"]));
    abilities.extend(p(36, "spec.thief.poisoner", &["Mind Control Poison"]));
    abilities.extend(p(40, "spec.thief.poisoner", &["Forgiving Cruor"]));
    abilities.extend(p(42, "spec.thief.poisoner", &["Grenade Missile"]));

    abilities
}

fn thief_paths() -> Vec<ClassSpecialization> {
    [
        ("spec.thief.general", "General Path", 13),
        ("spec.thief.binder", "Binder Path", 12),
        ("spec.thief.locksmith", "Locksmith Path", 34),
        ("spec.thief.mystic", "Mystic Path", 1),
        ("spec.thief.pickpocket", "Pickpocket Path", 12),
        ("spec.thief.thug", "Thug Path", 12),
        ("spec.thief.trapper", "Trapper Path", 10),
        ("spec.thief.poisoner", "Poisoner Path", 10),
    ]
    .into_iter()
    .map(|(id, name, level)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: level,
        description: format!(
            "{name}: a thief-point-buy skill tree; the more a shade strays \
             from one path, the less they can learn from it."
        ),
        eligibility: None,
    })
    .collect()
}

fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A group of same-level, unspecialized Skill abilities (mirrors a CF skills-table row).
fn g(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: slug(name),
            display_name: name.to_string(),
            kind: AbilityKind::Skill,
            specialization_ids: vec![],
            acquisition: None,
        })
        .collect()
}

/// A group of same-level, path-tagged Skill abilities. The ability id is prefixed with the
/// specialization id so identically-named skills in different paths never collide on the
/// `(level, ability_id)` uniqueness the validator enforces.
fn p(level: u16, spec_id: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: format!("{spec_id}.{}", slug(name)),
            display_name: name.to_string(),
            kind: AbilityKind::Skill,
            specialization_ids: vec![spec_id.to_string()],
            acquisition: None,
        })
        .collect()
}

/// A costed Binder-path skill (schema v2, gap 1). `cost`/`cumulative` are the CF
/// thief-point columns; `prereq` (if any) is the display name of the preceding
/// binder skill, resolved to its `spec.thief.binder.<slug>` ability id.
fn pc(
    level: u16,
    name: &str,
    cost: u16,
    cumulative: u16,
    prereq: Option<&str>,
) -> ClassLeveledAbility {
    ClassLeveledAbility {
        level,
        ability_id: format!("spec.thief.binder.{}", slug(name)),
        display_name: name.to_string(),
        kind: AbilityKind::Skill,
        specialization_ids: vec!["spec.thief.binder".to_string()],
        acquisition: Some(AbilityAcquisitionCost {
            cost,
            cumulative_cost: cumulative,
            prerequisite_ability_id: prereq
                .map(|prereq| format!("spec.thief.binder.{}", slug(prereq))),
        }),
    }
}
