// Factual mechanics transcribed from wiki.carrionfields.net/class/necromancer (issue #1035
// FACTUAL SPEC comment; page recovered via headless render after exa returned an empty body).
// Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLegacy,
    ClassLeveledAbility, ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const NECROMANCER_CLASS_ID: &str = "class.gravebinder";

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

// CF race restriction (verbatim, #1035): "The only races that may learn the necromantic arts
// are Humans, Half-Elves, Half-Drows, Dark-Elves, Arials, and Saurians."
pub(crate) fn necromancer_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: NECROMANCER_CLASS_ID.to_string(),
        max_level: 47,
        display_name: "Gravebinder".to_string(),
        player_facing_canon_name: "Gravebinder".to_string(),
        source_issue: 1035,
        canon_issues: vec![986, 1035],
        mechanics_source_url: "https://wiki.carrionfields.net/class/necromancer".to_string(),
        source_history_label: "CF Necromancer factual mechanics source only".to_string(),
        concept_summary: "Evil-aligned mages who compel the dead back into cold motion and drain \
             negative energy from the living, culminating at rank 47 in a dangerous elixir-borne \
             attempt to Become a deathless Lich."
            .to_string(),
        prime_stat: PrimeStat::Intelligence,
        stat_minimums: AttributeScores::new(0, 13, 0, 0, 0, 0),
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
        fount: ClassFount::Unresolved {
            reason: "Whether the Gravebinder is a Pact/Hollow-fount class or requires a canon \
                     amendment (it collides with the 'dead-stay-dead' cosmology) is an \
                     unresolved operator/lore decision flagged by the #1035 capture."
                .to_string(),
        },
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.sylphkin",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "sword", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.bone".to_string(), "armor.mystical".to_string()],
        leveled_abilities: necromancer_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: vec![ClassLegacy {
            legacy_id: "legacy.necromancer.the_becoming".to_string(),
            display_name: "The Becoming".to_string(),
            description: "CF (verbatim): the elixir of death may only be brewed at 47th rank; \
                 the transformation to Lich is necessary to reach greater power but risks \
                 permanent crippling or utter self-destruction. See HELP LICH (not retrieved)."
                .to_string(),
        }],
        legacy_choice_levels: vec![47],
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.necromancer.gravebind".to_string(),
            signature_vfx: vec![
                "vfx.necromancer.animate_dead_rise".to_string(),
                "vfx.necromancer.vampiric_touch_drain".to_string(),
                "vfx.necromancer.wraithform_shimmer".to_string(),
                "vfx.necromancer.becoming_transfiguration".to_string(),
            ],
            palette_notes: "Necrotic green and bone-grey palette; no copied class iconography."
                .to_string(),
            animation_notes: "Undead-pet ladder (reanimate -> corpse thrall -> lesser golem -> \
                 animate dead -> control undead -> greater undead) swaps in progressively larger \
                 summon models; the Becoming plays a one-time transfiguration animation."
                .to_string(),
        },
        implementation_notes: vec![
            "MAJOR CANON FLAG (not resolved here, per #1035's own instruction): this archetype \
             collides with Ashen Reach's 'dead-stay-dead' cosmology and the 'do not make Hollow \
             horrors ordinary undead' injunction. Whether the Gravebinder is authored as a \
             Pact/Hollow-fount class (recommended by the capture) or requires a canon amendment \
             is an open operator/lore decision, not resolved by this ClassSpec row."
                .to_string(),
            "The level-47 'Becoming' capstone is captured as a single ClassLegacy with \
             legacy_choice_levels=[47]; the actual Lich transformation mechanics (CON reset to \
             6, ritual request, physical-kill-only mortality) already exist as an implemented \
             engine bit, litd.rpg.undeath.v1 (#1061, commit d097454e), and must be wired to \
             that bit rather than re-implemented."
                .to_string(),
            "The undead-pet capability ladder (reanimate/corpse thrall/lesser golem/animate \
             dead/control undead/greater undead) is an unmarked CF progression, not a formally \
             named path system; captured as plain leveled Spell abilities."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (INT), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn necromancer_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Dagger",
            "Sword",
            "Whip",
            "Scrolls",
            "Talismans",
            "Wands",
            "Recall",
            "Spellcraft",
        ],
    ));
    a.extend(g(7, &["Meditation"]));
    a.extend(g(9, &["Haggle"]));
    a.extend(g(10, &["Pen"]));
    a.extend(g(12, &["Parry"]));
    a.extend(g(14, &["Bone Armor Use", "Trance"]));
    a.extend(g(15, &["Fast Healing"]));
    a.extend(g(19, &["Embalm"]));
    a.extend(g(20, &["Hand to Hand", "Second Attack", "Lash"]));
    a.extend(g(21, &["Mystical Armor Use"]));
    a.extend(g(27, &["Repel Undead"]));
    a.extend(g(38, &["Speak with Dead"]));

    a.extend(sp(1, &["Chill Touch", "Detect Magic"]));
    a.extend(sp(2, &["Torment Spirit"]));
    a.extend(sp(3, &["Detect Invis", "Ghoulish Numbing"]));
    a.extend(sp(4, &["Invis"]));
    a.extend(sp(5, &["Armor of Living Bone"]));
    a.extend(sp(6, &["Faerie Fire"]));
    a.extend(sp(8, &["Reanimate"]));
    a.extend(sp(9, &["Infravision"]));
    a.extend(sp(10, &["Sleep", "Identify"]));
    a.extend(sp(11, &["Weaken"]));
    a.extend(sp(12, &["Blindness"]));
    a.extend(sp(13, &["Teleport"]));
    a.extend(sp(14, &["Dispel Magic"]));
    a.extend(sp(15, &["Gravesight"]));
    a.extend(sp(16, &["Corpse Thrall"]));
    a.extend(sp(17, &["Faerie Fog", "Poison"]));
    a.extend(sp(18, &["Cancellation", "Curse"]));
    a.extend(sp(19, &["Energy Drain"]));
    a.extend(sp(20, &["Lesser Golem", "Nether Protection"]));
    a.extend(sp(23, &["Crimson Scourge"]));
    a.extend(sp(24, &["Wraithform"]));
    a.extend(sp(25, &["Animate Dead"]));
    a.extend(sp(26, &["Disruption"]));
    a.extend(sp(27, &["Summon", "Graft"]));
    a.extend(sp(28, &["Vampiric Touch"]));
    a.extend(sp(29, &["Wall of Putrid Flesh"]));
    a.extend(sp(30, &["Control Undead"]));
    a.extend(sp(32, &["Word of Recall"]));
    a.extend(sp(33, &["Greater Undead"]));
    a.extend(sp(35, &["Forget"]));
    a.extend(sp(36, &["Invis to Undead"]));
    a.extend(sp(39, &["Power Word Decay"]));
    a.extend(sp(40, &["Power Word Kill"]));
    a.extend(sp(41, &["Manifest Shadow"]));
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
