// Factual mechanics transcribed from wiki.carrionfields.net/class/conjurer (issue #1034
// FACTUAL SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const CONJURER_CLASS_ID: &str = "class.circlebinder";

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

// CF race restriction (verbatim, #1034): "All races may become Conjurers except for Giants,
// Dwarves, Duergar, Orcs, Felar, Goblins, Wood-Elves, Azure-Touched, Minotaurs, Saurians, and
// Centaurs." Azure-touched/Centaur have no authored race analog in this roster.
pub(crate) fn conjurer_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: CONJURER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Circlebinder".to_string(),
        player_facing_canon_name: "Circlebinder".to_string(),
        source_issue: 1034,
        canon_issues: vec![986, 1034],
        mechanics_source_url: "https://wiki.carrionfields.net/class/conjurer".to_string(),
        source_history_label: "CF Conjurer factual mechanics source only".to_string(),
        concept_summary: "Mages who bind extra-planar beings into service for information, \
             protection, transport, or offensive strength, chalking circles of protection \
             against the chance a binding fails; every service hides a cost."
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
            reason: "Whether the Circlebinder draws from the Invocation fount (mortal Weave) or \
                     the Pact fount (Hollow bargain) is an unresolved operator/lore decision \
                     flagged by the #1034 capture."
                .to_string(),
        },
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.gnome",
                "race.deep_kin",
                "race.sylphkin",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["axe", "dagger", "staff", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.mystical".to_string()],
        leveled_abilities: conjurer_abilities(),
        specializations: conjurer_familiar_tiers(),
        specialization_choice_levels: vec![17],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.conjurer.circlebind".to_string(),
            signature_vfx: vec![
                "vfx.conjurer.summon_circle_glyph".to_string(),
                "vfx.conjurer.planar_ward_shimmer".to_string(),
                "vfx.conjurer.bind_elemental_chains".to_string(),
                "vfx.conjurer.phase_door_rift".to_string(),
            ],
            palette_notes: "Circle-gold glyphwork over deep indigo; no copied class iconography."
                .to_string(),
            animation_notes: "Circle-drawing cast stance for binding/protection spells; \
                 conjured-ally VFX vary by familiar tier (celestial/neutral/infernal)."
                .to_string(),
        },
        implementation_notes: vec![
            "Azure-touched and Centaur are named in the CF race text but have no authored \
             RaceSpec in this roster yet; they are simply absent from the allow accounting here."
                .to_string(),
            "OPEN LORE QUESTION (not resolved here, per #1034's own flag): whether the \
             Circlebinder draws from the Invocation fount (mortal Weave) or the Pact fount \
             (Hollow bargain) is unresolved; ClassSpec has no fount-classification field. Filed \
             as a framework gap pending operator/lore decision."
                .to_string(),
            "The three Key-Concept familiar alignment tiers (Good/Neutral/Evil Familiars) are \
             captured as ClassSpecialization rows at the 'familiar' spell's unlock level (17); \
             CF's spell table does not tag individual conjure/bind spells to a specific tier, so \
             no leveled_abilities reference them by specialization_id."
                .to_string(),
            "'Kaubris Anchor' (L37) is transcribed verbatim including its capitalization as \
             printed on the CF spells table."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (CHA), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn conjurer_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Axe",
            "Dagger",
            "Staff",
            "Whip",
            "Scrolls",
            "Talismans",
            "Wands",
            "Recall",
        ],
    ));
    a.extend(g(7, &["Meditation"]));
    a.extend(g(9, &["Haggle"]));
    a.extend(g(10, &["Pen"]));
    a.extend(g(12, &["Parry"]));
    a.extend(g(14, &["Trance", "Hand to Hand"]));
    a.extend(g(15, &["Fast Healing", "Shield Block"]));
    a.extend(g(16, &["Lash"]));
    a.extend(g(21, &["Second Attack"]));
    a.extend(g(24, &["Mystical Armor Use"]));

    a.extend(sp(1, &["Detect Magic", "Magic Missile"]));
    a.extend(sp(2, &["Detect Charm", "Sighttheft"]));
    a.extend(sp(3, &["Detect Invis"]));
    a.extend(sp(4, &["Invis"]));
    a.extend(sp(5, &["Armor"]));
    a.extend(sp(6, &["Faerie Fire"]));
    a.extend(sp(9, &["Infravision"]));
    a.extend(sp(10, &["Identify"]));
    a.extend(sp(11, &["Warp Dimension"]));
    a.extend(sp(13, &["Teleport"]));
    a.extend(sp(14, &["Dispel Magic"]));
    a.extend(sp(15, &["Locate Object"]));
    a.extend(sp(16, &["Flash"]));
    a.extend(sp(17, &["Familiar"]));
    a.extend(sp(
        18,
        &["Cancellation", "Resist Negative", "Resist Positive"],
    ));
    a.extend(sp(19, &["Conjure Smoke Mephit"]));
    a.extend(sp(
        20,
        &["Pass Door", "Conjure Elemental", "Bind Elemental"],
    ));
    a.extend(sp(21, &["Deny Summoning"]));
    a.extend(sp(22, &["Celestial Circle"]));
    a.extend(sp(23, &["Conjure Will-o-Wisp", "Dismissal"]));
    a.extend(sp(
        25,
        &[
            "Planar Ward",
            "Conjure Angel",
            "Conjure Demon",
            "Lightbind",
            "Darkbind",
        ],
    ));
    a.extend(sp(26, &["Sending"]));
    a.extend(sp(27, &["Thaumaturgic Circle"]));
    a.extend(sp(28, &["Phase Door"]));
    a.extend(sp(30, &["Projection"]));
    a.extend(sp(34, &["Word of Recall"]));
    a.extend(sp(35, &["Conjure Archon", "Conjure Devil"]));
    a.extend(sp(37, &["Kaubris Anchor"]));
    a.extend(sp(40, &["Bind Nightgaunt", "Conjure Nightgaunt"]));
    a.extend(sp(43, &["Clairaudience"]));
    a.extend(sp(44, &["Clairvoyance"]));
    a.extend(sp(45, &["Contact Other Planes"]));
    a.extend(sp(51, &["Tesseract"]));
    a
}

fn conjurer_familiar_tiers() -> Vec<ClassSpecialization> {
    [
        ("spec.conjurer.good_familiars", "Good Familiars"),
        ("spec.conjurer.neutral_familiars", "Neutral Familiars"),
        ("spec.conjurer.evil_familiars", "Evil Familiars"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: 17,
        description: format!(
            "{name}: the alignment tier of extra-planar allies a Circlebinder \
             may bind, gated by the familiar spell."
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
