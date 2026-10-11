use litd_engine::rpg::{
    ASHEN_REACH_INFERNAL_ATTRITION_POLICY_ID, InfernalAttritionPolicy, InfernalCircleId,
    InfernalCircleProfile, InfernalHazardKind, RPG_INFERNAL_ATTRITION_SCHEMA,
};

pub(crate) fn ashen_reach_infernal_attrition_policy() -> InfernalAttritionPolicy {
    InfernalAttritionPolicy {
        schema: RPG_INFERNAL_ATTRITION_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_INFERNAL_ATTRITION_POLICY_ID.to_string(),
        source_issue: 1070,
        constitution_strain_per_loss: 100,
        max_step_ticks: 240,
        max_mitigation_percent: 80,
        circles: vec![
            circle(InfernalCircleId::Threshold, InfernalHazardKind::Heat, 2, 4),
            circle(
                InfernalCircleId::EmberWastes,
                InfernalHazardKind::Heat,
                9,
                12,
            ),
            circle(
                InfernalCircleId::IronBastion,
                InfernalHazardKind::Heat,
                6,
                9,
            ),
            circle(
                InfernalCircleId::HungeringDark,
                InfernalHazardKind::DarkPressure,
                7,
                15,
            ),
            circle(
                InfernalCircleId::WailingTombs,
                InfernalHazardKind::SoulDrain,
                5,
                25,
            ),
            circle(
                InfernalCircleId::MoltenThroneRoad,
                InfernalHazardKind::Heat,
                14,
                18,
            ),
            circle(
                InfernalCircleId::ObsidianThrone,
                InfernalHazardKind::SoulDrain,
                12,
                22,
            ),
        ],
    }
}

fn circle(
    circle_id: InfernalCircleId,
    hazard: InfernalHazardKind,
    health_damage_per_tick: i32,
    constitution_strain_per_tick: i32,
) -> InfernalCircleProfile {
    InfernalCircleProfile {
        circle_id,
        hazard,
        health_damage_per_tick,
        constitution_strain_per_tick,
    }
}
