// Factual mechanics transcribed from wiki.carrionfields.net/class/paladin (issue #1027 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityClause, ClassEligibilityPolicy, ClassFount,
    ClassLeveledAbility, ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract,
    PrimeStat, RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const PALADIN_CLASS_ID: &str = "class.lampbearer";

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

// CF race restriction (verbatim, #1027): "Only Humans, Storm Giants, Half-Elves, Half-Drows,
// Dwarves, Azure-touched and Elves may learn to be a paladin." Azure-touched has no authored
// race analog in this roster.
pub(crate) fn paladin_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: PALADIN_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Lampbearer".to_string(),
        player_facing_canon_name: "Lampbearer".to_string(),
        source_issue: 1027,
        canon_issues: vec![986, 1027],
        mechanics_source_url: "https://wiki.carrionfields.net/class/paladin".to_string(),
        source_history_label: "CF Paladin factual mechanics source only".to_string(),
        concept_summary: "Holy warriors of an order of banners and witnesses, fighting corruption \
             in public because it thrives in private; empowerment sharpens their supplications, \
             and a DEDICATE path (shield, two-handed, monk, or champion) shapes their combat \
             kit."
            .to_string(),
        prime_stat: PrimeStat::Strength,
        stat_minimums: AttributeScores::new(13, 0, 0, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["good".to_string()],
        },
        ethos: RaceAccessRule::Restricted {
            values: vec!["neutral".to_string(), "orderly".to_string()],
        },
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy {
            clauses: vec![
                ClassEligibilityClause {
                    match_all: false,
                    race_ids: None,
                    alignments: Some(vec!["good".to_string()]),
                    ethoses: Some(vec!["orderly".to_string()]),
                },
                ClassEligibilityClause {
                    match_all: false,
                    race_ids: None,
                    alignments: Some(vec!["good".to_string()]),
                    ethoses: Some(vec!["neutral".to_string()]),
                },
            ],
        },
        fount: ClassFount::Supplication,
        empowerment: true,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.storm_giant",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.dwarf",
                "race.elf",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["mace", "polearm", "sword", "staff"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.metal".to_string(), "armor.light".to_string()],
        leveled_abilities: paladin_abilities(),
        specializations: paladin_dedications(),
        specialization_choice_levels: vec![10],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.paladin.lamplight".to_string(),
            signature_vfx: vec![
                "vfx.paladin.lay_hands_glow".to_string(),
                "vfx.paladin.wrath_smite".to_string(),
                "vfx.paladin.sanctuary_shield".to_string(),
                "vfx.paladin.holy_word_burst".to_string(),
            ],
            palette_notes: "Lamp-gold and iron-grey palette; no copied class iconography."
                .to_string(),
            animation_notes: "Dedication-path VFX (shield-slam, two-hand overhead, monk unarmed \
                 stance, champion banner-call) layered onto shared biped skeleton animations."
                .to_string(),
        },
        implementation_notes: vec![
            "Azure-touched is named in the CF race text but has no authored RaceSpec in this \
             roster yet; it is simply absent from the allow accounting here."
                .to_string(),
            "Alignment/ethos gate: CF states 'Paladins can be either Orderly Good or Neutral \
             Good' — captured as alignment=Restricted{good} and ethos=Restricted{neutral, \
             orderly}; RaceAccessRule cannot express the cross-product (orderly-good OR \
             neutral-good, excluding e.g. chaotic-good), only independent gates on each axis. \
             Filed as a framework gap."
                .to_string(),
            "The paladin-code worship loop ('must continue to periodically seek out their deity \
             in order to receive further supplications') is not modeled; only the class-level \
             empowerment=true flag is captured. Wiring to the empowerment system is #948."
                .to_string(),
            "Supplications use AbilityKind::Supplication; skills use AbilityKind::Skill, per the \
             #961 ability-framework kind taxonomy."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (STR), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn paladin_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Mace",
            "Polearm",
            "Sword",
            "Staff",
            "Hand to Hand",
            "Parry",
            "Rescue",
            "Recall",
            "Shield Block",
        ],
    ));
    a.extend(g(2, &["Anoint Weapon"]));
    a.extend(g(3, &["First Aid", "Metal Armor Use"]));
    a.extend(g(4, &["Enhanced Damage"]));
    a.extend(g(6, &["Second Attack"]));
    a.extend(g(7, &["Fast Healing"]));
    a.extend(g(9, &["Aim", "Light Armor Use"]));
    a.extend(g(
        10,
        &["Charge", "Disarm", "Shield Bash", "Lay Hands", "Pen"],
    ));
    a.extend(d(10, "spec.paladin.shield", &["Shield Mastery"]));
    a.extend(d(10, "spec.paladin.two_hand", &["Strike of Faith"]));
    a.extend(d(10, "spec.paladin.monk", &["Avoid"]));
    a.extend(d(10, "spec.paladin.champion", &["Bat"]));
    a.extend(g(12, &["Haggle"]));
    a.extend(g(13, &["Third Attack"]));
    a.extend(g(14, &["Meditation", "Lore"]));
    a.extend(d(15, "spec.paladin.shield", &["Shield Jab"]));
    a.extend(d(15, "spec.paladin.champion", &["Fend"]));
    a.extend(d(20, "spec.paladin.two_hand", &["Strike of Purity"]));
    a.extend(d(20, "spec.paladin.monk", &["Transcend"]));
    a.extend(g(22, &["Pugil"]));
    a.extend(g(23, &["Slice"]));
    a.extend(d(23, "spec.paladin.monk", &["Defy"]));
    a.extend(d(24, "spec.paladin.shield", &["Flank Attack"]));
    a.extend(d(24, "spec.paladin.champion", &["Maneuver"]));
    a.extend(d(25, "spec.paladin.two_hand", &["Templars Defense"]));
    a.extend(d(25, "spec.paladin.monk", &["Respond"]));
    a.extend(g(28, &["Track the Wicked"]));
    a.extend(d(30, "spec.paladin.two_hand", &["Strike of Deliverance"]));
    a.extend(d(30, "spec.paladin.monk", &["Intensify"]));
    a.extend(d(31, "spec.paladin.monk", &["Ascend"]));
    a.extend(d(31, "spec.paladin.champion", &["Marshal"]));
    a.extend(d(32, "spec.paladin.shield", &["Angels Wing"]));
    a.extend(g(35, &["Evade"]));
    a.extend(d(35, "spec.paladin.monk", &["Anticipate"]));

    a.extend(sup(1, &["Detect Evil", "Mend Wounds"]));
    a.extend(sup(3, &["Create Food"]));
    a.extend(sup(4, &["Create Water", "Know Alignment"]));
    a.extend(sup(5, &["Detect Consecration"]));
    a.extend(sup(6, &["Bless", "Detect Invis"]));
    a.extend(sup(7, &["Cure Blindness"]));
    a.extend(sup(10, &["Flamestrike"]));
    a.extend(sup(11, &["Call Lightning", "Protection"]));
    a.extend(sup(13, &["Remove Curse"]));
    a.extend(sup(15, &["Haven"]));
    a.extend(sup(17, &["Cure Deafness"]));
    a.extend(sup(19, &["Blessed Divination"]));
    a.extend(sup(20, &["Summon"]));
    a.extend(sup(21, &["Wrath"]));
    a.extend(sup(22, &["Crusade"]));
    a.extend(sup(24, &["Remove Taint"]));
    a.extend(sup(25, &["Turn Undead"]));
    a.extend(sup(27, &["Sanctuary"]));
    a.extend(sup(29, &["Heal"]));
    a.extend(sup(32, &["Word of Recall", "Fervor"]));
    a.extend(sup(33, &["Righteousness"]));
    a.extend(sup(37, &["Redemption"]));
    a.extend(sup(38, &["Holy Word"]));
    a.extend(sup(40, &["Sanctify Room"]));
    a
}

fn paladin_dedications() -> Vec<ClassSpecialization> {
    [
        ("spec.paladin.shield", "Shield Dedication"),
        ("spec.paladin.two_hand", "Two-Handed Dedication"),
        ("spec.paladin.monk", "Monk Dedication"),
        ("spec.paladin.champion", "Champion Dedication"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: 10,
        description: format!("{name}: a combat path a Lampbearer dedicates to at level 10."),
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

fn sup(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: slug(name),
            display_name: name.to_string(),
            kind: AbilityKind::Supplication,
            specialization_ids: vec![],
            acquisition: None,
        })
        .collect()
}

fn d(level: u16, spec_id: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
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
