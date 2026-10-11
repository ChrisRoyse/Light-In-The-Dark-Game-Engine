use super::{
    AbilityActivation, AbilityCatalog, AbilityCategory, AbilityCost, AbilityDefinition,
    AbilityEffect, AbilityEffectKind, AbilityKind, AbilityPrerequisite, AbilityRequirements,
    AbilityStackingPolicy, AbilityTargetMode, AbilityTargeting, AbilityTiming, RPG_ABILITY_SCHEMA,
    UNHALLOWED_KNIGHT_CLASS_ID,
};

pub fn unhallowed_knight_ability_catalog() -> AbilityCatalog {
    AbilityCatalog {
        schema: RPG_ABILITY_SCHEMA.to_string(),
        catalog_id: "catalog.ashen_reach.unhallowed_knight.issue1062".to_string(),
        abilities: vec![
            black_oath_binding(),
            soul_cleave(),
            gravebrand_flame(),
            marrow_plague(),
            dread_standard(),
            call_cinder_fiend(),
        ],
    }
}

fn black_oath_binding() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.black_oath_binding",
        "Black Oath Binding",
        AbilityKind::Skill,
        AbilityActivation::Active,
        AbilityTargetMode::Object,
        1,
    );
    ability.costs = vec![cost("will", 30)];
    ability.timing.lag_ticks = 3;
    ability.timing.cooldown_ticks = 120;
    ability.timing.cooldown_group = Some("cooldown.soul_weapon_rite".to_string());
    ability.effects = vec![effect(
        "effect.black_oath_bind",
        AbilityEffectKind::StateTag,
        "soul_weapon_bound",
        1,
        1,
        0,
    )];
    ability.tags = ids(&["unhallowed.soul_weapon", "rite.binding"]);
    ability.ui_slot = Some("hotbar.1".to_string());
    ability
}

fn soul_cleave() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.soul_cleave",
        "Soul Cleave",
        AbilityKind::Skill,
        AbilityActivation::Active,
        AbilityTargetMode::Enemy,
        6,
    );
    ability.requirements.prerequisites = vec![prereq("ability.unhallowed.black_oath_binding")];
    ability.costs = vec![cost("movement", 18)];
    ability.timing.lag_ticks = 2;
    ability.timing.cooldown_ticks = 5;
    ability.timing.cooldown_group = Some("cooldown.soul_cleave".to_string());
    ability.effects = vec![
        effect(
            "effect.soul_cleave_damage",
            AbilityEffectKind::Damage,
            "unhallowed_blade",
            85,
            115,
            0,
        ),
        effect(
            "effect.soul_cleave_siphon_tag",
            AbilityEffectKind::StateTag,
            "trigger.soul_weapon_kill",
            1,
            1,
            0,
        ),
    ];
    ability.tags = ids(&["unhallowed.soul_weapon", "melee.finisher"]);
    ability.ui_slot = Some("hotbar.2".to_string());
    ability
}

fn gravebrand_flame() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.gravebrand_flame",
        "Gravebrand Flame",
        AbilityKind::Spell,
        AbilityActivation::Active,
        AbilityTargetMode::Enemy,
        12,
    );
    ability.costs = vec![cost("mana", 70)];
    ability.timing.cast_ticks = 1;
    ability.timing.cooldown_ticks = 8;
    ability.timing.cooldown_group = Some("cooldown.unhallowed_fire".to_string());
    ability.effects = vec![
        effect(
            "effect.gravebrand_burn",
            AbilityEffectKind::Damage,
            "infernal_fire",
            120,
            155,
            0,
        ),
        effect(
            "effect.gravebrand_mark",
            AbilityEffectKind::ApplyAffect,
            "soul_vulnerable",
            10,
            10,
            6,
        ),
    ];
    ability.tags = ids(&["unhallowed.magic", "infernal_fire"]);
    ability.ui_slot = Some("hotbar.3".to_string());
    ability
}

fn marrow_plague() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.marrow_plague",
        "Marrow Plague",
        AbilityKind::Spell,
        AbilityActivation::Active,
        AbilityTargetMode::Enemy,
        18,
    );
    ability.costs = vec![cost("mana", 95)];
    ability.timing.cast_ticks = 2;
    ability.timing.cooldown_ticks = 15;
    ability.effects = vec![effect(
        "effect.marrow_plague_rot",
        AbilityEffectKind::ApplyAffect,
        "plague",
        18,
        24,
        10,
    )];
    ability.tags = ids(&["unhallowed.magic", "plague"]);
    ability.ui_slot = Some("hotbar.4".to_string());
    ability
}

fn dread_standard() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.dread_standard",
        "Dread Standard",
        AbilityKind::Spell,
        AbilityActivation::Active,
        AbilityTargetMode::Room,
        26,
    );
    ability.costs = vec![cost("mana", 110)];
    ability.timing.round_ticks = 1;
    ability.timing.cooldown_ticks = 20;
    ability.targeting.max_targets = 16;
    ability.effects = vec![effect(
        "effect.dread_standard_aura",
        AbilityEffectKind::ApplyAffect,
        "despair_aura",
        14,
        14,
        8,
    )];
    ability.tags = ids(&["unhallowed.aura", "despair"]);
    ability.ui_slot = Some("hotbar.5".to_string());
    ability
}

fn call_cinder_fiend() -> AbilityDefinition {
    let mut ability = base(
        "ability.unhallowed.call_cinder_fiend",
        "Call Cinder Fiend",
        AbilityKind::Spell,
        AbilityActivation::Active,
        AbilityTargetMode::Room,
        34,
    );
    ability.category.capstone = true;
    ability.costs = vec![cost("mana", 160), cost("siphoned_souls", 3)];
    ability.timing.cast_ticks = 2;
    ability.timing.cooldown_ticks = 45;
    ability.timing.cooldown_group = Some("cooldown.fiend_summon".to_string());
    ability.effects = vec![effect(
        "effect.call_cinder_fiend",
        AbilityEffectKind::Summon,
        "fiend",
        1,
        1,
        12,
    )];
    ability.tags = ids(&["unhallowed.summon", "fiend"]);
    ability.ui_slot = Some("hotbar.6".to_string());
    ability
}

fn base(
    id: &str,
    name: &str,
    kind: AbilityKind,
    activation: AbilityActivation,
    mode: AbilityTargetMode,
    level: u16,
) -> AbilityDefinition {
    AbilityDefinition {
        ability_id: id.to_string(),
        display_name: name.to_string(),
        description: format!("{name} is an original Ashen Reach Unhallowed Knight ability."),
        kind,
        activation,
        category: AbilityCategory {
            family_id: UNHALLOWED_KNIGHT_CLASS_ID.to_string(),
            school_id: Some("school.unhallowed".to_string()),
            path_id: Some("path.black_oath".to_string()),
            branch_id: Some("branch.soul_weapon".to_string()),
            rank: 1,
            max_rank: 5,
            capstone: false,
        },
        requirements: AbilityRequirements {
            min_level: level,
            class_ids: ids(&[UNHALLOWED_KNIGHT_CLASS_ID]),
            race_ids: Vec::new(),
            cabal_ids: Vec::new(),
            alignments: ids(&["evil"]),
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
        targeting: target(mode),
        effects: Vec::new(),
        tags: ids(&["ability.issue1062", "class.unhallowed_knight"]),
        grants_tags: Vec::new(),
        blocks_tags: Vec::new(),
        trigger_tags: Vec::new(),
        ui_slot: None,
    }
}

fn target(mode: AbilityTargetMode) -> AbilityTargeting {
    AbilityTargeting {
        mode,
        max_targets: if mode == AbilityTargetMode::None {
            0
        } else {
            1
        },
        range_tiles: if mode == AbilityTargetMode::Object {
            0
        } else {
            3
        },
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

fn prereq(ability_id: &str) -> AbilityPrerequisite {
    AbilityPrerequisite {
        ability_id: ability_id.to_string(),
        min_rank: 1,
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
