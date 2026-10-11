use litd_engine::rpg::{
    ASHEN_REACH_MORTALITY_POLICY_ID, MortalityPolicy, RPG_MORTALITY_SCHEMA, RaceLifespan,
};

pub(crate) fn ashen_reach_mortality_policy() -> MortalityPolicy {
    MortalityPolicy {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_MORTALITY_POLICY_ID.to_string(),
        source_issue: 1060,
        deaths_per_constitution_loss: 3,
        permanent_death_constitution_threshold: 3,
        constitution_hit_points: 8,
        constitution_regen_per_tick: 2,
        death_threshold_base: 12,
        death_threshold_per_constitution: 2,
        mourning_wall_required: true,
        race_lifespans: vec![
            RaceLifespan::new("race.ashen_human", "Human", 16, 72, 95),
            RaceLifespan::new("race.ashen_elf", "Elf", 40, 420, 560),
            RaceLifespan::new("race.ashen_half_elf", "Half-Elf", 20, 165, 230),
            RaceLifespan::new("race.ashen_dwarf", "Dwarf", 35, 220, 310),
            RaceLifespan::new("race.ashen_orc", "Orc", 14, 58, 82),
            RaceLifespan::new("race.ashen_gnome", "Gnome", 22, 170, 240),
            RaceLifespan::new("race.ashen_goblin", "Goblin", 12, 52, 74),
            RaceLifespan::new("race.sylphkin", "Sylphkin", 18, 120, 170),
            RaceLifespan::new("race.felisar", "Felisar", 15, 86, 120),
            RaceLifespan::new("race.ashkin", "Ashkin", 35, 320, 470),
            RaceLifespan::new("race.saur", "Saur", 18, 135, 190),
        ],
    }
}
