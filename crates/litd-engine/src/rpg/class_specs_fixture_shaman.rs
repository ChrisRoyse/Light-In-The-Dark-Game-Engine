// Factual mechanics transcribed from wiki.carrionfields.net/en/class/shaman (issue #1029
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityClause, ClassEligibilityPolicy, ClassFount,
    ClassLeveledAbility, ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract,
    PrimeStat, RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const SHAMAN_CLASS_ID: &str = "class.scourge_priest";

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

// CF race restriction (verbatim, #1029): "All races except for Elves, Cloud Giants, Felar,
// Gnomes, Svirfnebli, Wood-elves, Orcs, and Goblins may become Shamans."
pub(crate) fn shaman_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: SHAMAN_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Scourge-Priest".to_string(),
        player_facing_canon_name: "Scourge-Priest".to_string(),
        source_issue: 1029,
        canon_issues: vec![986, 1029],
        mechanics_source_url: "https://wiki.carrionfields.net/en/class/shaman".to_string(),
        source_history_label: "CF Shaman factual mechanics source only".to_string(),
        concept_summary: "The offensive arm of a faith: priests who seek and destroy the \
             enemies of their god rather than heal the faithful, granted a deity-assigned path \
             of combat supplications upon empowerment."
            .to_string(),
        prime_stat: PrimeStat::Wisdom,
        stat_minimums: AttributeScores::new(0, 0, 13, 0, 0, 0),
        alignment: RaceAccessRule::Restricted {
            values: vec!["good".to_string(), "evil".to_string()],
        },
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Restricted {
                values: vec!["good".to_string(), "evil".to_string()],
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
                "race.ashkin_deep",
                "race.dwarf",
                "race.onyx_kin",
                "race.minotaur",
                "race.storm_giant",
                "race.fire_giant",
                "race.frost_giant",
                "race.sylphkin",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["axe", "flail", "polearm", "sword"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.metal".to_string(), "armor.light".to_string()],
        leveled_abilities: shaman_abilities(),
        specializations: shaman_paths(),
        specialization_choice_levels: vec![21, 40],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.shaman.scourge".to_string(),
            signature_vfx: vec![
                "vfx.shaman.rot_decay_burst".to_string(),
                "vfx.shaman.frenzy_aura".to_string(),
                "vfx.shaman.aristaeia_radiance".to_string(),
                "vfx.shaman.wither_drain".to_string(),
            ],
            palette_notes: "Path-tinted palette (each of the 15 paths gets an accent hue over a \
                 shared ash-and-bone base); no copied class iconography."
                .to_string(),
            animation_notes: "Beseech-and-commune cast stance shared across all 15 paths; \
                 path-specific VFX color/particle overrides per commune."
                .to_string(),
        },
        implementation_notes: vec![
            "Shaman Paths are deity-assigned, not freely chosen ('one path is often assigned \
             upon empowerment and a second path may be given later'); \
             specialization_choice_levels=[21, 40] approximates first/second assignment timing \
             from the earliest and a representative later path-ability level, since CF does not \
             state an exact second-assignment level. This is an approximation, not a literal CF \
             value."
                .to_string(),
            "Several path-alignment/ethos gates ('only chaotic shamans', 'only non-chaotic good \
             shamans', etc.) are captured in each specialization's description text only; \
             ClassSpecialization has no structured alignment/ethos gate field. Filed as a \
             framework gap."
                .to_string(),
            "The deity-to-path assignment table (CF help id=3512) was not retrieved on the \
             capture pass; not modeled here, not fabricated."
                .to_string(),
            "Martial Faith (War, L21), Augmented Edict (Proclamation, L35), Aspect of War (War, \
             L40), and Pastor of Pestilence (Pestilence, L47) are each listed once, as \
             path-tagged Supplication rows, since CF's own skills table and paths table both \
             name them as the same ability."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (WIS), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn shaman_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(1, &["Axe", "Flail", "Polearm", "Sword", "Recall"]));
    a.extend(g(2, &["Beseech"]));
    a.extend(g(6, &["Fast Healing"]));
    a.extend(g(7, &["Kick"]));
    a.extend(g(9, &["Meditation"]));
    a.extend(g(10, &["Hand to Hand", "Shield Block", "Pen"]));
    a.extend(g(14, &["Parry", "Aim", "Metal Armor Use"]));
    a.extend(g(16, &["Charge"]));
    a.extend(g(17, &["Lash", "Warcry", "Light Armor Use"]));
    a.extend(g(18, &["Haggle"]));
    a.extend(g(19, &["Shield Cleave"]));
    a.extend(g(20, &["Second Attack", "Trance"]));
    a.extend(g(22, &["Slice"]));
    a.extend(g(25, &["Enhanced Damage"]));

    a.extend(sup(1, &["Afflict"]));
    a.extend(sup(2, &["Illuminate"]));
    a.extend(sup(3, &["Detect Consecration", "Mend Wounds"]));
    a.extend(sup(4, &["Detect Evil", "Detect Good"]));
    a.extend(sup(5, &["Blindness", "Refresh"]));
    a.extend(sup(6, &["Detect Invis", "Protection"]));
    a.extend(sup(7, &["Know Alignment"]));
    a.extend(sup(8, &["Poison"]));
    a.extend(sup(9, &["Infravision", "Weaken"]));
    a.extend(sup(11, &["Summon"]));
    a.extend(sup(
        12,
        &[
            "Cure Poison",
            "Dispel",
            "Dispel Evil",
            "Dispel Good",
            "Famish",
        ],
    ));
    a.extend(sup(13, &["Haven"]));
    a.extend(sup(15, &["Cure Blindness", "Curse", "Plague"]));
    a.extend(sup(16, &["Protective Shield"]));
    a.extend(sup(17, &["Unveiling"]));
    a.extend(sup(18, &["Cure Disease"]));
    a.extend(sup(19, &["Energy Drain", "Identify"]));
    a.extend(sup(20, &["Frenzy", "Damnation"]));
    a.extend(sup(21, &["Fatigue"]));
    a.extend(sup(22, &["Turn Undead"]));
    a.extend(sup(23, &["Fervor"]));
    a.extend(sup(24, &["Sanctuary"]));
    a.extend(sup(25, &["Cancellation"]));
    a.extend(sup(26, &["Redemption"]));
    a.extend(sup(28, &["Word of Recall"]));
    a.extend(sup(29, &["Holy Word"]));
    a.extend(sup(34, &["Demonfire"]));
    a.extend(sup(38, &["Aristaeia"]));
    a.extend(sup(40, &["Wither"]));
    a.extend(sup(42, &["Rot"]));

    a.extend(ps(22, "spec.shaman.anarchy", &["Sicken"]));
    a.extend(ps(27, "spec.shaman.anarchy", &["Chromatic Fire"]));
    a.extend(ps(33, "spec.shaman.anarchy", &["Disorient"]));
    a.extend(ps(40, "spec.shaman.anarchy", &["Ethos Protection"]));
    a.extend(ps(45, "spec.shaman.anarchy", &["Havoc"]));

    a.extend(ps(21, "spec.shaman.archons_choir", &["Charity of Archons"]));
    a.extend(ps(25, "spec.shaman.archons_choir", &["Holy Justice"]));
    a.extend(ps(33, "spec.shaman.archons_choir", &["Emblaze"]));
    a.extend(ps(37, "spec.shaman.archons_choir", &["Sermon of Censure"]));
    a.extend(ps(43, "spec.shaman.archons_choir", &["Majesty"]));

    a.extend(ps(22, "spec.shaman.breaker", &["Questioning Thoughts"]));
    a.extend(ps(29, "spec.shaman.breaker", &["Fearful Mind"]));
    a.extend(ps(32, "spec.shaman.breaker", &["Devalue Life"]));
    a.extend(ps(39, "spec.shaman.breaker", &["Harrowing Visage"]));
    a.extend(ps(44, "spec.shaman.breaker", &["Forbidden Thoughts"]));

    a.extend(ps(21, "spec.shaman.demonspawn", &["Challegha Aspect"]));
    a.extend(ps(23, "spec.shaman.demonspawn", &["Mors Gravis Aspect"]));
    a.extend(ps(27, "spec.shaman.demonspawn", &["Demonic Summons"]));
    a.extend(ps(31, "spec.shaman.demonspawn", &["Harmentia Aspect"]));
    a.extend(ps(38, "spec.shaman.demonspawn", &["Abyssal Flame"]));
    a.extend(ps(39, "spec.shaman.demonspawn", &["Morosa Aspect"]));
    a.extend(ps(45, "spec.shaman.demonspawn", &["Acid Spray"]));

    a.extend(ps(21, "spec.shaman.desolation", &["Lava Fountain"]));
    a.extend(ps(30, "spec.shaman.desolation", &["Earthrend"]));
    a.extend(ps(33, "spec.shaman.desolation", &["Locust Plague"]));
    a.extend(ps(36, "spec.shaman.desolation", &["Storm Cloak"]));
    a.extend(ps(45, "spec.shaman.desolation", &["Typhoon"]));

    a.extend(ps(
        21,
        "spec.shaman.divine_anger",
        &["Shield of Retributive Rage"],
    ));
    a.extend(ps(
        27,
        "spec.shaman.divine_anger",
        &["Amplified Affliction"],
    ));
    a.extend(ps(
        32,
        "spec.shaman.divine_anger",
        &["Instrument of Judgment"],
    ));
    a.extend(ps(36, "spec.shaman.divine_anger", &["All Seeing Eye"]));
    a.extend(ps(47, "spec.shaman.divine_anger", &["Unerring Smite"]));

    a.extend(ps(21, "spec.shaman.hells_pact", &["Covetousness"]));
    a.extend(ps(27, "spec.shaman.hells_pact", &["Heresy"]));
    a.extend(ps(33, "spec.shaman.hells_pact", &["Gluttony"]));
    a.extend(ps(34, "spec.shaman.hells_pact", &["Hellfire"]));
    a.extend(ps(40, "spec.shaman.hells_pact", &["Betrayal"]));
    a.extend(ps(45, "spec.shaman.hells_pact", &["Aspect of the Devil"]));

    a.extend(ps(23, "spec.shaman.lifes_champion", &["Lifebind"]));
    a.extend(ps(31, "spec.shaman.lifes_champion", &["Searing Ward"]));
    a.extend(ps(32, "spec.shaman.lifes_champion", &["Soul Citadel"]));
    a.extend(ps(37, "spec.shaman.lifes_champion", &["Vivimantic Aura"]));
    a.extend(ps(47, "spec.shaman.lifes_champion", &["Luminous Assault"]));

    a.extend(ps(21, "spec.shaman.pain", &["Dolorous Coil"]));
    a.extend(ps(28, "spec.shaman.pain", &["Anguish"]));
    a.extend(ps(33, "spec.shaman.pain", &["Spasms"]));
    a.extend(ps(38, "spec.shaman.pain", &["Embrace Pain", "Reject Pain"]));
    a.extend(ps(43, "spec.shaman.pain", &["Wrack"]));

    a.extend(ps(21, "spec.shaman.pestilence", &["Decaying Touch"]));
    a.extend(ps(25, "spec.shaman.pestilence", &["Pestilence"]));
    a.extend(ps(30, "spec.shaman.pestilence", &["Corrode"]));
    a.extend(ps(40, "spec.shaman.pestilence", &["Effluvium"]));
    a.extend(ps(47, "spec.shaman.pestilence", &["Pastor of Pestilence"]));

    a.extend(ps(21, "spec.shaman.proclamation", &["Unity"]));
    a.extend(ps(26, "spec.shaman.proclamation", &["Proclamation"]));
    a.extend(ps(29, "spec.shaman.proclamation", &["Equalize"]));
    a.extend(ps(35, "spec.shaman.proclamation", &["Augmented Edict"]));
    a.extend(ps(39, "spec.shaman.proclamation", &["Solitude"]));
    a.extend(ps(44, "spec.shaman.proclamation", &["Vociferation"]));

    a.extend(ps(21, "spec.shaman.revelation", &["Sanctimonious Sight"]));
    a.extend(ps(30, "spec.shaman.revelation", &["Dire Omens"]));
    a.extend(ps(33, "spec.shaman.revelation", &["Oracle"]));
    a.extend(ps(39, "spec.shaman.revelation", &["Miraculous Foresight"]));
    a.extend(ps(48, "spec.shaman.revelation", &["Seal Fate"]));

    a.extend(ps(
        21,
        "spec.shaman.seraphs_wing",
        &["Soldier of the Light"],
    ));
    a.extend(ps(28, "spec.shaman.seraphs_wing", &["Shadowbane"]));
    a.extend(ps(34, "spec.shaman.seraphs_wing", &["Celestial Choir"]));
    a.extend(ps(37, "spec.shaman.seraphs_wing", &["Aspect of the Angel"]));
    a.extend(ps(45, "spec.shaman.seraphs_wing", &["Elysian Shower"]));

    a.extend(ps(21, "spec.shaman.vengeance", &["Tooth for a Tooth"]));
    a.extend(ps(27, "spec.shaman.vengeance", &["Bulwark of Faith"]));
    a.extend(ps(32, "spec.shaman.vengeance", &["Rally the Fallen"]));
    a.extend(ps(40, "spec.shaman.vengeance", &["Ethos Protection"]));
    a.extend(ps(45, "spec.shaman.vengeance", &["Eye for an Eye"]));

    a.extend(ps(21, "spec.shaman.war", &["Martial Faith"]));
    a.extend(ps(27, "spec.shaman.war", &["Spectral Weapon"]));
    a.extend(ps(30, "spec.shaman.war", &["Courage"]));
    a.extend(ps(35, "spec.shaman.war", &["Razor Chain"]));
    a.extend(ps(40, "spec.shaman.war", &["Aspect of War"]));
    a
}

fn shaman_paths() -> Vec<ClassSpecialization> {
    [
        (
            "spec.shaman.anarchy",
            "Anarchy",
            "Only chaotic shamans can walk this path.",
        ),
        (
            "spec.shaman.archons_choir",
            "Archon's Choir",
            "Only non-chaotic good shamans.",
        ),
        (
            "spec.shaman.breaker",
            "Breaker",
            "Mind-breaking affliction path.",
        ),
        (
            "spec.shaman.demonspawn",
            "Demonspawn",
            "Only non-orderly evil shamans.",
        ),
        (
            "spec.shaman.desolation",
            "Desolation",
            "Elemental-devastation path.",
        ),
        (
            "spec.shaman.divine_anger",
            "Divine Anger",
            "Retributive-judgment path.",
        ),
        (
            "spec.shaman.hells_pact",
            "Hell's Pact",
            "Only non-chaotic evil shamans.",
        ),
        (
            "spec.shaman.lifes_champion",
            "Life's Champion",
            "Only good shamans.",
        ),
        (
            "spec.shaman.pain",
            "Pain",
            "Pain-affliction and endurance path.",
        ),
        (
            "spec.shaman.pestilence",
            "Pestilence",
            "Disease and decay path.",
        ),
        (
            "spec.shaman.proclamation",
            "Proclamation",
            "Ethos-enforcement path.",
        ),
        (
            "spec.shaman.revelation",
            "Revelation",
            "Foresight and truth-seeing path.",
        ),
        (
            "spec.shaman.seraphs_wing",
            "Seraph's Wing",
            "Only non-orderly good shamans.",
        ),
        (
            "spec.shaman.vengeance",
            "Vengeance",
            "Only orderly shamans.",
        ),
        ("spec.shaman.war", "War", "Martial-favor path."),
    ]
    .into_iter()
    .map(|(id, name, desc)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: 21,
        description: desc.to_string(),
        eligibility: shaman_path_policy(id),
    })
    .collect()
}

fn shaman_path_policy(id: &str) -> Option<ClassEligibilityPolicy> {
    let clause =
        |alignments: Option<Vec<&str>>, ethoses: Option<Vec<&str>>| ClassEligibilityClause {
            match_all: false,
            race_ids: None,
            alignments: alignments.map(|values| values.into_iter().map(str::to_string).collect()),
            ethoses: ethoses.map(|values| values.into_iter().map(str::to_string).collect()),
        };
    let policy = match id {
        "spec.shaman.demonspawn" => ClassEligibilityPolicy {
            clauses: vec![clause(Some(vec!["evil"]), Some(vec!["neutral", "chaotic"]))],
        },
        "spec.shaman.hells_pact" => ClassEligibilityPolicy {
            clauses: vec![clause(Some(vec!["evil"]), Some(vec!["neutral", "orderly"]))],
        },
        "spec.shaman.lifes_champion" => ClassEligibilityPolicy {
            clauses: vec![clause(Some(vec!["good"]), None)],
        },
        "spec.shaman.seraphs_wing" => ClassEligibilityPolicy {
            clauses: vec![clause(Some(vec!["good"]), Some(vec!["neutral", "chaotic"]))],
        },
        "spec.shaman.vengeance" => ClassEligibilityPolicy {
            clauses: vec![clause(None, Some(vec!["orderly"]))],
        },
        _ => return None,
    };
    Some(policy)
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

fn ps(level: u16, spec_id: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: format!("{spec_id}.{}", slug(name)),
            display_name: name.to_string(),
            kind: AbilityKind::Supplication,
            specialization_ids: vec![spec_id.to_string()],
            acquisition: None,
        })
        .collect()
}
