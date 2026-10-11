// Factual mechanics transcribed from wiki.carrionfields.net/class/assassin (issue #1022 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const ASSASSIN_CLASS_ID: &str = "class.hushblade";

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

// CF race restriction (verbatim, #1022): "The skills of an Assassin can only be learned by
// Humans, Half-Elves, Half-Drows, Dark-Elves, Arials, and Felar." — the most restrictive
// whitelist in this cluster (6 races).
pub(crate) fn assassin_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: ASSASSIN_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Hushblade".to_string(),
        player_facing_canon_name: "Hushblade".to_string(),
        source_issue: 1022,
        canon_issues: vec![986, 1022],
        mechanics_source_url: "https://wiki.carrionfields.net/class/assassin".to_string(),
        source_history_label: "CF Assassin factual mechanics source only".to_string(),
        concept_summary: "Killers who deliver a death that stays unspoken: pressure-point \
             strikes, a linear unarmed kick-chain, and marked-target tradecraft."
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
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.sylphkin",
                "race.felisar",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "spear", "sword", "staff", "hand_to_hand"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string()],
        leveled_abilities: assassin_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.assassin.pressure_point".to_string(),
            signature_vfx: vec![
                "vfx.assassin.assassinate_flash".to_string(),
                "vfx.assassin.kick_chain_blur".to_string(),
                "vfx.assassin.mark_of_the_prey_glyph".to_string(),
                "vfx.assassin.poison_smoke_burst".to_string(),
            ],
            palette_notes: "Ink-black and pale bone palette; no copied class iconography."
                .to_string(),
            animation_notes:
                "The 10-step kick chain (wheel -> sweep -> side -> scissor -> mule -> crescent -> \
                 axe -> mountain storm -> double spin -> rising phoenix) is one connected combo \
                 animation tree, escalating in speed/flourish per unlock."
                    .to_string(),
        },
        implementation_notes: vec![
            "CF's kick chain caps each new kick at 91% of the prior kick's success chance; \
             ClassSpec has no per-ability probability-modifier field, so only the unlock level \
             and chain order are captured here. Filed as a framework gap."
                .to_string(),
            "No subclass/path/legacy system exists on the CF Assassin class page (confirmed \
             absent, unlike Warrior/Thief/Ranger); specializations/legacies are intentionally \
             empty, not an omission."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (DEX), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn assassin_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Dagger",
            "Spear",
            "Sword",
            "Staff",
            "Dodge",
            "Hand to Hand",
            "Recall",
            "Wheel Kick",
        ],
    ));
    a.extend(g(3, &["Dirt Kicking"]));
    a.extend(g(4, &["Hide", "Sneak"]));
    a.extend(g(5, &["Aim", "Dark Vision"]));
    a.extend(g(6, &["Sweep Kick"]));
    a.extend(g(7, &["Evade", "Heightened Awareness"]));
    a.extend(g(8, &["Trip", "Light Armor Use"]));
    a.extend(g(9, &["Fast Healing", "Bind Wounds"]));
    a.extend(g(
        10,
        &[
            "Detect Hidden",
            "Second Attack",
            "Pen",
            "Endure",
            "Mark of the Prey",
        ],
    ));
    a.extend(g(11, &["Parry", "Side Kick"]));
    a.extend(g(12, &["Shield Block"]));
    a.extend(g(13, &["Disarm", "Caltraps"]));
    a.extend(g(14, &["Pick Lock", "Lore", "Pierce"]));
    a.extend(g(15, &["Meditation", "Unarmed Defense"]));
    a.extend(g(16, &["Vanish", "Scissor Kick"]));
    a.extend(g(17, &["Haggle", "Throw"]));
    a.extend(g(18, &["Blindness Dust"]));
    a.extend(g(20, &["Poison Smoke", "Nerve", "Strangle", "Locate Mark"]));
    a.extend(g(21, &["Enhanced Damage", "Mule Kick"]));
    a.extend(g(22, &["Kotegaeshi"]));
    a.extend(g(23, &["Dual Wield"]));
    a.extend(g(24, &["Ground Control"]));
    a.extend(g(25, &["Pugil", "Third Attack", "Poison Dagger"]));
    a.extend(g(26, &["Crescent Kick"]));
    a.extend(g(27, &["Tiger Claw"]));
    a.extend(g(28, &["Kansetsuwaza"]));
    a.extend(g(29, &["Precise Aim"]));
    a.extend(g(30, &["Backfist"]));
    a.extend(g(31, &["Axe Kick"]));
    a.extend(g(35, &["Assassinate", "Stalk", "Poison Darts"]));
    a.extend(g(36, &["Mountain Storm Kick"]));
    a.extend(g(40, &["Owaza"]));
    a.extend(g(41, &["Double Spin Kick"]));
    a.extend(g(44, &["Martial Trance"]));
    a.extend(g(46, &["Rising Phoenix Kick"]));
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
