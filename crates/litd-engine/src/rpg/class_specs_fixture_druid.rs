// Factual mechanics transcribed from wiki.carrionfields.net/en/class/druid (issue #1031
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const DRUID_CLASS_ID: &str = "class.wildbinder";

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

// CF race restriction (verbatim, #1031): "Only Humans, Half-elves, Half-drows, Wood-elves,
// Gnomes, Cloud Giants, Centaurs and Arials may be druids." Centaur has no authored race analog
// in this roster.
pub(crate) fn druid_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: DRUID_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Wildbinder".to_string(),
        player_facing_canon_name: "Wildbinder".to_string(),
        source_issue: 1031,
        canon_issues: vec![986, 1031],
        mechanics_source_url: "https://wiki.carrionfields.net/en/class/druid".to_string(),
        source_history_label: "CF Druid factual mechanics source only".to_string(),
        concept_summary: "Neutral priests of root, storm, and beast: strong under open sky and \
             deep wood, uneasy behind city walls, calling wilderness allies and wearing bark and \
             thorn as armor."
            .to_string(),
        prime_stat: PrimeStat::Wisdom,
        stat_minimums: AttributeScores::new(0, 0, 13, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["neutral".to_string()],
        },
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Restricted {
                values: vec!["neutral".to_string()],
            },
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Supplication,
        empowerment: true,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.wood_elf",
                "race.gnome",
                "race.cloud_giant",
                "race.sylphkin",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["axe", "mace", "spear", "staff"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.bone".to_string(), "armor.light".to_string()],
        leveled_abilities: druid_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.druid.wildbind".to_string(),
            signature_vfx: vec![
                "vfx.druid.spike_growth_roots".to_string(),
                "vfx.druid.treeform_transform".to_string(),
                "vfx.druid.sunray_beam".to_string(),
                "vfx.druid.liveoak_summon".to_string(),
            ],
            palette_notes: "Bark-brown and leaf-green palette; no copied class iconography."
                .to_string(),
            animation_notes: "Treeform/birdform/bearform each swap the biped skeleton for a \
                 dedicated form-model with its own attack/move animation set."
                .to_string(),
        },
        implementation_notes: vec![
            "Centaur is named in the CF race text but has no authored RaceSpec in this roster \
             yet; it is simply absent from the allow accounting here."
                .to_string(),
            "The wilderness-vs-city effectiveness modifier ('uncomfortable and much less \
             effective indoors or within the confines of a city') is not modeled; ClassSpec has \
             no location-context modifier field. Filed as a framework gap."
                .to_string(),
            "Treeform (L20), birdform (L40), and bearform (L45) are the class's shapeshifting \
             forms, captured as ordinary AbilityKind::Supplication rows (CF lists them in the \
             Supplications table, not as a separate DEDICATE tree); no formal specialization \
             system exists for this class."
                .to_string(),
            "OPEN LORE QUESTION (not resolved here, per #1031's own flag): the Ashen Reach \
             pantheon has no dedicated nature/wilderness god, so this priest class's \
             deity-empowerment source is unresolved pending an operator/lore decision."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (WIS), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn druid_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(1, &["Axe", "Mace", "Spear", "Staff", "Recall"]));
    a.extend(g(3, &["Dirt Kicking"]));
    a.extend(g(4, &["Herbal Medicine"]));
    a.extend(g(5, &["Meditation"]));
    a.extend(g(6, &["Bone Armor Use", "Recuperate"]));
    a.extend(g(7, &["Light Armor Use"]));
    a.extend(g(9, &["Fast Healing", "Pathfinding"]));
    a.extend(g(10, &["Pen"]));
    a.extend(g(11, &["Kick", "Trance"]));
    a.extend(g(13, &["Quiet Movement", "Forage"]));
    a.extend(g(14, &["Hand to Hand"]));
    a.extend(g(
        15,
        &["Haggle", "Bark Skin", "Herbal Forage", "Shield Block"],
    ));
    a.extend(g(17, &["Shield Cleave", "Pugil"]));
    a.extend(g(18, &["Parry"]));
    a.extend(g(19, &["Second Attack"]));
    a.extend(g(20, &["Protection Elements"]));
    a.extend(g(21, &["Thrust"]));
    a.extend(g(22, &["Pierce"]));
    a.extend(g(38, &["Call of the Wild"]));

    a.extend(sup(1, &["Mend Wounds"]));
    a.extend(sup(2, &["Detect Consecration", "Carapace"]));
    a.extend(sup(3, &["Refresh", "Decompose"]));
    a.extend(sup(4, &["Detect Evil", "Detect Good"]));
    a.extend(sup(5, &["Detect Invis"]));
    a.extend(sup(6, &["Know Alignment"]));
    a.extend(sup(7, &["Earthquake"]));
    a.extend(sup(8, &["Bless", "Infravision"]));
    a.extend(sup(10, &["Control Weather", "Moonbeam"]));
    a.extend(sup(11, &["Create Spring", "Identify", "Serpentine Staff"]));
    a.extend(sup(12, &["Protection from Metals"]));
    a.extend(sup(13, &["Telluric Surge"]));
    a.extend(sup(14, &["Call Lightning"]));
    a.extend(sup(15, &["Fly", "Armor of Thorns"]));
    a.extend(sup(16, &["Water Breathing"]));
    a.extend(sup(17, &["Entangle", "Animism"]));
    a.extend(sup(18, &["Fire Seeds", "Forest Haven"]));
    a.extend(sup(19, &["Faerie Fog", "Tsunami"]));
    a.extend(sup(20, &["Treeform", "Whirling Simoon"]));
    a.extend(sup(21, &["Frenzy"]));
    a.extend(sup(22, &["Thornheart"]));
    a.extend(sup(23, &["Cancellation"]));
    a.extend(sup(24, &["Spike Growth"]));
    a.extend(sup(25, &["Swarm of Leeches"]));
    a.extend(sup(26, &["Pass Door"]));
    a.extend(sup(28, &["Thorn"]));
    a.extend(sup(29, &["Spores"]));
    a.extend(sup(30, &["Heroes Feast"]));
    a.extend(sup(31, &["Plant Growth"]));
    a.extend(sup(32, &["Briartangle"]));
    a.extend(sup(33, &["Liveoak"]));
    a.extend(sup(34, &["Primordial Vengeance"]));
    a.extend(sup(35, &["Sunray"]));
    a.extend(sup(40, &["Birdform"]));
    a.extend(sup(42, &["The Hunt"]));
    a.extend(sup(45, &["Bearform"]));
    a
}

fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn g(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    gk(level, AbilityKind::Skill, names)
}

fn sup(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    gk(level, AbilityKind::Supplication, names)
}

fn gk(level: u16, kind: AbilityKind, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: slug(name),
            display_name: name.to_string(),
            kind,
            specialization_ids: vec![],
            acquisition: None,
        })
        .collect()
}
