// Factual mechanics transcribed from wiki.carrionfields.net/class/bard (issue #1024 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule,
};

pub(crate) const BARD_CLASS_ID: &str = "class.wandersong";

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

// CF race restriction (verbatim, #1024): "Humans, Half-elves, Half-drows, Elves, Woodelves,
// Arials, Azure-touched, and Centaurs may be bards." Azure-touched/Centaur have no authored
// race analog in this roster.
pub(crate) fn bard_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: BARD_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Wandersong".to_string(),
        player_facing_canon_name: "Wandersong".to_string(),
        source_issue: 1024,
        canon_issues: vec![986, 1024],
        mechanics_source_url: "https://wiki.carrionfields.net/class/bard".to_string(),
        source_history_label: "CF Bard factual mechanics source only".to_string(),
        concept_summary: "Itinerant musicians whose songs carry mood-affecting, suggestive, and \
             (at high mastery) mind-weaving power; rudimentary fighters who preserve inconvenient \
             memory on the road."
            .to_string(),
        prime_stat: PrimeStat::Charisma,
        stat_minimums: AttributeScores::new(0, 0, 0, 0, 0, 13),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Any,
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Unresolved {
            reason: "Bard song-magic tradition has no canon fount classification in the Ashen \
                     Reach cosmology docs; open operator/lore decision per #1754."
                .to_string(),
        },
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.elf",
                "race.wood_elf",
                "race.sylphkin",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "mace", "sword", "staff"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string()],
        leveled_abilities: bard_abilities(),
        specializations: Vec::new(),
        specialization_choice_levels: Vec::new(),
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.bard.wandersong".to_string(),
            signature_vfx: vec![
                "vfx.bard.song_resonance_waves".to_string(),
                "vfx.bard.instrument_shimmer".to_string(),
                "vfx.bard.illusion_fantasia".to_string(),
                "vfx.bard.crowd_mood_aura".to_string(),
            ],
            palette_notes: "Warm amber and road-dust palette; no copied class iconography."
                .to_string(),
            animation_notes: "Instrument-in-hand performance animations drive song VFX; higher \
                 mastery songs (post level 30) add illusion-layer particle work per CF's 'must \
                 master singing before rehearsing songs above the 30th level' note."
                .to_string(),
        },
        implementation_notes: vec![
            "CF's Repertoire/Rehearse/instrument-choice mechanic ('All songs are affected by a \
             bard's chosen instrument and REPERTOIRE') has no field in ClassSpec; only song name \
             and unlock level are captured. Filed as a framework gap."
                .to_string(),
            "The Bard Edges list (wiki.carrionfields.net/game/edges#bard) was not retrieved on \
             the #1024 capture pass; not included here, not fabricated."
                .to_string(),
            "Per-song and per-skill mechanical effects live behind separate CF HELP links not \
             retrieved on the capture pass; only names and unlock levels are factual here."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (CHA), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn bard_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Dagger",
            "Mace",
            "Sword",
            "Staff",
            "Haggle",
            "Scrolls",
            "Talismans",
            "Recall",
            "Sing",
        ],
    ));
    a.extend(g(2, &["Dirt Kicking"]));
    a.extend(g(3, &["Dodge", "Mimic"]));
    a.extend(g(4, &["Trip"]));
    a.extend(g(5, &["Aim", "Tune"]));
    a.extend(g(6, &["Hand to Hand", "Lore", "Elbow"]));
    a.extend(g(7, &["Kick", "Knee"]));
    a.extend(g(8, &["Flourish", "Light Armor Use"]));
    a.extend(g(9, &["Pick Lock", "Evade"]));
    a.extend(g(10, &["Fast Healing", "Pen"]));
    a.extend(g(11, &["Roundhouse"]));
    a.extend(g(12, &["Meditation", "Panhandle"]));
    a.extend(g(13, &["Parry"]));
    a.extend(g(15, &["Second Attack"]));
    a.extend(g(16, &["Shield Block"]));
    a.extend(g(18, &["Disarm"]));
    a.extend(g(19, &["Pugil"]));
    a.extend(g(25, &["Enhanced Damage"]));
    a.extend(g(30, &["Sneak"]));
    a.extend(g(36, &["Precise Aim"]));

    a.extend(gk(2, AbilityKind::Song, &["Travelers March"]));
    a.extend(gk(4, AbilityKind::Song, &["Charismatic Prelude"]));
    a.extend(gk(7, AbilityKind::Song, &["Adagio"]));
    a.extend(gk(9, AbilityKind::Song, &["Piercing Dissonance"]));
    a.extend(gk(10, AbilityKind::Song, &["Chant of Resilience"]));
    a.extend(gk(11, AbilityKind::Song, &["Battaglia"]));
    a.extend(gk(14, AbilityKind::Song, &["Canticle of the Gods"]));
    a.extend(gk(15, AbilityKind::Song, &["Elven Adagio"]));
    a.extend(gk(16, AbilityKind::Song, &["Lullaby"]));
    a.extend(gk(17, AbilityKind::Song, &["Languid Carol"]));
    a.extend(gk(18, AbilityKind::Song, &["Anthem of Resistance"]));
    a.extend(gk(
        19,
        AbilityKind::Song,
        &["Riddle of Revelation", "Cacophonous Clamor"],
    ));
    a.extend(gk(20, AbilityKind::Song, &["Reveille"]));
    a.extend(gk(21, AbilityKind::Song, &["Pastoral of the Mind"]));
    a.extend(gk(22, AbilityKind::Song, &["Tranquil Serenade"]));
    a.extend(gk(26, AbilityKind::Song, &["Requiem"]));
    a.extend(gk(27, AbilityKind::Song, &["Elegy of Tears"]));
    a.extend(gk(28, AbilityKind::Song, &["Bagatelle of Bravado"]));
    a.extend(gk(29, AbilityKind::Song, &["Laborious Lament"]));
    a.extend(gk(32, AbilityKind::Song, &["Vibrato"]));
    a.extend(gk(33, AbilityKind::Song, &["Nocturne of Fright"]));
    a.extend(gk(34, AbilityKind::Song, &["Apocalyptic Overture"]));
    a.extend(gk(38, AbilityKind::Song, &["Fantasia of Illusion"]));
    a.extend(gk(40, AbilityKind::Song, &["Symphonic Echoes"]));
    a.extend(gk(42, AbilityKind::Song, &["Dirge of Solitude"]));
    a.extend(gk(45, AbilityKind::Song, &["Grand Nocturne"]));
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
