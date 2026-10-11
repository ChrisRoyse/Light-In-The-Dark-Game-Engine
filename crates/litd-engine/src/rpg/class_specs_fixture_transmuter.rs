// Factual mechanics transcribed from wiki.carrionfields.net/class/transmuter (issue #1033
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const TRANSMUTER_CLASS_ID: &str = "class.sinewright";

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

// CF race restriction (verbatim, #1033): "The races that may become transmuters are Arial,
// Dark-Elf, Elf, Gnome, Half-Drow, Half-Elf, Human, Svirfnebli, and Saurian."
pub(crate) fn transmuter_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: TRANSMUTER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Sinewright".to_string(),
        player_facing_canon_name: "Sinewright".to_string(),
        source_issue: 1033,
        canon_issues: vec![986, 1033],
        mechanics_source_url: "https://wiki.carrionfields.net/class/transmuter".to_string(),
        source_history_label: "CF Transmuter factual mechanics source only".to_string(),
        concept_summary: "Mages who alter existing things rather than invoke new energy: bodily \
             enhancement and painful body-twisting drawn from the same small pressure that \
             changes a life forever."
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
        fount: ClassFount::Invocation,
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.sylphkin",
                "race.ashkin_deep",
                "race.elf",
                "race.gnome",
                "race.ashkin_mixed",
                "race.half_elf",
                "race.ashen_human",
                "race.deep_kin",
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
        leveled_abilities: transmuter_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.transmuter.sinewright".to_string(),
            signature_vfx: vec![
                "vfx.transmuter.adrenal_surge_pulse".to_string(),
                "vfx.transmuter.calcify_stiffen".to_string(),
                "vfx.transmuter.repair_flesh_mend".to_string(),
                "vfx.transmuter.disrupt_flesh_twist".to_string(),
            ],
            palette_notes: "Sinew-red and bone-white palette; no copied class iconography."
                .to_string(),
            animation_notes: "Hands-on-target alteration cast animation shared by both buff \
                 (enhancement) and debuff (twisting) spell families."
                .to_string(),
        },
        implementation_notes: vec![
            "CF explicitly states no path/circle/school subsystem exists for this class (unlike \
             Invoker's paths or Conjurer's circles); specializations/legacies are intentionally \
             empty, not an omission."
                .to_string(),
            "Per-spell effect magnitudes live behind separate CF helpsearch.php links not \
             retrieved on the capture pass; only names and unlock levels are factual here."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (INT), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn transmuter_abilities() -> Vec<ClassLeveledAbility> {
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
    a.extend(g(8, &["Haggle"]));
    a.extend(g(10, &["Pen"]));
    a.extend(g(13, &["Mystical Armor Use"]));
    a.extend(g(14, &["Trance", "Parry", "Hand to Hand"]));
    a.extend(g(15, &["Fast Healing"]));
    a.extend(g(20, &["Pugil", "Lash"]));
    a.extend(g(25, &["Second Attack"]));
    a.extend(g(33, &["Metabolic Unity"]));

    a.extend(sp(1, &["Detect Magic", "Bioluminescence"]));
    a.extend(sp(2, &["Relaxation", "Crack Flesh"]));
    a.extend(sp(3, &["Ocular Enhancement"]));
    a.extend(sp(4, &["Invis", "Desensitize"]));
    a.extend(sp(5, &["Adrenal Surge"]));
    a.extend(sp(6, &["Muscular Lethargy"]));
    a.extend(sp(7, &["Toughened Skin"]));
    a.extend(sp(9, &["Adrenal Burst"]));
    a.extend(sp(10, &["Fly", "Identify"]));
    a.extend(sp(11, &["Giant Strength", "Mass Invis"]));
    a.extend(sp(12, &["Metabolic Quickening"]));
    a.extend(sp(
        13,
        &["Faerie Fog", "Mass Bioluminescence", "Mental Jolt"],
    ));
    a.extend(sp(14, &["Teleport", "Gill Growth", "Mass Relaxation"]));
    a.extend(sp(16, &["Metabolic Slowing"]));
    a.extend(sp(17, &["Dispel Magic", "Gill Removal"]));
    a.extend(sp(18, &["Cancellation", "Mass Lethargy", "Earthbind"]));
    a.extend(sp(20, &["Arcane Absorption"]));
    a.extend(sp(21, &["Accelerate"]));
    a.extend(sp(22, &["Fumble", "Corporeal Softening"]));
    a.extend(sp(23, &["Enlarge"]));
    a.extend(sp(24, &["Pass Door", "Calcify"]));
    a.extend(sp(25, &["Corporeal Hardening", "Disrupt Flesh"]));
    a.extend(sp(26, &["Reduce"]));
    a.extend(sp(27, &["Repair Flesh", "Petrify Flesh"]));
    a.extend(sp(28, &["Word of Recall", "Decalcify"]));
    a.extend(sp(29, &["Disrupt Muscle"]));
    a.extend(sp(31, &["Mass Metabolic Slowing", "Augment Toxins"]));
    a.extend(sp(32, &["Disrupt Bone"]));
    a.extend(sp(
        34,
        &["Mass Corporeal Softening", "Repair Bone", "Spiderhands"],
    ));
    a.extend(sp(35, &["Malleability"]));
    a.extend(sp(36, &["Control Translucence", "Duo Dimension"]));
    a.extend(sp(37, &["Neurological Disruption"]));
    a.extend(sp(38, &["Mass Reduce"]));
    a.extend(sp(40, &["Mass Decalcify"]));
    a.extend(sp(41, &["Disrupt Organ"]));
    a.extend(sp(42, &["Buoyancy"]));
    a.extend(sp(43, &["Mass Augment Toxins", "Repair Organ"]));
    a.extend(sp(50, &["Mass Buoyancy"]));
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

fn sp(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    gk(level, AbilityKind::Spell, names)
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
