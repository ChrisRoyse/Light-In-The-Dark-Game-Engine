use litd_engine::rpg::{
    AbilityActivation, AbilityCatalog, AbilityCategory, AbilityCost, AbilityDefinition,
    AbilityEffect, AbilityEffectKind, AbilityLearningState, AbilityPrerequisite,
    AbilityRequirements, AbilityStackingPolicy, AbilityTargetMode, AbilityTargeting, AbilityTiming,
    AdvancementAction, AdvancementAttribute, AdvancementInput, AdvancementPools, AdvancementRules,
    AdvancementState, AdvancementVital, AdvancementVitals, AttributeScores, RPG_ABILITY_SCHEMA,
    RaceAttributeCaps, StudyAbilityCost,
};

pub(crate) fn happy_rules() -> AdvancementRules {
    let mut rules = AdvancementRules::mud_defaults();
    rules.study_costs = vec![StudyAbilityCost {
        ability_id: "ability.cinder_bash".to_string(),
        points: 3,
    }];
    rules
}

pub(crate) fn happy_input() -> AdvancementInput {
    AdvancementInput {
        character_id: "char.ashen.cutpurse".to_string(),
        class_id: "class.thief".to_string(),
        race_id: "race.ashen_human".to_string(),
        from_level: 10,
        to_level: 12,
        attributes: AttributeScores::new(16, 14, 18, 17, 13, 11),
        race_caps: vec![RaceAttributeCaps {
            race_id: "race.ashen_human".to_string(),
            caps: AttributeScores::new(20, 20, 20, 20, 20, 20),
        }],
        ability_catalog: ability_catalog(),
        starting: AdvancementState {
            level: 10,
            attributes: AttributeScores::new(16, 14, 18, 17, 13, 11),
            pools: AdvancementPools {
                practices: 1,
                trains: 0,
                study_points: 0,
            },
            vitals: AdvancementVitals {
                hit_points: 120,
                mana: 100,
                movement: 140,
            },
            abilities: vec![learned("ability.iron_guard", 75, true, true)],
        },
        focused_ability_id: Some("ability.cinder_bash".to_string()),
        actions: vec![
            AdvancementAction::Study {
                ability_id: "ability.cinder_bash".to_string(),
            },
            AdvancementAction::Practice {
                ability_id: "ability.cinder_bash".to_string(),
                sessions: 2,
            },
            AdvancementAction::UseAbility {
                ability_id: "ability.cinder_bash".to_string(),
                successful_uses: 5,
            },
            AdvancementAction::TrainAttribute {
                attribute: AdvancementAttribute::Dexterity,
                sessions: 1,
            },
            AdvancementAction::TrainVital {
                vital: AdvancementVital::Mana,
                sessions: 1,
            },
        ],
    }
}

pub(crate) fn duplicate_learning_state() -> (AdvancementInput, AdvancementRules) {
    let mut input = happy_input();
    input
        .starting
        .abilities
        .push(learned("ability.iron_guard", 10, true, true));
    (input, happy_rules())
}

pub(crate) fn insufficient_practices() -> (AdvancementInput, AdvancementRules) {
    let mut input = happy_input();
    input.starting.pools.practices = 0;
    input.actions[1] = AdvancementAction::Practice {
        ability_id: "ability.cinder_bash".to_string(),
        sessions: 10,
    };
    (input, happy_rules())
}

pub(crate) fn train_attribute_cap_exceeded() -> (AdvancementInput, AdvancementRules) {
    let mut input = happy_input();
    input.attributes.dexterity = 20;
    input.starting.attributes.dexterity = 20;
    (input, happy_rules())
}

pub(crate) fn unknown_ability_use() -> (AdvancementInput, AdvancementRules) {
    let mut input = happy_input();
    input.actions = vec![AdvancementAction::UseAbility {
        ability_id: "ability.missing".to_string(),
        successful_uses: 1,
    }];
    (input, happy_rules())
}

pub(crate) fn study_insufficient_points() -> (AdvancementInput, AdvancementRules) {
    let input = happy_input();
    let mut rules = happy_rules();
    rules.study_costs[0].points = 10;
    (input, rules)
}

fn ability_catalog() -> AbilityCatalog {
    AbilityCatalog {
        schema: RPG_ABILITY_SCHEMA.to_string(),
        catalog_id: "catalog.issue962.advancement".to_string(),
        abilities: vec![iron_guard(), cinder_bash()],
    }
}

fn iron_guard() -> AbilityDefinition {
    let mut ability = base(
        "ability.iron_guard",
        "Iron Guard",
        AbilityActivation::Passive,
    );
    ability.targeting = target(AbilityTargetMode::None, 0, 0);
    ability.effects = vec![effect("effect.guard", AbilityEffectKind::ApplyAffect, 1, 1)];
    ability
}

fn cinder_bash() -> AbilityDefinition {
    let mut ability = base(
        "ability.cinder_bash",
        "Cinder Bash",
        AbilityActivation::Active,
    );
    ability.requirements.min_level = 8;
    ability.requirements.prerequisites = vec![AbilityPrerequisite {
        ability_id: "ability.iron_guard".to_string(),
        min_rank: 1,
    }];
    ability.costs = vec![AbilityCost {
        resource_id: "movement".to_string(),
        amount: 12,
    }];
    ability.timing.lag_ticks = 2;
    ability.targeting = target(AbilityTargetMode::Enemy, 1, 2);
    ability.effects = vec![effect("effect.bash", AbilityEffectKind::Damage, 42, 54)];
    ability
}

fn base(ability_id: &str, name: &str, activation: AbilityActivation) -> AbilityDefinition {
    AbilityDefinition {
        ability_id: ability_id.to_string(),
        display_name: name.to_string(),
        description: format!("{name} issue962 fixture."),
        kind: litd_engine::rpg::AbilityKind::Skill,
        activation,
        category: AbilityCategory {
            family_id: "class.thief".to_string(),
            school_id: Some("school.martial".to_string()),
            path_id: Some("path.cutpurse".to_string()),
            branch_id: Some("branch.control".to_string()),
            rank: 1,
            max_rank: 100,
            capstone: false,
        },
        requirements: AbilityRequirements {
            min_level: 1,
            class_ids: vec!["class.thief".to_string()],
            race_ids: Vec::new(),
            cabal_ids: Vec::new(),
            alignments: Vec::new(),
            ethos: Vec::new(),
            deity_domains: Vec::new(),
            prerequisites: Vec::new(),
        },
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
        tags: vec!["ability.issue962".to_string()],
        grants_tags: Vec::new(),
        blocks_tags: Vec::new(),
        trigger_tags: Vec::new(),
        ui_slot: None,
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

fn effect(id: &str, kind: AbilityEffectKind, min: i32, max: i32) -> AbilityEffect {
    AbilityEffect {
        effect_id: id.to_string(),
        kind,
        channel: "physical".to_string(),
        magnitude_min: min,
        magnitude_max: max,
        duration_ticks: 0,
        tick_interval: 0,
        stacking_policy: AbilityStackingPolicy::None,
        stack_cap: 0,
    }
}

fn learned(
    ability_id: &str,
    proficiency: u16,
    studied: bool,
    practiced: bool,
) -> AbilityLearningState {
    AbilityLearningState {
        ability_id: ability_id.to_string(),
        proficiency,
        studied,
        practiced,
    }
}
