// Factual mechanics transcribed from wiki.carrionfields.net/class/ranger (issue #1023 FACTUAL
// SPEC comment). Name/lore originalized per #984; mechanics are factual CF data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassEligibilityPolicy, ClassFount, ClassLeveledAbility,
    ClassRaceAccess, ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat,
    RPG_CLASS_SPEC_SCHEMA, RaceAccessRule,
};

pub(crate) const RANGER_CLASS_ID: &str = "class.thornwarden";

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

// CF race restriction (verbatim, #1023): a versatile-with-restrictions list naming Cloud Giant,
// Human, Elf, Wood-Elf, Half-Elf, Half-Drow, Frost Giant, Centaur, Felar, Saurian, Storm Giant,
// Fire Giant, Dark-Elf, Arial, Gnome, and Svirfneblin; Dwarf/Duergar/Orc/Goblin/Minotaur are
// unmentioned (excluded). Centaur has no authored race analog in this roster.
pub(crate) fn ranger_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: RANGER_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Thornwarden".to_string(),
        player_facing_canon_name: "Thornwarden".to_string(),
        source_issue: 1023,
        canon_issues: vec![986, 1023],
        mechanics_source_url: "https://wiki.carrionfields.net/class/ranger".to_string(),
        source_history_label: "CF Ranger factual mechanics source only".to_string(),
        concept_summary: "Guardians of the wild who keep root-law against the encroachment of \
             walls: a home terrain and a combat/lore expertise shape which of the class's wide \
             skill and spell list a given warden actually gains."
            .to_string(),
        prime_stat: PrimeStat::Strength,
        stat_minimums: AttributeScores::new(13, 0, 0, 0, 0, 0),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Any,
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Unresolved {
            reason: "Ranger nature/animism magic ('evil individuals are unable to practice \
                     animism') is spirit-drawn, but the Ashen Reach cosmology docs do not \
                     classify its fount; open operator/lore decision per #1754."
                .to_string(),
        },
        empowerment: false,
        race_access: ClassRaceAccess::Restricted {
            race_ids: [
                "race.ashen_human",
                "race.elf",
                "race.wood_elf",
                "race.half_elf",
                "race.ashkin_mixed",
                "race.frost_giant",
                "race.felisar",
                "race.saur",
                "race.storm_giant",
                "race.fire_giant",
                "race.ashkin_deep",
                "race.sylphkin",
                "race.gnome",
                "race.deep_kin",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        allowed_weapons: ["axe", "dagger", "spear", "sword", "staff", "mace", "whip"]
            .into_iter()
            .map(|w| format!("weapon.{w}"))
            .collect(),
        allowed_armor: vec!["armor.light".to_string(), "armor.bone".to_string()],
        leveled_abilities: ranger_abilities(),
        specializations: ranger_paths(),
        specialization_choice_levels: vec![1],
        legacies: Vec::new(),
        legacy_choice_levels: Vec::new(),
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.ranger.wilderness".to_string(),
            signature_vfx: vec![
                "vfx.ranger.ambush_flash".to_string(),
                "vfx.ranger.camouflage_shimmer".to_string(),
                "vfx.ranger.charge_dust".to_string(),
                "vfx.ranger.entangle_roots".to_string(),
            ],
            palette_notes: "Moss-green and bark-brown wilderness palette; no copied class \
                 iconography."
                .to_string(),
            animation_notes: "Terrain-specific ambient VFX (bedouin heat-shimmer, caverndweller \
                 dust, mariner spray) layered under shared biped skeleton animations."
                .to_string(),
        },
        implementation_notes: vec![
            "Centaur is named in the CF race text but has no authored RaceSpec in this roster \
             yet; it is simply absent from the allow accounting here, not silently included."
                .to_string(),
            "CF's two race-gating matrices (which races may take which terrain/expertise) are \
             not modeled; only the class-level race_access gate is captured. ClassSpecialization \
             has no per-specialization race-gate field. Filed as a framework gap."
                .to_string(),
            "CF states 'Rangers have both a home terrain and an expertise', chosen once at \
             creation; specialization_choice_levels is therefore [1] for all 13 \
             terrain/expertise rows, not the level of each row's first tagged ability."
                .to_string(),
            "Three CF entries are dual-tagged (draw breath: marshdweller/mariner; sure footing: \
             mountaineer/caverndweller; pursuit: plainsrunner/marshdweller). Both terrain tags \
             are now captured in ClassLeveledAbility.specialization_ids (schema v2, gap 4, \
             #1718) via the `td` helper; the first-listed tag still owns the ability id."
                .to_string(),
            "Prime-stat nuance (verbatim): humans and half-breeds may choose their own prime \
             stat instead of Strength; ClassSpec has one prime_stat field, so the base STR value \
             is captured and the exception is noted here, not modeled."
                .to_string(),
            "stat_minimums uses a prime-stat-only floor of 13 (STR), the same convention as the \
             Warrior pilot (#1020)."
                .to_string(),
        ],
    }
}

fn ranger_abilities() -> Vec<ClassLeveledAbility> {
    let mut a = Vec::new();
    a.extend(g(
        1,
        &[
            "Axe",
            "Dagger",
            "Spear",
            "Sword",
            "Staff",
            "Whip",
            "Parry",
            "Scrolls",
            "Talismans",
            "Recall",
        ],
    ));
    a.extend(t(1, "spec.ranger.expertise.savage", &["Mace"]));
    a.extend(g(2, &["Light Armor Use"]));
    a.extend(g(3, &["Elbow"]));
    a.extend(g(4, &["Camouflage"]));
    a.extend(g(5, &["Dirt Kicking", "Quiet Movement"]));
    a.extend(g(6, &["Fast Healing", "Knee"]));
    a.extend(g(7, &["Acute Vision", "Pathfinding", "Aim", "Dark Vision"]));
    a.extend(g(8, &["Kick", "Second Attack"]));
    a.extend(g(9, &["Dodge", "Butcher"]));
    a.extend(g(
        10,
        &["Herbs", "Protection Heat Cold", "Pen", "Shield Block"],
    ));
    a.extend(t(10, "spec.ranger.terrain.mariner", &["Navigation"]));
    a.extend(g(11, &["Disarm"]));
    a.extend(g(12, &["Ambush", "Bone Armor Use"]));
    a.extend(t(12, "spec.ranger.terrain.mountaineer", &["Endure Cold"]));
    a.extend(t(12, "spec.ranger.terrain.bedouin", &["Endure Heat"]));
    a.extend(g(13, &["Enhanced Damage", "Lore"]));
    a.extend(g(14, &["Hand to Hand"]));
    a.push(td(14, "marshdweller", "mariner", "Draw Breath"));
    a.extend(g(15, &["Haggle", "Tame", "Bark Skin"]));
    a.extend(t(15, "spec.ranger.expertise.hunter", &["Archery"]));
    a.extend(g(16, &["Lash", "Meditation", "Pierce"]));
    a.extend(t(16, "spec.ranger.expertise.explorer", &["Wanderlust"]));
    a.extend(g(18, &["Dual Wield"]));
    a.extend(t(18, "spec.ranger.expertise.animist", &["Trance"]));
    a.extend(g(19, &["Find Water", "Creep"]));
    a.extend(g(
        20,
        &["Shield Cleave", "Pugil", "Warcry", "Serpent Strike"],
    ));
    a.extend(t(20, "spec.ranger.terrain.bedouin", &["Sandswirl"]));
    a.extend(t(
        20,
        "spec.ranger.terrain.plainsrunner",
        &["Plainsrunning"],
    ));
    a.extend(t(20, "spec.ranger.expertise.savage", &["Brutal Rush"]));
    a.extend(g(21, &["Thrust"]));
    a.extend(t(21, "spec.ranger.expertise.explorer", &["Avoidance"]));
    a.extend(g(22, &["Camp"]));
    a.extend(t(22, "spec.ranger.expertise.hunter", &["Poison Arrow"]));
    a.extend(t(
        22,
        "spec.ranger.expertise.beastmaster",
        &["Predators Stance"],
    ));
    a.extend(g(23, &["Charge"]));
    a.extend(t(24, "spec.ranger.terrain.marshdweller", &["Bloodletting"]));
    a.extend(t(24, "spec.ranger.expertise.explorer", &["Resilience"]));
    a.extend(g(25, &["Third Attack"]));
    a.push(td(25, "mountaineer", "caverndweller", "Sure Footing"));
    a.extend(t(
        25,
        "spec.ranger.expertise.hunter",
        &["Rapid Shot", "Bait"],
    ));
    a.extend(t(25, "spec.ranger.terrain.bedouin", &["Stash"]));
    a.extend(t(25, "spec.ranger.expertise.savage", &["Improvise Weapon"]));
    a.extend(g(26, &["Evade"]));
    a.extend(g(28, &["Survivalist"]));
    a.extend(t(28, "spec.ranger.expertise.explorer", &["Watchful Eye"]));
    a.extend(t(28, "spec.ranger.expertise.savage", &["Smear Mud"]));
    a.extend(g(30, &["Fashion Staff", "Track", "Bearcharge"]));
    a.extend(t(30, "spec.ranger.expertise.hunter", &["Flaming Arrow"]));
    a.extend(t(30, "spec.ranger.terrain.marshdweller", &["Swampmire"]));
    a.extend(t(30, "spec.ranger.expertise.savage", &["Staunch"]));
    a.extend(t(31, "spec.ranger.expertise.explorer", &["Tend Wounds"]));
    a.extend(g(32, &["Fashion Spear", "Waylay"]));
    a.extend(t(
        32,
        "spec.ranger.terrain.caverndweller",
        &["Rebounding Strike"],
    ));
    a.extend(t(32, "spec.ranger.expertise.hunter", &["Aimed Shot"]));
    a.extend(t(32, "spec.ranger.terrain.mariner", &["Harpoon"]));
    a.extend(t(33, "spec.ranger.expertise.animist", &["Spirit Allies"]));
    a.extend(t(
        33,
        "spec.ranger.expertise.hunter",
        &["Observe", "Fashion Bow"],
    ));
    a.extend(t(33, "spec.ranger.expertise.explorer", &["Scatter"]));
    a.extend(t(33, "spec.ranger.expertise.savage", &["Warpaint"]));
    a.extend(g(35, &["Backrake", "Wilderness Familiarity"]));
    a.extend(t(35, "spec.ranger.terrain.plainsrunner", &["Bramble Pit"]));
    a.extend(t(
        36,
        "spec.ranger.expertise.beastmaster",
        &["Bear Kinship"],
    ));
    a.extend(t(36, "spec.ranger.expertise.hunter", &["Aerial Shot"]));
    a.extend(t(
        37,
        "spec.ranger.terrain.caverndweller",
        &["Shadowstride"],
    ));
    a.extend(t(
        38,
        "spec.ranger.expertise.hunter",
        &["Stalker of the Wilds"],
    ));
    a.extend(t(
        38,
        "spec.ranger.expertise.beastmaster",
        &["Serpent Kinship"],
    ));
    a.extend(t(38, "spec.ranger.terrain.mariner", &["Netcasting"]));
    a.extend(t(
        39,
        "spec.ranger.terrain.caverndweller",
        &["Deafening Howl"],
    ));
    a.extend(g(40, &["Snare"]));
    a.extend(t(40, "spec.ranger.expertise.hunter", &["Precision"]));
    a.extend(t(40, "spec.ranger.terrain.caverndweller", &["Smokescreen"]));
    a.extend(t(42, "spec.ranger.terrain.bedouin", &["Dervish Dance"]));
    a.extend(t(42, "spec.ranger.expertise.savage", &["Savage Blow"]));
    a.extend(t(44, "spec.ranger.expertise.hunter", &["Timed Attack"]));
    a.push(td(45, "plainsrunner", "marshdweller", "Pursuit"));
    a.extend(t(
        46,
        "spec.ranger.expertise.beastmaster",
        &["Wolf Kinship"],
    ));

    a.extend(gs(8, &["Armor"]));
    a.extend(gs(17, &["Call Lightning"]));
    a.extend(gs(19, &["Control Weather"]));
    a.extend(gs(20, &["Entangle"]));
    a.extend(ts(22, "spec.ranger.expertise.animist", &["Animism"]));
    a.extend(ts(
        23,
        "spec.ranger.expertise.savage",
        &["Protection from Metals"],
    ));
    a.extend(ts(25, "spec.ranger.expertise.animist", &["Zephyr"]));
    a.extend(ts(28, "spec.ranger.expertise.animist", &["Symbiosis"]));
    a.extend(ts(
        28,
        "spec.ranger.terrain.marshdweller",
        &["Swarm of Leeches"],
    ));
    a.extend(ts(
        29,
        "spec.ranger.expertise.animist",
        &["Ancestral Spirit"],
    ));
    a.extend(gs(30, &["Imbue Weapon"]));
    a.extend(ts(30, "spec.ranger.expertise.animist", &["Call Messenger"]));
    a.extend(ts(
        31,
        "spec.ranger.expertise.animist",
        &["Speak with Spirits"],
    ));
    a.extend(ts(
        31,
        "spec.ranger.expertise.beastmaster",
        &["Cloud of Wasps"],
    ));
    a.extend(ts(32, "spec.ranger.terrain.bedouin", &["Whirling Simoon"]));
    a.extend(ts(
        38,
        "spec.ranger.expertise.animist",
        &["Primordial Vengeance"],
    ));
    a.extend(ts(
        38,
        "spec.ranger.terrain.marshdweller",
        &["Scourge of Mosquitoes"],
    ));
    a.extend(ts(40, "spec.ranger.expertise.animist", &["Gust of Wind"]));
    a.extend(ts(42, "spec.ranger.terrain.mountaineer", &["Boreal Wind"]));
    a.extend(ts(
        42,
        "spec.ranger.expertise.animist",
        &["Spirit Guardian"],
    ));
    a.extend(ts(45, "spec.ranger.terrain.caverndweller", &["Cavernfall"]));
    a.extend(ts(45, "spec.ranger.terrain.mariner", &["Stormcalling"]));
    a.extend(ts(46, "spec.ranger.expertise.animist", &["Spirit Howl"]));
    a
}

fn ranger_paths() -> Vec<ClassSpecialization> {
    let terrains = [
        (
            "spec.ranger.terrain.forester",
            "Forester",
            "Baseline home terrain; CF: 'true traditionalist in ranger lore', no listed swaps.",
        ),
        (
            "spec.ranger.terrain.plainsrunner",
            "Plainsrunner",
            "Open-plains home terrain.",
        ),
        (
            "spec.ranger.terrain.bedouin",
            "Bedouin",
            "Desert home terrain.",
        ),
        (
            "spec.ranger.terrain.mountaineer",
            "Mountaineer",
            "Mountain home terrain.",
        ),
        (
            "spec.ranger.terrain.caverndweller",
            "Caverndweller",
            "Underground home terrain.",
        ),
        (
            "spec.ranger.terrain.marshdweller",
            "Marshdweller",
            "Wetland home terrain.",
        ),
        (
            "spec.ranger.terrain.mariner",
            "Mariner",
            "Seaborne home terrain.",
        ),
    ];
    let expertises = [
        (
            "spec.ranger.expertise.survivalist",
            "Survivalist",
            "CF: 'do not lose any base ranger abilities' / 'do not gain any additional abilities.'",
        ),
        (
            "spec.ranger.expertise.animist",
            "Animist",
            "Spirit-communion expertise; CF: 'evil individuals are unable to practice animism.'",
        ),
        (
            "spec.ranger.expertise.beastmaster",
            "Beastmaster",
            "Beast-kinship expertise.",
        ),
        (
            "spec.ranger.expertise.explorer",
            "Explorer",
            "Wayfinding/resilience expertise.",
        ),
        (
            "spec.ranger.expertise.hunter",
            "Hunter",
            "Ranged/archery expertise.",
        ),
        (
            "spec.ranger.expertise.savage",
            "Savage",
            "Brute-force melee expertise.",
        ),
    ];
    terrains
        .into_iter()
        .chain(expertises)
        .map(|(id, name, desc)| ClassSpecialization {
            specialization_id: id.to_string(),
            display_name: name.to_string(),
            unlock_level: 1,
            description: desc.to_string(),
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
    ability_group(level, AbilityKind::Skill, None, names)
}

fn gs(level: u16, names: &[&str]) -> Vec<ClassLeveledAbility> {
    ability_group(level, AbilityKind::Spell, None, names)
}

fn t(level: u16, spec_id: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    ability_group(level, AbilityKind::Skill, Some(spec_id), names)
}

fn ts(level: u16, spec_id: &str, names: &[&str]) -> Vec<ClassLeveledAbility> {
    ability_group(level, AbilityKind::Spell, Some(spec_id), names)
}

/// A dual-tagged terrain skill (schema v2, gap 4): CF lists a few ranger skills
/// under two terrains at once. `primary` (first-listed CF tag) owns the derived
/// ability id; both terrain ids join the membership set. Verbatim CF pairs: draw
/// breath (marshdweller/mariner), sure footing (mountaineer/caverndweller),
/// pursuit (plainsrunner/marshdweller).
fn td(level: u16, primary: &str, secondary: &str, name: &str) -> ClassLeveledAbility {
    let primary = format!("spec.ranger.terrain.{primary}");
    ClassLeveledAbility {
        level,
        ability_id: format!("{primary}.{}", slug(name)),
        display_name: name.to_string(),
        kind: AbilityKind::Skill,
        specialization_ids: vec![primary, format!("spec.ranger.terrain.{secondary}")],
        acquisition: None,
    }
}

fn ability_group(
    level: u16,
    kind: AbilityKind,
    spec_id: Option<&str>,
    names: &[&str],
) -> Vec<ClassLeveledAbility> {
    names
        .iter()
        .map(|name| ClassLeveledAbility {
            level,
            ability_id: match spec_id {
                Some(spec) => format!("{spec}.{}", slug(name)),
                None => slug(name),
            },
            display_name: name.to_string(),
            kind,
            specialization_ids: spec_id.map(str::to_string).into_iter().collect(),
            acquisition: None,
        })
        .collect()
}
