// Shared Warrior fixture. Uses the `litd_engine` self-alias
// (`extern crate self as litd_engine` in lib.rs) for its type paths so this
// single file is the source of truth for BOTH the `#[cfg(test)]` lib tests and
// the `issue1020_warrior_class_fsv` example (included there via `#[path]`),
// keeping the two byte-identical without duplicating the CF-factual data.
use std::collections::BTreeSet;

use litd_engine::rpg::{
    AbilityKind, AttributeScores, ClassFount, ClassLegacy, ClassLeveledAbility, ClassRaceAccess,
    ClassSpec, ClassSpecialization, ClassVisualContract, PrimeStat, RPG_CLASS_SPEC_SCHEMA,
    RaceAccessRule, WARRIOR_CLASS_ID,
};

/// The authored race roster the class race-gate is validated against. The
/// Warrior itself is `race_access = All`, so this is only exercised by the
/// unknown-race edge probes.
pub(crate) fn known_races() -> BTreeSet<String> {
    ["race.ashen_human".to_string()].into_iter().collect()
}

// Factual mechanics transcribed from wiki.carrionfields.net/class/warrior
// (prime stat STR; any alignment/ethos; no empowerment; weapon specialists;
// two specializations at 20/40; two legacies at 44/48). Name/lore are
// originalized for public release per #984; the numbers are factual per the
// content-issues doctrine (Carrion Fields source).
pub(crate) fn warrior_class_spec() -> ClassSpec {
    ClassSpec {
        schema: RPG_CLASS_SPEC_SCHEMA.to_string(),
        class_id: WARRIOR_CLASS_ID.to_string(),
        max_level: 51,
        display_name: "Warrior".to_string(),
        player_facing_canon_name: "Warrior".to_string(),
        source_issue: 1020,
        canon_issues: vec![986, 1020],
        mechanics_source_url: "https://wiki.carrionfields.net/class/warrior".to_string(),
        source_history_label: "CF Warrior factual mechanics source only".to_string(),
        concept_summary:
            "Frontline weapon masters of the Ashen Reach: the best pure fighters, trading subtle \
             craft and magic for direct martial dominance."
                .to_string(),
        prime_stat: PrimeStat::Strength,
        stat_minimums: AttributeScores::new(13, 0, 0, 0, 0, 0),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        alignment_exceptions: vec![],
        eligibility: litd_engine::rpg::ClassEligibilityPolicy::from_legacy(
            &RaceAccessRule::Any,
            &RaceAccessRule::Any,
        ),
        fount: ClassFount::Martial,
        empowerment: false,
        race_access: ClassRaceAccess::All,
        allowed_weapons: weapon_categories(),
        allowed_armor: vec![
            "armor.light".to_string(),
            "armor.metal".to_string(),
            "armor.bone".to_string(),
        ],
        leveled_abilities: warrior_abilities(),
        specializations: weapon_specializations(),
        specialization_choice_levels: vec![20, 40],
        legacies: macalla_legacies(),
        legacy_choice_levels: vec![44, 48],
        visual: ClassVisualContract {
            renderer: "Rust/wgpu".to_string(),
            vfx_theme_id: "vfx.class.warrior.martial".to_string(),
            signature_vfx: vec![
                "vfx.warrior.enhanced_damage_impact".to_string(),
                "vfx.warrior.berserk_aura".to_string(),
                "vfx.warrior.warcry_shockwave".to_string(),
                "vfx.warrior.parry_spark".to_string(),
                "vfx.warrior.charge_dust".to_string(),
            ],
            palette_notes: "Steel-grey and ember-red martial palette; no copied class iconography."
                .to_string(),
            animation_notes:
                "Weapon-specific attack, block, and finisher VFX bound to the shared biped skeleton."
                    .to_string(),
        },
        implementation_notes: vec![
            "All races and alignments may become Warriors (race_access = All).".to_string(),
            "Warriors receive two specializations, one at level 20 and one at level 40; not all \
             warriors gain every listed skill."
                .to_string(),
            "Legacy display names are Order-of-Macalla lore and must be originalized for public \
             release per #984; only mechanics are factual here."
                .to_string(),
        ],
    }
}

fn weapon_categories() -> Vec<String> {
    [
        "weapon.axe",
        "weapon.dagger",
        "weapon.flail",
        "weapon.mace",
        "weapon.polearm",
        "weapon.spear",
        "weapon.sword",
        "weapon.staff",
        "weapon.whip",
        "weapon.hand_to_hand",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn warrior_abilities() -> Vec<ClassLeveledAbility> {
    let mut abilities = vec![
        la(1, "axe", "Axe Proficiency"),
        la(1, "dagger", "Dagger Proficiency"),
        la(1, "flail", "Flail Proficiency"),
        la(1, "mace", "Mace Proficiency"),
        la(1, "polearm", "Polearm Proficiency"),
        la(1, "spear", "Spear Proficiency"),
        la(1, "sword", "Sword Proficiency"),
        la(1, "staff", "Staff Proficiency"),
        la(1, "whip", "Whip Proficiency"),
        la(1, "enhanced_damage", "Enhanced Damage"),
        la(1, "hand_to_hand", "Hand to Hand"),
        la(1, "parry", "Parry"),
        la(1, "rescue", "Rescue"),
        la(1, "recall", "Recall"),
        la(1, "shield_block", "Shield Block"),
        la(2, "elbow", "Elbow"),
        la(2, "metal_armor_use", "Metal Armor Use"),
        la(2, "light_armor_use", "Light Armor Use"),
        la(3, "dirt_kicking", "Dirt Kicking"),
        la(4, "aim", "Aim"),
        la(4, "knee", "Knee"),
        la(5, "second_attack", "Second Attack"),
        la(6, "fast_healing", "Fast Healing"),
        la(7, "recuperate", "Recuperate"),
        la(8, "kick", "Kick"),
        la(10, "disarm", "Disarm"),
        la(10, "pen", "Pen"),
        la(10, "feint", "Feint"),
        la(12, "third_attack", "Third Attack"),
        la(12, "dual_wield", "Dual Wield"),
        la(13, "dodge", "Dodge"),
        la(14, "haggle", "Haggle"),
        la(14, "pierce", "Pierce"),
        la(15, "bash", "Bash"),
        la(15, "trip", "Trip"),
        la(15, "shield_cleave", "Shield Cleave"),
        la(15, "pugil", "Pugil"),
        la(15, "meditation", "Meditation"),
        la(16, "charge", "Charge"),
        la(17, "thrust", "Thrust"),
        la(17, "lore", "Lore"),
        la(18, "berserk", "Berserk"),
        la(18, "lash", "Lash"),
        la(19, "slice", "Slice"),
        la(20, "warcry", "Warcry"),
        la(21, "retreat", "Retreat"),
        la(23, "offhand_disarm", "Offhand Disarm"),
        la(24, "evade", "Evade"),
        la(25, "drive", "Drive"),
        la(27, "parting_blow", "Parting Blow"),
        la(30, "fourth_attack", "Fourth Attack"),
        la(30, "dash", "Dash"),
        la(35, "precise_aim", "Precise Aim"),
        la(46, "enhanced_reactions", "Enhanced Reactions"),
    ];
    abilities.extend([
        las(20, "flourintine", "Flourintine", "spec.weapon.sword"),
        las(20, "whirl", "Whirl", "spec.weapon.axe"),
        las(20, "backhand", "Backhand", "spec.weapon.mace"),
        las(22, "cross", "Cross", "spec.weapon.sword"),
        las(23, "underhand", "Underhand", "spec.weapon.dagger"),
        las(28, "boneshatter", "Boneshatter", "spec.weapon.mace"),
        las(31, "flurry", "Flurry", "spec.weapon.sword"),
    ]);
    abilities
}

fn weapon_specializations() -> Vec<ClassSpecialization> {
    [
        ("spec.weapon.axe", "Axe Specialization"),
        ("spec.weapon.dagger", "Dagger Specialization"),
        ("spec.weapon.flail", "Flail Specialization"),
        ("spec.weapon.mace", "Mace Specialization"),
        ("spec.weapon.polearm", "Polearm Specialization"),
        ("spec.weapon.spear", "Spear Specialization"),
        ("spec.weapon.sword", "Sword Specialization"),
        ("spec.weapon.staff", "Staff Specialization"),
        ("spec.weapon.whip", "Whip Specialization"),
        ("spec.weapon.hand_to_hand", "Hand to Hand Specialization"),
    ]
    .into_iter()
    .map(|(id, name)| ClassSpecialization {
        specialization_id: id.to_string(),
        display_name: name.to_string(),
        unlock_level: 20,
        description: format!("{name}: unlocks specialist maneuvers for the chosen weapon type."),
        eligibility: None,
    })
    .collect()
}

fn macalla_legacies() -> Vec<ClassLegacy> {
    [
        ("autumn_harvest", "Autumn Harvest"),
        ("balance_of_the_sisters", "Balance of the Sisters"),
        ("calming_the_tempest", "Calming the Tempest"),
        ("chilling_embrace", "Chilling Embrace"),
        ("crashing_of_waves", "Crashing of Waves"),
        ("cry_of_thunder", "Cry of Thunder"),
        (
            "dance_upon_the_southern_wind",
            "Dance Upon the Southern Wind",
        ),
        ("enigma_of_the_thirteen", "Enigma of the Thirteen"),
        ("fires_of_adversity", "Fires of Adversity"),
        ("fist_of_the_titan", "Fist of the Titan"),
        ("flow_of_shadows", "Flow of Shadows"),
        ("fluid_deceptions", "Fluid Deceptions"),
        ("forsaken_thoughts", "Forsaken Thoughts"),
        ("fortress_of_the_spirit", "Fortress of the Spirit"),
        ("gates_of_the_forge", "Gates of the Forge"),
        ("greeting_the_avalanche", "Greeting the Avalanche"),
        ("harmonious_equilibrium", "Harmonious Equilibrium"),
        ("hour_past_midnight", "Hour Past Midnight"),
        ("incarnadine_wave", "Incarnadine Wave"),
        (
            "landslide_of_the_second_breath",
            "Landslide of the Second Breath",
        ),
        ("maelstrom_of_the_veils", "Maelstrom of the Veils"),
        ("mercy_of_the_typhoon", "Mercy of the Typhoon"),
        ("one_with_shadows", "One With Shadows"),
        ("outcry_of_steel", "Outcry of Steel"),
        ("place_of_the_raging_river", "Place of the Raging River"),
        ("renewal_of_spring", "Renewal of Spring"),
        ("riddle_of_resilience", "Riddle of Resilience"),
        ("soul_of_the_mountain", "Soul of the Mountain"),
        ("space_between_heartbeats", "Space Between Heartbeats"),
        (
            "striking_the_shadows_footfall",
            "Striking the Shadows Footfall",
        ),
        ("surrounding_the_sunrise", "Surrounding the Sunrise"),
        ("trapping_beneath_thunder", "Trapping Beneath Thunder"),
        ("vanguards_desire", "Vanguards Desire"),
        ("ward_of_blades", "Ward of Blades"),
        ("whirlwind_of_steel", "Whirlwind of Steel"),
        ("whispers_of_the_great_siege", "Whispers of the Great Siege"),
    ]
    .into_iter()
    .map(|(id, name)| ClassLegacy {
        legacy_id: format!("legacy.macalla.{id}"),
        display_name: name.to_string(),
        description: format!("Order of Macalla legacy technique: {name}."),
    })
    .collect()
}

fn la(level: u16, ability_id: &str, display_name: &str) -> ClassLeveledAbility {
    ClassLeveledAbility {
        level,
        ability_id: ability_id.to_string(),
        display_name: display_name.to_string(),
        kind: AbilityKind::Skill,
        specialization_ids: vec![],
        acquisition: None,
    }
}

fn las(
    level: u16,
    ability_id: &str,
    display_name: &str,
    specialization_id: &str,
) -> ClassLeveledAbility {
    ClassLeveledAbility {
        level,
        ability_id: ability_id.to_string(),
        display_name: display_name.to_string(),
        kind: AbilityKind::Skill,
        specialization_ids: vec![specialization_id.to_string()],
        acquisition: None,
    }
}
