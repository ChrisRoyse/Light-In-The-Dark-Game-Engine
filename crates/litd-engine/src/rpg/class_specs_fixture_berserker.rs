// Factual mechanics transcribed from wiki.carrionfields.net/class/berserker (issue #1025
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const BERSERKER_CLASS_ID: &str = "class.reach_ravager";

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

// CF (verbatim, #1025): "Berserkers ... are the innate class of all player orcs." Race = Orc
// only.
pub(crate) fn berserker_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: BERSERKER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Reach-Ravager".to_string(),
        player_facing_canon_name: "Reach-Ravager".to_string(),
        source_issue: 1025,
        canon_issues: vec![986, 1025],
        mechanics_source_url: "https://wiki.carrionfields.net/class/berserker".to_string(),
        source_history_label: "CF Berserker factual mechanics source only".to_string(),
        concept_summary: "The innate warband fury of the orc clans: cowardly at heart but \
             fearless in the thrashing of battle, adapting past level 35 into one of four clan \
             traditions, with a single Clan Chief per clan holding the horde-command abilities."
            .to_string(),
        prime_stat: PrimeStat::Strength,
        stat_minimums: AttributeScores::new(13, 0, 0, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["evil".to_string()],
        },
        ethos: RaceAccessRule::Restricted {
            values: vec!["neutral".to_string(), "chaotic".to_string()],
        },
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Restricted {
                values: vec!["evil".to_string()],
            },
            &RaceAccessRule::Restricted {
                values: vec!["neutral".to_string(), "chaotic".to_string()],
            },
        ),
        fount: ClassFount::Martial,
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: vec!["race.orc".to_string()],
        },
        allowed_weapons: [
            "axe",
            "dagger",
            "flail",
            "mace",
            "polearm",
            "spear",
            "sword",
            "whip",
            "hand_to_hand",
        ]
        .into_iter()
        .map(|w| format!("weapon.{w}"))
        .collect(),
        allowed_armor: vec![
            "armor.metal".to_string(),
            "armor.bone".to_string(),
            "armor.light".to_string(),
        ],
        leveled_abilities: berserker_abilities(),
        specializations: berserker_adaptations(),
        specialization_choice_levels: vec![35],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.berserker.warband_fury".to_string(),
            signature_vfx: vec![
                "vfx.berserker.berserk_rage_aura".to_string(),
                "vfx.berserker.charge_dust".to_string(),
                "vfx.berserker.bloodletting_spray".to_string(),
                "vfx.berserker.headbutt_impact".to_string(),
            ],
            palette_notes: "Ash-grey and blood-rust warband palette; no copied class iconography."
                .to_string(),
            animation_notes: "Adaptation-path VFX (Mundunugu ritual smoke, Skrugga evasive \
                 flinch, Shig-Ru rush blur, Mamlauk stone-hand impact) layered onto shared biped \
                 skeleton animations."
                .to_string(),
        },
        implementation_notes: vec![
            "CF DISCREPANCY (captured verbatim, not resolved here): the General-Info box lists \
             Prime Stat Strength, but the class description states 'Berserkers have no prime \
             statistic like the other classes, since they are the innate class of all player \
             orcs.' The General-Info value (Strength) is used as authoritative-of-source, \
             matching the same discrepancy-handling convention as the race-batch Orc XP \
             conflict."
                .to_string(),
            "The level-51 Clan Chief abilities (Assemble Horde, Demand Tribute, Rally Forces) \
             are captured as normal leveled abilities; the CF constraint 'only one berserker in \
             any given clan can be level 51 (Chieftain) at any given time' is a social/roster \
             rule with no field in ClassSpec. Filed as a framework gap."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (STR), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn berserker_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Axe",
            "Dagger",
            "Flail",
            "Mace",
            "Polearm",
            "Spear",
            "Sword",
            "Whip",
            "Enhanced Damage",
            "Hand to Hand",
            "Recall",
        ],
    ));
    a.extend(g(2, &["Dirt Kicking", "Elbow", "Scavenging"]));
    a.extend(g(3, &["Knee", "Metal Armor Use"]));
    a.extend(g(4, &["Kick", "Bone Armor Use"]));
    a.extend(g(5, &["Trip", "Fast Healing", "Forced March"]));
    a.extend(g(6, &["Berserk", "Parry"]));
    a.extend(g(7, &["Fallback"]));
    a.extend(g(8, &["Aim"]));
    a.extend(g(9, &["Cheap Shot"]));
    a.extend(g(10, &["Second Attack", "Pen", "Warcry"]));
    a.extend(g(11, &["Light Armor Use"]));
    a.extend(g(13, &["Footstomp"]));
    a.extend(g(14, &["Hide"]));
    a.extend(g(15, &["Bash", "Dual Wield", "Exsanguinate"]));
    a.extend(g(16, &["Dodge"]));
    a.extend(g(17, &["Thrust"]));
    a.extend(g(18, &["Butcher"]));
    a.extend(g(19, &["Toughness", "Slice"]));
    a.extend(g(20, &["Third Attack", "Shield Cleave", "Headbutt"]));
    a.extend(g(21, &["Charge"]));
    a.extend(g(22, &["Bloodletting", "Pierce"]));
    a.extend(g(23, &["Crushing Assault"]));
    a.extend(g(24, &["Lash"]));
    a.extend(g(25, &["Steal", "Bloodsteep"]));
    a.extend(g(26, &["War Shout", "Evade"]));
    a.extend(g(27, &["Spinebreaker"]));
    a.extend(g(28, &["Last Stand"]));
    a.extend(g(
        29,
        &["Opportunity Strike", "Ingest Spoils", "Regurgitate"],
    ));
    a.extend(g(30, &["Demoralize", "Grapple Weapon"]));
    a.extend(g(31, &["Pin"]));
    a.extend(g(32, &["Savage Feeding"]));
    a.extend(g(33, &["Offhand Grapple"]));
    a.extend(g(34, &["Fashion War Banner"]));
    a.extend(g(35, &["Trample", "Enslave"]));
    a.extend(p(35, "spec.berserker.shigru", &["Rush"]));
    a.extend(p(35, "spec.berserker.mundunugu", &["Soot Ritual"]));
    a.extend(p(35, "spec.berserker.mamlauk", &["Whomp"]));
    a.extend(p(35, "spec.berserker.skrugga", &["Cower"]));
    a.extend(g(36, &["Bully"]));
    a.extend(p(37, "spec.berserker.mamlauk", &["Stone Hands"]));
    a.extend(p(38, "spec.berserker.mundunugu", &["Blood Ritual"]));
    a.extend(p(38, "spec.berserker.skrugga", &["Improved Flee"]));
    a.extend(g(39, &["Track Prey"]));
    a.extend(g(40, &["Desecration", "Set Snares"]));
    a.extend(p(40, "spec.berserker.shigru", &["Willbreaker"]));
    a.extend(p(40, "spec.berserker.mamlauk", &["Swat"]));
    a.extend(p(40, "spec.berserker.skrugga", &["Take Cover"]));
    a.extend(g(42, &["Skulk"]));
    a.extend(p(42, "spec.berserker.skrugga", &["Staff Defense"]));
    a.extend(p(43, "spec.berserker.shigru", &["Pillage"]));
    a.extend(p(43, "spec.berserker.mundunugu", &["Sanguine Adaptation"]));
    a.extend(p(43, "spec.berserker.mamlauk", &["Brute Force"]));
    a.extend(g(44, &["Grit"]));
    a.extend(p(45, "spec.berserker.mundunugu", &["Fetish"]));
    a.extend(p(45, "spec.berserker.skrugga", &["Dirty Fighting"]));
    a.extend(p(46, "spec.berserker.shigru", &["Masochism"]));
    a.extend(p(46, "spec.berserker.mamlauk", &["Hack"]));
    a.extend(g(48, &["Dash"]));
    a.extend(p(48, "spec.berserker.shigru", &["Punt"]));
    a.extend(p(48, "spec.berserker.mundunugu", &["Gravesleep"]));
    a.extend(g(49, &["Smear"]));
    a.extend(g(50, &["Overrun"]));
    a.extend(g(51, &["Assemble Horde", "Demand Tribute", "Rally Forces"]));
    a
}

fn berserker_adaptations() -> Vec<ClassSpecialization> {
    [
        ("spec.berserker.mundunugu", "Mundunugu"),
        ("spec.berserker.skrugga", "Skrugga"),
        ("spec.berserker.shigru", "Shig-Ru"),
        ("spec.berserker.mamlauk", "Mamlauk"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: 35,
        description: format!(
            "{name}: a level-35 adaptation path, granting strength and \
             cunning beyond the peon-level berserker."
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
