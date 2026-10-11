// Factual mechanics transcribed from wiki.carrionfields.net/class/invoker (issue #1032 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const INVOKER_CLASS_ID: &str = "class.currentbraider";

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

// CF race restriction (verbatim, #1032): "All races may become invokers except for the Giant
// races, Duergar, Dwarves, Orcs, Felar, Goblins, Azure-Touched, Minotaurs, and Centaurs."
// Azure-touched/Centaur have no authored race analog in this roster.
pub(crate) fn invoker_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: INVOKER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Currentbraider".to_string(),
        player_facing_canon_name: "Currentbraider".to_string(),
        source_issue: 1032,
        canon_issues: vec![986, 1032],
        mechanics_source_url: "https://wiki.carrionfields.net/class/invoker".to_string(),
        source_history_label: "CF Invoker factual mechanics source only".to_string(),
        concept_summary: "Mortal Weave-reachers who invoke elemental energy directly rather than \
             alter existing things: masters of seven elemental currents, each with a base spell \
             that a Currentbraider can later master into its 'perfect' form."
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
                "race.ashen_human",
                "race.elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.ashkin_deep",
                "race.wood_elf",
                "race.gnome",
                "race.deep_kin",
                "race.sylphkin",
                "race.saur",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["dagger", "mace", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.mystical".to_string()],
        leveled_abilities: invoker_abilities(),
        specializations: invoker_paths(),
        specialization_choice_levels: vec![1],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.invoker.current_weave".to_string(),
            signature_vfx: vec![
                "vfx.invoker.fireball_burst".to_string(),
                "vfx.invoker.lightning_bolt_arc".to_string(),
                "vfx.invoker.elemental_shield_shell".to_string(),
                "vfx.invoker.avalanche_debris".to_string(),
            ],
            palette_notes: "Seven current-tinted accent hues (ember/stone/sky/tide/rime/mire/\
                 storm) over a shared arcane-blue base; no copied class iconography."
                .to_string(),
            animation_notes: "Shared spellcraft cast stance; 'perfect' spells replace their base \
                 spell's VFX with a larger, current-colored escalation."
                .to_string(),
        },
        implementation_notes: vec![
            "Azure-touched and Centaur are named in the CF race text but have no authored \
             RaceSpec in this roster yet; they are simply absent from the allow accounting here."
                .to_string(),
            "The base-to-'perfect' spell mastery chain is captured by grouping each path's \
             spells under one ClassSpecialization per element (fire/earth/air/water/frost/ooze/\
             lightning); CF frames this as automatic per-spell mastery, not a discrete choice, \
             so specialization_choice_levels=[1] reflects 'available from creation', not a real \
             gate."
                .to_string(),
            "Two entries are dual-element in CF (noxious cloud: perfect wall of fire AND buffet; \
             incendiary cloud: perfect fireshield AND buffet); ClassLeveledAbility supports only \
             one specialization_id, so both were assigned to their first-listed (fire) path only."
                .to_string(),
            "Per-spell effect magnitudes live behind separate CF helpsearch.php links not \
             retrieved on the capture pass; only names, levels, and perfect-chain relationships \
             are factual here."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (INT), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn invoker_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Dagger",
            "Mace",
            "Whip",
            "Hand to Hand",
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
    a.extend(g(13, &["Trance", "Parry"]));
    a.extend(g(15, &["Fast Healing"]));
    a.extend(g(18, &["Mystical Armor Use"]));
    a.extend(g(20, &["Lash"]));
    a.extend(g(23, &["Second Attack"]));

    a.extend(el(1, "fire", &["Channel Heat"]));
    a.extend(el(1, "water", &["Create Water"]));
    a.extend(el(1, "air", &["Cyclone"]));
    a.extend(el(1, "earth", &["Earthquake"]));
    a.extend(el(1, "frost", &["Frost Fingers"]));
    a.extend(el(1, "lightning", &["Shocking Touch"]));
    a.extend(el(1, "ooze", &["Acidic Secretion"]));
    a.extend(sp(2, &["Detect Magic"]));
    a.extend(sp(3, &["Detect Invis"]));
    a.extend(sp(4, &["Invis"]));
    a.extend(el(5, "earth", &["Sheen of Stone"]));
    a.extend(sp(6, &["Flare"]));
    a.extend(sp(9, &["Infravision"]));
    a.extend(sp(10, &["Identify"]));
    a.extend(el(11, "air", &["Whispering Wind"]));
    a.extend(el(11, "water", &["Create Spring"]));
    a.extend(el(12, "ooze", &["Grease"]));
    a.extend(el(12, "fire", &["Wall of Fire"]));
    a.extend(sp(13, &["Teleport"]));
    a.extend(el(13, "frost", &["Icicle"]));
    a.extend(el(13, "lightning", &["Charge Weapon"]));
    a.extend(sp(14, &["Faerie Fog"]));
    a.extend(el(14, "earth", &["Stoneshatter"]));
    a.extend(sp(15, &["Elemental Attunement"]));
    a.extend(el(16, "air", &["Wind Wall"]));
    a.extend(sp(18, &["Cancellation"]));
    a.extend(sp(19, &["Dispel Magic"]));
    a.extend(el(19, "ooze", &["Siltscreen"]));
    a.extend(el(21, "fire", &["Fireball"]));
    a.extend(el(21, "lightning", &["Lightning Bolt"]));
    a.extend(el(22, "earth", &["Avalanche"]));
    a.extend(el(22, "frost", &["Iceshards"]));
    a.extend(el(23, "ooze", &["Adhesive Web"]));
    a.extend(sp(24, &["Pass Door"]));
    a.extend(sp(25, &["Weave of Elements", "Word of Recall"]));
    a.extend(el(26, "water", &["Drown"]));
    a.extend(el(27, "air", &["Buffet"]));
    a.extend(el(27, "earth", &["Earth Ripple"]));
    a.extend(el(28, "water", &["Tsunami"]));
    a.extend(el(29, "ooze", &["Gel"]));
    a.extend(el(30, "fire", &["Controlled Fireball"]));
    a.extend(el(30, "lightning", &["Forked Lightning"]));
    a.extend(el(32, "earth", &["Pebble to Boulder"]));
    a.extend(el(32, "frost", &["Cone of Cold"]));
    a.extend(el(34, "fire", &["Noxious Cloud"]));
    a.extend(el(35, "fire", &["Nova"]));
    a.extend(el(36, "ooze", &["Vitriolic Stream"]));
    a.extend(el(37, "frost", &["Iceneedles"]));
    a.extend(el(38, "water", &["Watershield"]));
    a.extend(el(38, "air", &["Airshield"]));
    a.extend(el(38, "fire", &["Fireshield"]));
    a.extend(el(38, "lightning", &["Lightningshield"]));
    a.extend(el(38, "earth", &["Earthshield"]));
    a.extend(el(38, "frost", &["Frostshield"]));
    a.extend(el(38, "ooze", &["Oozeshield"]));
    a.extend(el(39, "frost", &["Iceslick"]));
    a.extend(el(41, "earth", &["Dig"]));
    a.extend(el(41, "air", &["Vortex"]));
    a.extend(el(43, "water", &["Engulf"]));
    a.extend(el(43, "frost", &["Conglaciation"]));
    a.extend(el(44, "lightning", &["Pillar of the Heavens"]));
    a.extend(el(44, "air", &["Improved Invis"]));
    a.extend(el(45, "fire", &["Immolation"]));
    a.extend(el(45, "water", &["Geyser"]));
    a.extend(el(46, "ooze", &["Quicksand"]));
    a.extend(el(47, "fire", &["Incendiary Cloud"]));
    a.extend(el(48, "earth", &["Rain of Stone"]));
    a
}

fn invoker_paths() -> Vec<ClassSpecialization> {
    [
        ("fire", "Fire Current"),
        ("earth", "Stone Current"),
        ("air", "Air Current"),
        ("water", "Tide Current"),
        ("frost", "Frost Current"),
        ("ooze", "Mire Current"),
        ("lightning", "Storm Current"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: format!("spec.invoker.{id}"),
        display_name: name.to_string(),
        unlock_level: 1,
        description: format!(
            "{name}: base spell available from level 1, masterable into its 'perfect' form."
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

fn sp(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: slug(name),
            display_name: name.to_string(),
            kind: AbilityKind::Spell,
            specialization_ids: vec![],
            acquisition: None,
        })
        .collect()
}

fn el(level: u16, path: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    let spec_id = format!("spec.invoker.{path}");
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: format!("{spec_id}.{}", slug(name)),
            display_name: name.to_string(),
            kind: AbilityKind::Spell,
            specialization_ids: vec![spec_id.clone()],
            acquisition: None,
        })
        .collect()
}
