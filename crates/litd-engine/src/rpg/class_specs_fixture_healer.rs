// Factual mechanics transcribed from wiki.carrionfields.net/en/class/healer (issue #1030
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityClause, ClassEligibilityPolicy, ClassFount,
    ClassLeveledAbility, ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule, RaceAlignmentException,
};

pub(crate) const HEALER_CLASS_ID: &str = "class.lifewarden";

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

// CF race restriction (verbatim, #1030): "All races except for Fire Giants, Felar, Duergars,
// Orcs, Minotaurs, Frost Giants, Centaurs, and Goblins may become healers." Centaur has no
// authored race analog in this roster.
pub(crate) fn healer_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: HEALER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Lifewarden".to_string(),
        player_facing_canon_name: "Lifewarden".to_string(),
        source_issue: 1030,
        canon_issues: vec![986, 1030],
        mechanics_source_url: "https://wiki.carrionfields.net/en/class/healer".to_string(),
        source_history_label: "CF Healer factual mechanics source only".to_string(),
        concept_summary: "The most defensive of priests: sworn to shield and mend the faithful, \
             with an unmatched breadth of curative supplications gated behind empowerment."
            .to_string(),
        prime_stat: PrimeStat::Wisdom,
        stat_minimums: AttributeScores::new(0, 0, 13, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["good".to_string(), "neutral".to_string()],
        },
        ethos: RaceAccessRule::Any,
        alignment_exceptions: healer_alignment_exceptions(),
        eligibility: healer_eligibility_policy(),
        fount: ClassFount::Supplication,
        empowerment: true,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.wood_elf",
                "race.dwarf",
                "race.gnome",
                "race.deep_kin",
                "race.storm_giant",
                "race.cloud_giant",
                "race.sylphkin",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["flail", "mace", "staff", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string()],
        leveled_abilities: healer_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.healer.lifewarden".to_string(),
            signature_vfx: vec![
                "vfx.healer.mend_wounds_glow".to_string(),
                "vfx.healer.blade_barrier_wall".to_string(),
                "vfx.healer.mass_healing_pulse".to_string(),
                "vfx.healer.sanctuary_shimmer".to_string(),
            ],
            palette_notes: "Soft ivory and healing-green palette; no copied class iconography."
                .to_string(),
            animation_notes: "Commune-and-lay-on-hands cast stances; group supplications (mass \
                 healing, group teleport) emit a shared radial pulse VFX."
                .to_string(),
        },
        implementation_notes: vec![
            "Alignment (schema v2, gap 2, #1718): the description's narrower rule - 'Healers \
             must be good or neutral, with the exception of humans, dark-elves, and saurians - \
             whose society's or values are warped enough to produce them' - is now ENCODED, \
             superseding the General-Info box's looser 'Any'. The class-level gate is \
             good-or-neutral and `alignment_exceptions` lifts it for those three races (see \
             healer_alignment_exceptions)."
                .to_string(),
            "Alignment-gated supplication variant pairs (e.g. divine bond/dark pact at L42, \
             sanctify room/sanctify self at L35, turn undead/vitality good-align-only at L28/12) \
             are captured as plain leveled abilities; ClassLeveledAbility has no per-ability \
             alignment gate field. Filed as a framework gap."
                .to_string(),
            "No path/subclass/DEDICATE tree exists on the CF Healer class page (the Healer Edge \
             List, id=1964, was not retrieved on this pass); specializations/legacies are \
             intentionally empty."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (WIS), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

// CF Healer alignment (verbatim, #1030): "Healers must be good or neutral, with
// the exception of humans, dark-elves, and saurians - whose society's or values
// are warped enough to produce them." The class-level `alignment` gate is
// good-or-neutral; these three races carry a per-race carve-out (schema v2, gap
// 2) that lifts the restriction entirely, letting them also be evil. Race ids
// map to the authored roster: human -> race.ashen_human, dark-elf (CF
// wiki/race/dark-elf) -> race.ashkin_deep, saurian -> race.saur.
fn healer_alignment_exceptions() -> Vec<RaceAlignmentException> {
    ["race.ashen_human", "race.ashkin_deep", "race.saur"]
        .into_iter()
        .map(|race_id| RaceAlignmentException {
            race_id: race_id.to_string(),
            allowed: RaceAccessRule::Any,
        })
        .collect()
}

fn healer_eligibility_policy() -> ClassEligibilityPolicy {
    ClassEligibilityPolicy {
        clauses: vec![
            ClassEligibilityClause {
                match_all: false,
                race_ids: None,
                alignments: Some(vec!["good".to_string(), "neutral".to_string()]),
                ethoses: None,
            },
            ClassEligibilityClause {
                match_all: false,
                race_ids: Some(
                    ["race.ashen_human", "race.ashkin_deep", "race.saur"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                ),
                alignments: Some(vec!["evil".to_string()]),
                ethoses: None,
            },
        ],
    }
}

fn healer_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &["Flail", "Mace", "Staff", "Whip", "Recall", "First Aid"],
    ));
    a.extend(g(6, &["Meditation"]));
    a.extend(g(7, &["Shield Block"]));
    a.extend(g(10, &["Fast Healing", "Pen"]));
    a.extend(g(12, &["Assess"]));
    a.extend(g(13, &["Hand to Hand"]));
    a.extend(g(15, &["Kick"]));
    a.extend(g(16, &["Trance", "Parry"]));
    a.extend(g(17, &["Haggle", "Nap"]));
    a.extend(g(18, &["Pugil"]));
    a.extend(g(19, &["Lash"]));
    a.extend(g(23, &["Light Armor Use"]));
    a.extend(g(24, &["Second Attack"]));

    a.extend(sup(1, &["Mend Wounds"]));
    a.extend(sup(2, &["Detect Consecration", "Soothe"]));
    a.extend(sup(3, &["Create Food", "Illuminate"]));
    a.extend(sup(4, &["Sate"]));
    a.extend(sup(5, &["Detect Evil", "Detect Good", "Rouse"]));
    a.extend(sup(6, &["Create Water", "Refresh"]));
    a.extend(sup(7, &["Bless", "Cure Blindness"]));
    a.extend(sup(8, &["Detect Invis", "Know Alignment", "Protection"]));
    a.extend(sup(9, &["Quench"]));
    a.extend(sup(10, &["Cure Poison", "Infravision", "Aegis"]));
    a.extend(sup(11, &["Cure Disease", "Gate", "Cure Deafness", "Haven"]));
    a.extend(sup(12, &["Remove Curse", "Vitality"]));
    a.extend(sup(13, &["Protective Shield", "Identify"]));
    a.extend(sup(14, &["Curse"]));
    a.extend(sup(15, &["Invigorate", "Ardent Faith", "Stalwart Arm"]));
    a.extend(sup(16, &["Calm"]));
    a.extend(sup(17, &["Create Spring", "Heal"]));
    a.extend(sup(18, &["Unveiling"]));
    a.extend(sup(19, &["Teleport", "Restoration", "Guiding Hand"]));
    a.extend(sup(20, &["Awaken", "Antipathy"]));
    a.extend(sup(21, &["Sanctuary"]));
    a.extend(sup(22, &["Spirit Shepherd"]));
    a.extend(sup(23, &["Prayer"]));
    a.extend(sup(24, &["Cancellation"]));
    a.extend(sup(25, &["Cleanse"]));
    a.extend(sup(26, &["Pass Door"]));
    a.extend(sup(27, &["Word of Recall"]));
    a.extend(sup(28, &["Turn Undead"]));
    a.extend(sup(29, &["Frenzy", "Spiritual Salve"]));
    a.extend(sup(30, &["Swiftness"]));
    a.extend(sup(31, &["Healing Sleep"]));
    a.extend(sup(32, &["Remove Paralysis"]));
    a.extend(sup(33, &["Group Teleport"]));
    a.extend(sup(34, &["Purity of Blood"]));
    a.extend(sup(35, &["Mass Healing", "Sanctify Self", "Sanctify Room"]));
    a.extend(sup(36, &["Purity of Flesh"]));
    a.extend(sup(37, &["Blade Barrier"]));
    a.extend(sup(38, &["Purity of Mind"]));
    a.extend(sup(42, &["Divine Bond", "Dark Pact"]));
    a.extend(sup(43, &["Rejuvenate"]));
    a.extend(sup(45, &["Lifeshield"]));
    a.extend(sup(47, &["Bulwark of Faith"]));
    a.extend(sup(48, &["Soul Citadel"]));
    a.extend(sup(51, &["Miraculous Foresight"]));
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
