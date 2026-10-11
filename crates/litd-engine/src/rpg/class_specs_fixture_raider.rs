// Factual mechanics transcribed from wiki.carrionfields.net/class/raider (issue #1026 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const RAIDER_CLASS_ID: &str = "class.reach_whelp_raider";

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

// CF (verbatim, #1026): "Goblins are trained up from whelps into ... the title of 'Raider'."
// Race = Goblin only (innate class, like Berserker/Orc).
pub(crate) fn raider_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: RAIDER_CLASS_ID.to_string(),
        max_level: 40,
        display_name: "Reach-Whelp Raider".to_string(),
        player_facing_canon_name: "Reach-Whelp Raider".to_string(),
        source_issue: 1026,
        canon_issues: vec![986, 1026],
        mechanics_source_url: "https://wiki.carrionfields.net/class/raider".to_string(),
        source_history_label: "CF Raider factual mechanics source only".to_string(),
        concept_summary: "An unrefined mix of berserker and thief: crude, sneaky warfare without \
             the size and strength of the orc clans, loosely affiliated with orc warbands but \
             hard-capped at 40th rank of training."
            .to_string(),
        prime_stat: PrimeStat::Dexterity,
        stat_minimums: AttributeScores::new(0, 0, 0, 13, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["neutral".to_string(), "evil".to_string()],
        },
        ethos: RaceAccessRule::Restricted {
            values: vec!["neutral".to_string(), "chaotic".to_string()],
        },
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Restricted {
                values: vec!["neutral".to_string(), "evil".to_string()],
            },
            &RaceAccessRule::Restricted {
                values: vec!["neutral".to_string(), "chaotic".to_string()],
            },
        ),
        fount: ClassFount::Martial,
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: vec!["race.goblin".to_string()],
        },
        allowed_weapons: ["axe", "dagger", "spear", "sword", "bow"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string(), "armor.bone".to_string()],
        leveled_abilities: raider_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.raider.warband_skulk".to_string(),
            signature_vfx: vec![
                "vfx.raider.raid_dust_charge".to_string(),
                "vfx.raider.backstab_flash".to_string(),
                "vfx.raider.poison_arrow_trail".to_string(),
                "vfx.raider.flee_scatter".to_string(),
            ],
            palette_notes: "Rust-brown and dirty-hide palette; no copied class iconography."
                .to_string(),
            animation_notes: "Small-frame biped skeleton with mounted-charge and rapid-shot \
                 archery animation variants."
                .to_string(),
        },
        implementation_notes: vec![
            "OPEN QUESTION (unresolved by this capture, per #1026's own flag): no Goblin people \
             exists in the Ashen Reach lore docs, so 'Reach-Whelp Raider' is a working \
             placeholder name, not a confirmed originalization. Do not treat as final."
                .to_string(),
            "CF hard-caps Raider advancement at rank 40 ('after which no further progress is \
             possible'); ClassSpec has no max-level field, so this is only implicitly reflected \
             by the highest leveled_abilities entry (40). Filed as a framework gap."
                .to_string(),
            "No path/subclass/legacy system exists on the CF Raider class page (the Raider Edge \
             List was not retrieved on this pass); specializations/legacies are intentionally \
             empty."
                .to_string(),
            "CF-named mounts (Wargs, Giant Spiders) have no confirmed Ashen Reach analog and are \
             not modeled or fabricated here."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (DEX), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn raider_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &["Axe", "Dagger", "Spear", "Sword", "Haggle", "Recall"],
    ));
    a.extend(g(2, &["Elbow", "Light Armor Use", "Scavenging"]));
    a.extend(g(3, &["Dirt Kicking", "Knee"]));
    a.extend(g(4, &["Kick"]));
    a.extend(g(5, &["Sneak"]));
    a.extend(g(6, &["Dodge"]));
    a.extend(g(9, &["Hand to Hand", "Parry", "Fallback"]));
    a.extend(g(10, &["Trip", "Pen"]));
    a.extend(g(11, &["Hide"]));
    a.extend(g(12, &["Steal"]));
    a.extend(g(13, &["Second Attack"]));
    a.extend(g(15, &["Archery", "Dual Wield"]));
    a.extend(g(16, &["Fast Healing"]));
    a.extend(g(17, &["Pierce"]));
    a.extend(g(20, &["Detect Traps", "Thrust"]));
    a.extend(g(21, &["Charge"]));
    a.extend(g(22, &["Third Attack", "Bloodletting"]));
    a.extend(g(23, &["Butcher"]));
    a.extend(g(24, &["Poison Arrow"]));
    a.extend(g(
        25,
        &["Enhanced Damage", "Toughness", "Cheap Shot", "Bloodsteep"],
    ));
    a.extend(g(26, &["Evade", "Ululate"]));
    a.extend(g(27, &["Skirt", "Track Prey"]));
    a.extend(g(28, &["Flaming Arrow", "Bone Armor Use"]));
    a.extend(g(29, &["Artful Prey"]));
    a.extend(g(30, &["Grapple Weapon"]));
    a.extend(g(32, &["Rapid Shot"]));
    a.extend(g(33, &["Backstab"]));
    a.extend(g(35, &["Raid"]));
    a.extend(g(39, &["Improved Flee"]));
    a.extend(g(40, &["Aerial Shot", "Dash"]));
    a
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
