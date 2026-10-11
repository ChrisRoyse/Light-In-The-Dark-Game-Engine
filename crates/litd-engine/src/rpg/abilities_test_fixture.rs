use crate::{
    EngineError,
    rpg::{
        AbilityActivation, AbilityCatalog, AbilityCategory, AbilityCost, AbilityDefinition,
        AbilityEffect, AbilityEffectKind, AbilityKind, AbilityPrerequisite, AbilityRequirements,
        AbilityStackingPolicy, AbilityTargetMode, AbilityTargeting, AbilityTiming,
        RPG_ABILITY_SCHEMA, ResourceCostTotal, build_ability_catalog_readback,
    },
};

pub(crate) fn happy_catalog() -> AbilityCatalog {
    AbilityCatalog {
        schema: RPG_ABILITY_SCHEMA.to_string(),
        catalog_id: "catalog.ashen_reach.issue961".to_string(),
        abilities: vec![
            iron_guard(),
            cinder_bash(),
            chain_lightning(),
            ember_sanctuary(),
            chorus_of_iron(),
            bastion_seal(),
            ash_blood_reflex(),
        ],
    }
}

fn iron_guard() -> AbilityDefinition {
    let mut ability = base(
        "ability.iron_guard",
        "Iron Guard",
        AbilityKind::Skill,
        AbilityActivation::Passive,
        "class.warrior",
    );
    ability.description =
        "Hold a practiced defensive stance that improves block order.".to_string();
    ability.requirements = requirements(RequirementsArgs {
        min_level: 1,
        classes: &["class.warrior"],
        races: &[],
        cabals: &[],
        alignments: &[],
        ethos: &[],
        domains: &[],
        prerequisites: &[],
    });
    ability.targeting = target(AbilityTargetMode::None, 0, 0);
    ability.effects = vec![effect(
        "effect.guard_order",
        AbilityEffectKind::ApplyAffect,
        "defense_order",
        1,
        1,
        0,
    )];
    ability
}

fn cinder_bash() -> AbilityDefinition {
    let mut ability = active_base(
        "ability.cinder_bash",
        "Cinder Bash",
        AbilityKind::Skill,
        "class.warrior",
        AbilityTargetMode::Enemy,
    );
    ability.requirements.prerequisites = vec![prereq("ability.iron_guard", 1)];
    ability.requirements.min_level = 8;
    ability.costs = vec![cost("movement", 12)];
    ability.timing.lag_ticks = 2;
    ability.timing.cooldown_ticks = 4;
    ability.timing.cooldown_group = Some("cooldown.bash".to_string());
    ability.effects = vec![
        effect(
            "effect.cinder_impact",
            AbilityEffectKind::Damage,
            "physical",
            42,
            54,
            0,
        ),
        effect(
            "effect.cinder_wait",
            AbilityEffectKind::StateTag,
            "wait_state",
            1,
            1,
            2,
        ),
    ];
    ability.ui_slot = Some("hotbar.1".to_string());
    ability
}

fn chain_lightning() -> AbilityDefinition {
    let mut ability = active_base(
        "ability.chain_lightning",
        "Chain Lightning",
        AbilityKind::Spell,
        "class.invoker",
        AbilityTargetMode::Area,
    );
    set_category(
        &mut ability,
        "school.lightning",
        "path.storm",
        "branch.elemental_storm",
    );
    ability.category.rank = 5;
    ability.category.max_rank = 5;
    ability.category.capstone = true;
    ability.requirements.min_level = 18;
    ability.requirements.class_ids = ids(&["class.invoker"]);
    ability.requirements.prerequisites = vec![prereq("ability.cinder_bash", 1)];
    ability.costs = vec![cost("mana", 180)];
    ability.timing.cast_ticks = 2;
    ability.timing.round_ticks = 1;
    ability.timing.cooldown_ticks = 12;
    ability.timing.cooldown_group = Some("cooldown.lightning".to_string());
    ability.targeting.max_targets = 4;
    ability.targeting.range_tiles = 8;
    ability.effects = vec![effect(
        "effect.chain_lightning_damage",
        AbilityEffectKind::Damage,
        "lightning",
        250,
        300,
        0,
    )];
    ability.ui_slot = Some("hotbar.6".to_string());
    ability
}

fn ember_sanctuary() -> AbilityDefinition {
    let mut ability = active_base(
        "ability.ember_sanctuary",
        "Ember Sanctuary",
        AbilityKind::Supplication,
        "faith.dawnward",
        AbilityTargetMode::Ally,
    );
    ability.requirements = requirements(RequirementsArgs {
        min_level: 10,
        classes: &[],
        races: &[],
        cabals: &[],
        alignments: &[],
        ethos: &[],
        domains: &["domain.dawn"],
        prerequisites: &[prereq("ability.iron_guard", 1)],
    });
    set_category(
        &mut ability,
        "school.holy",
        "path.ember_ward",
        "branch.sanctuary",
    );
    ability.costs = vec![cost("devotion", 45), cost("mana", 60)];
    ability.timing.cooldown_ticks = 10;
    ability.effects = vec![
        effect(
            "effect.sanctuary_heal",
            AbilityEffectKind::Heal,
            "holy",
            90,
            120,
            0,
        ),
        effect(
            "effect.sanctuary_ward",
            AbilityEffectKind::ApplyAffect,
            "holy_ward",
            15,
            15,
            6,
        ),
    ];
    ability.ui_slot = Some("hotbar.3".to_string());
    ability
}

fn chorus_of_iron() -> AbilityDefinition {
    let mut ability = active_base(
        "ability.chorus_of_iron",
        "Chorus of Iron",
        AbilityKind::Song,
        "class.bard",
        AbilityTargetMode::Room,
    );
    ability.requirements.min_level = 12;
    ability.requirements.class_ids = ids(&["class.bard"]);
    ability.requirements.prerequisites = vec![prereq("ability.iron_guard", 1)];
    set_category(
        &mut ability,
        "school.song",
        "path.battle_hymn",
        "branch.morale",
    );
    ability.costs = vec![cost("focus", 20)];
    ability.timing.round_ticks = 1;
    ability.timing.cooldown_ticks = 6;
    ability.targeting.max_targets = 12;
    ability.effects = vec![effect(
        "effect.chorus_morale",
        AbilityEffectKind::ApplyAffect,
        "morale",
        8,
        8,
        5,
    )];
    ability.ui_slot = Some("hotbar.5".to_string());
    ability
}

fn bastion_seal() -> AbilityDefinition {
    let mut ability = active_base(
        "ability.bastion_seal",
        "Bastion Seal",
        AbilityKind::CabalPower,
        "cabal.verdict",
        AbilityTargetMode::Enemy,
    );
    ability.requirements = requirements(RequirementsArgs {
        min_level: 20,
        classes: &[],
        races: &[],
        cabals: &["cabal.verdict"],
        alignments: &[],
        ethos: &[],
        domains: &[],
        prerequisites: &[prereq("ability.iron_guard", 1)],
    });
    set_category(
        &mut ability,
        "school.cabal",
        "path.verdict_seal",
        "branch.bastion",
    );
    ability.costs = vec![cost("cabal_charge", 3)];
    ability.timing.cooldown_ticks = 30;
    ability.effects = vec![
        effect(
            "effect.bastion_purge",
            AbilityEffectKind::Dispel,
            "cabal",
            1,
            2,
            0,
        ),
        effect(
            "effect.bastion_burn",
            AbilityEffectKind::Damage,
            "holy",
            80,
            110,
            0,
        ),
    ];
    ability.ui_slot = Some("hotbar.9".to_string());
    ability
}

fn ash_blood_reflex() -> AbilityDefinition {
    let mut ability = base(
        "ability.ash_blood_reflex",
        "Ash Blood Reflex",
        AbilityKind::RacialInnate,
        AbilityActivation::Automatic,
        "race.ashen_human",
    );
    ability.requirements = requirements(RequirementsArgs {
        min_level: 1,
        classes: &[],
        races: &["race.ashen_human"],
        cabals: &[],
        alignments: &[],
        ethos: &[],
        domains: &[],
        prerequisites: &[],
    });
    ability.targeting = target(AbilityTargetMode::SelfOnly, 1, 0);
    ability.timing.cooldown_ticks = 20;
    ability.timing.cooldown_group = Some("cooldown.racial_reflex".to_string());
    ability.trigger_tags = ids(&["trigger.low_health"]);
    ability.effects = vec![effect(
        "effect.ash_reflex_tag",
        AbilityEffectKind::StateTag,
        "survival_reflex",
        1,
        1,
        4,
    )];
    ability
}

fn active_base(
    id: &str,
    name: &str,
    kind: AbilityKind,
    family: &str,
    mode: AbilityTargetMode,
) -> AbilityDefinition {
    let mut ability = base(id, name, kind, AbilityActivation::Active, family);
    ability.targeting = target(mode, 1, 3);
    ability
}

fn base(
    id: &str,
    name: &str,
    kind: AbilityKind,
    activation: AbilityActivation,
    family: &str,
) -> AbilityDefinition {
    AbilityDefinition {
        ability_id: id.to_string(),
        display_name: name.to_string(),
        description: format!("{name} issue961 fixture ability."),
        kind,
        activation,
        category: AbilityCategory {
            family_id: family.to_string(),
            school_id: Some("school.martial".to_string()),
            path_id: Some("path.frontline".to_string()),
            branch_id: Some("branch.guardian".to_string()),
            rank: 1,
            max_rank: 5,
            capstone: false,
        },
        requirements: requirements(RequirementsArgs {
            min_level: 1,
            classes: &[family],
            races: &[],
            cabals: &[],
            alignments: &[],
            ethos: &[],
            domains: &[],
            prerequisites: &[],
        }),
        costs: Vec::new(),
        timing: AbilityTiming {
            cast_ticks: 0,
            round_ticks: 0,
            lag_ticks: 0,
            cooldown_ticks: 0,
            cooldown_group: None,
        },
        targeting: target(AbilityTargetMode::None, 0, 0),
        effects: Vec::new(),
        tags: ids(&["ability.issue961"]),
        grants_tags: Vec::new(),
        blocks_tags: Vec::new(),
        trigger_tags: Vec::new(),
        ui_slot: None,
    }
}

struct RequirementsArgs<'a> {
    min_level: u16,
    classes: &'a [&'a str],
    races: &'a [&'a str],
    cabals: &'a [&'a str],
    alignments: &'a [&'a str],
    ethos: &'a [&'a str],
    domains: &'a [&'a str],
    prerequisites: &'a [AbilityPrerequisite],
}

fn requirements(args: RequirementsArgs<'_>) -> AbilityRequirements {
    let RequirementsArgs {
        min_level,
        classes,
        races,
        cabals,
        alignments,
        ethos,
        domains,
        prerequisites,
    } = args;
    AbilityRequirements {
        min_level,
        class_ids: ids(classes),
        race_ids: ids(races),
        cabal_ids: ids(cabals),
        alignments: ids(alignments),
        ethos: ids(ethos),
        deity_domains: ids(domains),
        prerequisites: prerequisites.to_vec(),
    }
}

fn target(mode: AbilityTargetMode, max_targets: u16, range_tiles: u16) -> AbilityTargeting {
    AbilityTargeting {
        mode,
        max_targets,
        range_tiles,
        requires_line_of_sight: mode != AbilityTargetMode::None,
        allow_friendly_fire: false,
    }
}

fn effect(
    id: &str,
    kind: AbilityEffectKind,
    channel: &str,
    min: i32,
    max: i32,
    duration_ticks: u32,
) -> AbilityEffect {
    AbilityEffect {
        effect_id: id.to_string(),
        kind,
        channel: channel.to_string(),
        magnitude_min: min,
        magnitude_max: max,
        duration_ticks,
        tick_interval: 0,
        stacking_policy: AbilityStackingPolicy::None,
        stack_cap: 0,
    }
}

fn cost(resource_id: &str, amount: i32) -> AbilityCost {
    AbilityCost {
        resource_id: resource_id.to_string(),
        amount,
    }
}

fn prereq(ability_id: &str, min_rank: u16) -> AbilityPrerequisite {
    AbilityPrerequisite {
        ability_id: ability_id.to_string(),
        min_rank,
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn set_category(ability: &mut AbilityDefinition, school: &str, path: &str, branch: &str) {
    ability.category.school_id = Some(school.to_string());
    ability.category.path_id = Some(path.to_string());
    ability.category.branch_id = Some(branch.to_string());
}

pub(crate) fn duplicate_ability() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.abilities.push(catalog.abilities[0].clone());
    build_ability_catalog_readback(&catalog).map(|_| ())
}

pub(crate) fn unknown_prerequisite() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.abilities[1].requirements.prerequisites[0].ability_id = "ability.missing".to_string();
    build_ability_catalog_readback(&catalog).map(|_| ())
}

pub(crate) fn passive_with_cost() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.abilities[0].costs = vec![cost("mana", 1)];
    build_ability_catalog_readback(&catalog).map(|_| ())
}

pub(crate) fn automatic_without_trigger() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.abilities[6].trigger_tags.clear();
    build_ability_catalog_readback(&catalog).map(|_| ())
}

pub(crate) fn active_without_cost_or_wait() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.abilities[1].costs.clear();
    catalog.abilities[1].timing.lag_ticks = 0;
    catalog.abilities[1].timing.cooldown_ticks = 0;
    catalog.abilities[1].timing.cooldown_group = None;
    build_ability_catalog_readback(&catalog).map(|_| ())
}

pub(crate) fn resource_total(totals: &[ResourceCostTotal], resource_id: &str) -> i64 {
    totals
        .iter()
        .find(|total| total.resource_id == resource_id)
        .unwrap()
        .amount
}
