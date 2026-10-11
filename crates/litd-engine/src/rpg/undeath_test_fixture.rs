use litd_engine::rpg::{
    ASHEN_REACH_UNDEATH_POLICY_ID, RPG_UNDEATH_SCHEMA, UndeadForm, UndeadFormSpec, UndeathPolicy,
};

pub(crate) fn ashen_reach_undeath_policy() -> UndeathPolicy {
    UndeathPolicy {
        schema: RPG_UNDEATH_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_UNDEATH_POLICY_ID.to_string(),
        source_issue: 1061,
        minimum_caster_level: 47,
        fixed_constitution_after_withering: 6,
        aging_death_immune: true,
        hunger_thirst_immune: true,
        physical_kill_only: true,
        irreversible: true,
        normal_resurrection_forbidden: true,
        allowed_dark_caster_class_ids: vec!["class.necromancer".to_string()],
        required_discovery_ref_count: 2,
        required_components: vec![
            "component.name_ash".to_string(),
            "component.black_candle".to_string(),
            "component.last_breath_salt".to_string(),
        ],
        forms: vec![
            UndeadFormSpec::new(
                UndeadForm::Lich,
                "focus.vessel_of_last_breath",
                "ritual.vessel_siphon",
                "Vessel Siphon",
                "Steals life-force into a hidden vessel instead of restoring life.",
            ),
            UndeadFormSpec::new(
                UndeadForm::Mummy,
                "focus.grave_wrappings",
                "ritual.flesh_memory_theft",
                "Flesh-Memory Theft",
                "Cuts knowledge from a living body and seals it into dead flesh.",
            ),
            UndeadFormSpec::new(
                UndeadForm::Wraith,
                "focus.veil_anchor",
                "ritual.thin_veil_stride",
                "Thin-Veil Stride",
                "Crosses short distances as a half-real shadow at visible risk.",
            ),
        ],
    }
}
