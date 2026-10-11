// Factual mechanics transcribed from wiki.carrionfields.net/en/class/anti-paladin (issue #1028
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const ANTI_PALADIN_CLASS_ID: &str = "class.blackoath_knight";

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

// CF race restriction (verbatim, #1028): "The abilities of an Anti-Paladin may only be learned
// by Humans, Dark-Elves, Half-Elves, Half-Drows, Arials, Felar, Duergar, Fire Giants, and
// Saurians."
pub(crate) fn anti_paladin_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: ANTI_PALADIN_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Blackoath Knight".to_string(),
        player_facing_canon_name: "Blackoath Knight".to_string(),
        source_issue: 1028,
        canon_issues: vec![986, 1028],
        mechanics_source_url: "https://wiki.carrionfields.net/en/class/anti-paladin".to_string(),
        source_history_label: "CF Anti-Paladin factual mechanics source only".to_string(),
        concept_summary: "A mailed champion who has inverted the knightly vow, swearing to \
             appetite and the Outer Dark: superb fighting skills paired with unholy spellcraft \
             that fails the moment evil is forsaken."
            .to_string(),
        prime_stat: PrimeStat::Strength,
        stat_minimums: AttributeScores::new(13, 0, 0, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["evil".to_string()],
        },
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Restricted {
                values: vec!["evil".to_string()],
            },
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Supplication,
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.ashkin_deep",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.sylphkin",
                "race.felisar",
                "race.onyx_kin",
                "race.fire_giant",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: [
            "axe", "dagger", "flail", "mace", "polearm", "spear", "sword", "whip",
        ]
        .into_iter()
        .map(|w| format!("weapon.{w}"))
        .collect(),
        allowed_armor: vec![
            "armor.metal".to_string(),
            "armor.light".to_string(),
            "armor.mystical".to_string(),
        ],
        leveled_abilities: anti_paladin_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.anti_paladin.blackoath".to_string(),
            signature_vfx: vec![
                "vfx.anti_paladin.aura_of_despair".to_string(),
                "vfx.anti_paladin.fireball_burst".to_string(),
                "vfx.anti_paladin.cleave_impact".to_string(),
                "vfx.anti_paladin.unholy_blessing_glow".to_string(),
            ],
            palette_notes: "Bruise-purple and pitch-black palette; no copied class iconography."
                .to_string(),
            animation_notes: "Shared armored biped skeleton with unholy-spellcraft cast \
                 animations layered over martial attack VFX."
                .to_string(),
        },
        implementation_notes: vec![
            "CF DISCREPANCY (captured verbatim, not resolved here): issue #1028's body states \
             empowerment=Yes, but the CF class page's General-Info block states Empowerment: \
             No. The CF page value is used as authoritative-of-source (No), matching the same \
             discrepancy-handling convention as the race-batch Orc XP conflict; flagged for \
             operator review."
                .to_string(),
            "No path/subclass/legacy system exists on the CF Anti-Paladin class page (the \
             Anti-Paladin Edge List, id=1970, was not retrieved on this pass); \
             specializations/legacies are intentionally empty."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (STR), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn anti_paladin_abilities() -> Vec<ClassLeveledAbility> {
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
            "Parry",
            "Scrolls",
            "Wands",
            "Recall",
            "Shield Block",
        ],
    ));
    a.extend(g(3, &["Dirt Kicking"]));
    a.extend(g(5, &["Elbow"]));
    a.extend(g(6, &["Knee"]));
    a.extend(g(7, &["Second Attack", "Fast Healing"]));
    a.extend(g(8, &["Haggle", "Aim"]));
    a.extend(g(9, &["Kick"]));
    a.extend(g(10, &["Trip", "Pen", "Pick Lock"]));
    a.extend(g(11, &["Disarm"]));
    a.extend(g(12, &["Metal Armor Use"]));
    a.extend(g(13, &["Third Attack"]));
    a.extend(g(14, &["Hand to Hand"]));
    a.extend(g(15, &["Bash", "Lash", "Meditation", "Dual Wield"]));
    a.extend(g(16, &["Lore"]));
    a.extend(g(17, &["Shield Cleave", "Light Armor Use"]));
    a.extend(g(18, &["Thrust"]));
    a.extend(g(19, &["Pierce"]));
    a.extend(g(20, &["Charge", "Slice"]));
    a.extend(g(23, &["Faceslash"]));
    a.extend(g(25, &["Cleave"]));
    a.extend(g(26, &["Lashes of the Slave"]));
    a.extend(g(28, &["Vault", "Mystical Armor Use"]));
    a.extend(g(29, &["Crippling Strike"]));
    a.extend(g(30, &["Demand"]));
    a.extend(g(33, &["Charge of the Morosa"]));
    a.extend(g(43, &["Evade"]));

    a.extend(sp(2, &["Chill Touch"]));
    a.extend(sp(4, &["Blindness"]));
    a.extend(sp(5, &["Detect Magic"]));
    a.extend(sp(6, &["Poison"]));
    a.extend(sp(8, &["Curse", "Protection"]));
    a.extend(sp(9, &["Invis"]));
    a.extend(sp(10, &["Detect Invis"]));
    a.extend(sp(12, &["Teleport"]));
    a.extend(sp(13, &["Lightning Bolt"]));
    a.extend(sp(16, &["Energy Drain"]));
    a.extend(sp(19, &["Excision"]));
    a.extend(sp(21, &["Summon"]));
    a.extend(sp(22, &["Deafen"]));
    a.extend(sp(24, &["Power Word Despondence"]));
    a.extend(sp(25, &["Plague", "Sleep"]));
    a.extend(sp(27, &["Fireball"]));
    a.extend(sp(31, &["Aura of Despair"]));
    a.extend(sp(32, &["Iceball"]));
    a.extend(sp(34, &["Bloodlust of the Twelve Fiends"]));
    a.extend(sp(35, &["Enervating Spirits"]));
    a.extend(sp(36, &["Unholy Blessing", "Leech Unholy Blessing"]));
    a.extend(sp(40, &["Iron Grip of Azazel"]));
    a.extend(sp(42, &["Worldbind"]));
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
