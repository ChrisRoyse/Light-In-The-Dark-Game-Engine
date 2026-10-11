// Factual mechanics transcribed from wiki.carrionfields.net/en/class/shapeshifter (issue #1036
// FACTUAL SPEC comment). Mechanics are factual CF data; name/lore is an OPEN QUESTION (no canon
// shapeshifter tradition exists in Ashen Reach lore yet, per #984 honesty rule).
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const SHAPESHIFTER_CLASS_ID: &str = "class.shapeshifter";

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

// CF race restriction (verbatim, #1036): "All races may become shapeshifters except for the
// Giant races, Duergar, Dwarves, Wood-Elves, Orcs, Minotaurs, Centaurs, Goblins, and
// Azure-touched. Felar may become shapeshifters, but can not focus upon the Water and Air foci,
// due to their roots." Centaur/Azure-touched have no authored race analog in this roster.
pub(crate) fn shapeshifter_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: SHAPESHIFTER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Shapeshifter".to_string(),
        player_facing_canon_name: "Shapeshifter".to_string(),
        source_issue: 1036,
        canon_issues: vec![986, 1036],
        mechanics_source_url: "https://wiki.carrionfields.net/en/class/shapeshifter".to_string(),
        source_history_label: "CF Shapeshifter factual mechanics source only".to_string(),
        concept_summary: "Mages who alter their own bodies rather than the world around them, \
             twisting and turning into progressively more powerful animal forms as they master \
             one of five focuses: offense, defense, utility, water, or air."
            .to_string(),
        prime_stat: PrimeStat::Intelligence,
        stat_minimums: AttributeScores::new(0, 13, 0, 0, 0, 0),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Any,
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Unresolved {
            reason: "No canon shapeshifter tradition exists in the Ashen Reach lore docs; the \
                     beast-form fount is an unresolved operator/lore decision flagged by the \
                     #1036 capture."
                .to_string(),
        },
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.gnome",
                "race.deep_kin",
                "race.sylphkin",
                "race.felisar",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "staff", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.mystical".to_string()],
        leveled_abilities: shapeshifter_abilities(),
        specializations: shapeshifter_foci(),
        specialization_choice_levels: vec![30],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.shapeshifter.beastform".to_string(),
            signature_vfx: vec![
                "vfx.shapeshifter.shapeshift_transform".to_string(),
                "vfx.shapeshifter.scales_of_the_dragon_shimmer".to_string(),
                "vfx.shapeshifter.claw_of_the_lion_slash".to_string(),
                "vfx.shapeshifter.revert_flash".to_string(),
            ],
            palette_notes: "Focus-tinted accent hue (offense red / defense grey / utility amber \
                 / water teal / air white) over a shared hide-brown base; no copied class \
                 iconography."
                .to_string(),
            animation_notes: "Each attainable form (jaguar, panther, crocodile, diamondback, \
                 hyena, cobra, owl, vulture, dolphin, giant squid, and others) swaps the biped \
                 skeleton for a dedicated beast-form model per CF's Forms/Shapefocus key \
                 concepts."
                .to_string(),
        },
        implementation_notes: vec![
            "OPEN LORE QUESTION (not resolved here, per #1036's own flag): no canon shapeshifter \
             class, kindred, or beast-form tradition exists in the Ashen Reach lore docs; \
             display_name/player_facing_canon_name are left as the literal mechanical name \
             'Shapeshifter' pending an operator lore decision, per the #984 honesty rule against \
             fabricating canon."
                .to_string(),
            "Felar (mapped to Felisar in this roster) is allowed but CF notes it 'can not focus \
             upon the Water and Air foci, due to their roots'; ClassRaceAccess has no \
             per-race/per-specialization exception field, so Felisar is included in race_access \
             and the water/air foci restriction is documented here only. Filed as a framework \
             gap."
                .to_string(),
            "CF's 'attainable forms' list (jaguar, panther, crocodile, diamondback, hyena, \
             cobra, owl, vulture, dolphin, giant squid) is explicitly stated as 'some of the \
             forms attainable', not exhaustive; not modeled as a closed leveled-ability list to \
             avoid fabricating a false-complete set."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (INT), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn shapeshifter_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Dagger",
            "Staff",
            "Whip",
            "Scrolls",
            "Talismans",
            "Wands",
            "Recall",
            "Spellcraft",
        ],
    ));
    a.extend(g(5, &["Meditation"]));
    a.extend(g(8, &["Hand to Hand", "Haggle"]));
    a.extend(g(10, &["Pen"]));
    a.extend(g(14, &["Trance"]));
    a.extend(g(15, &["Fast Healing"]));
    a.extend(g(17, &["Parry"]));
    a.extend(g(20, &["Pugil", "Lash"]));
    a.extend(g(25, &["Second Attack"]));
    a.extend(g(27, &["Mystical Armor Use"]));
    a.extend(g(28, &["Shapeshift Skill"]));
    a.extend(g(34, &["Control Levitation"]));
    a.extend(g(35, &["Control Speed"]));
    a.extend(g(36, &["Control Skin"]));
    a.extend(g(37, &["Control Phase"]));

    a.extend(sp(1, &["Faerie Fire", "Shell of the Tortoise"]));
    a.extend(sp(2, &["Detect Magic", "Chitin of the Crab"]));
    a.extend(sp(
        3,
        &[
            "Detect Invis",
            "Hooves of the Gazelle",
            "Scales of the Crocodile",
        ],
    ));
    a.extend(sp(4, &["Invis", "Tail of the Lizard"]));
    a.extend(sp(5, &["Hide of the Rhino"]));
    a.extend(sp(6, &["Horns of the Bull"]));
    a.extend(sp(8, &["Claw of the Lion"]));
    a.extend(sp(9, &["Eyes of the Cat"]));
    a.extend(sp(10, &["Fly", "Identify", "Paws of the Cheetah"]));
    a.extend(sp(11, &["Giant Strength"]));
    a.extend(sp(12, &["Fangs of the Serpent"]));
    a.extend(sp(13, &["Faerie Fog"]));
    a.extend(sp(
        14,
        &[
            "Teleport",
            "Water Breathing",
            "Shapeshift",
            "Ears of the Bat",
        ],
    ));
    a.extend(sp(15, &["Haste"]));
    a.extend(sp(17, &["Dispel Magic"]));
    a.extend(sp(18, &["Cancellation", "Slow"]));
    a.extend(sp(22, &["Fumble"]));
    a.extend(sp(23, &["Enlarge"]));
    a.extend(sp(24, &["Pass Door", "Stone Skin"]));
    a.extend(sp(25, &["Scales of the Dragon"]));
    a.extend(sp(26, &["Reduce"]));
    a.extend(sp(28, &["Word of Recall"]));

    a.extend(fc(30, "offense", &["Ferocity of the Tiger"]));
    a.extend(fc(30, "utility", &["Resolve of the Mandrill"]));
    a.extend(fc(30, "defense", &["Primal Tenacity"]));
    a.extend(fc(30, "water", &["Brutality of the Shark"]));
    a.extend(fc(30, "air", &["Spirit of the Thunderhawk"]));
    a.extend(fc(32, "defense", &["Agility of the Mongoose"]));
    a.extend(fc(32, "utility", &["Stride of the Cheetah"]));
    a.extend(fc(32, "air", &["Hearing of the Barn Owl"]));
    a.extend(fc(33, "offense", &["Relentlessness of the Jaguar"]));
    a.extend(fc(33, "utility", &["Savage Instincts"]));
    a.extend(fc(33, "water", &["Litheness of the Dolphin"]));
    a.extend(sp(34, &["Spiderhands"]));
    a.extend(fc(34, "defense", &["Shell of the Armadillo"]));
    a.extend(fc(35, "air", &["Deftness of the Falcon"]));
    a.extend(sp(36, &["Control Translucence"]));
    a.extend(fc(36, "offense", &["Bestial Fury"]));
    a.extend(fc(36, "defense", &["Skin of the Displacer Beast"]));
    a.extend(fc(36, "utility", &["Resilience of the Hyena"]));
    a.extend(fc(36, "water", &["Hide of the Walrus"]));
    a.extend(fc(38, "offense", &["Instinct of the Predator"]));
    a.extend(fc(38, "defense", &["Recovery of the Snake"]));
    a.extend(fc(39, "utility", &["Slyness of the Fox"]));
    a.extend(fc(39, "water", &["Gills of the Axolotl"]));
    a.extend(fc(40, "air", &["Eyes of the Eagle"]));
    a.extend(fc(40, "offense", &["Unstable Mutation"]));
    a
}

fn shapeshifter_foci() -> Vec<ClassSpecialization> {
    [
        ("offense", "Offense Focus"),
        ("defense", "Defense Focus"),
        ("utility", "Utility Focus"),
        ("water", "Water Focus"),
        ("air", "Air Focus"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: format!("spec.shapeshifter.{id}"),
        display_name: name.to_string(),
        unlock_level: 30,
        description: format!("{name}: a major-form spell line unlocked at level 30."),
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

fn sp(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: slug(name),
            display_name: name.to_string(),
            kind: AbilityKind::Spell,
            specialization_ids: vec![],
            acquisition: None,
        })
        .collect()
}

fn fc(level: u16, focus: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    let spec_id = format!("spec.shapeshifter.{focus}");
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: format!("{spec_id}.{}", slug(name)),
            display_name: name.to_string(),
            kind: AbilityKind::Spell,
            specialization_ids: vec![spec_id.clone()],
            acquisition: None,
        })
        .collect()
}
