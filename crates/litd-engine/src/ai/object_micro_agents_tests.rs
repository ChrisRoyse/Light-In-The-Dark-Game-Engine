use crate::{
    EngineError,
    ai::{
        AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY, CalyxObjectVisualSignal,
        ObjectMicroAgentActivationGrade, ObjectMicroAgentBudget, ObjectMicroAgentDefinition,
        ObjectMicroAgentInput, ObjectMicroAgentState, ObjectMicroAgentVisualIntent,
        evaluate_object_micro_agents, validate_object_micro_agent_readback,
    },
    sim::SimBodyId,
};

const MODEL_BYTES: u64 = 5 * 1024 * 1024;
const MODEL_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn issue971_lowers_hundreds_of_object_micro_agents() {
    let input = input_fixture(256, 256, 4096);
    let readback = evaluate_object_micro_agents(&input).unwrap();

    assert_eq!(readback.agent_count, 256);
    assert_eq!(readback.accepted_count, 256);
    assert_eq!(readback.visual_effect_count, 256);
    assert_eq!(readback.spawnable_object_count, 256);
    assert_eq!(readback.total_particles_reserved, 2048);
    assert_eq!(readback.total_embedded_model_bytes, 256 * MODEL_BYTES);
    assert_eq!(
        readback.hot_path_policy,
        AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY
    );
    assert!(!readback.calyx_live_query_allowed_in_hot_path);
    assert!(
        readback
            .lowered_effects
            .iter()
            .all(|effect| effect.deterministic
                && effect.source == "lowered_object_micro_agent"
                && effect.grounding_refs_contain("calyx://signal/object"))
    );
    validate_object_micro_agent_readback(&readback).unwrap();
}

#[test]
fn issue971_edges_fail_closed_for_bad_models_ward_budget_and_tamper() {
    let mut small_model = input_fixture(1, 1, 64);
    small_model.definitions[0].embedded_model_bytes = MODEL_BYTES - 1;
    expect_invalid(
        evaluate_object_micro_agents(&small_model),
        "definition.embedded_model_bytes",
        "5-20 MiB",
    );

    let mut duplicate = input_fixture(2, 2, 64);
    duplicate.definitions[1].agent_id = duplicate.definitions[0].agent_id.clone();
    expect_invalid(
        evaluate_object_micro_agents(&duplicate),
        "input.agent_id",
        "unique",
    );

    let mut ward = input_fixture(1, 1, 64);
    ward.signals[0].tags.push("nuclear".to_string());
    let ward_readback = evaluate_object_micro_agents(&ward).unwrap();
    assert_eq!(ward_readback.accepted_count, 0);
    assert_eq!(
        ward_readback.decisions[0].grade,
        ObjectMicroAgentActivationGrade::WardBlocked
    );

    let budget = input_fixture(4, 4, 16);
    let budget_readback = evaluate_object_micro_agents(&budget).unwrap();
    assert_eq!(budget_readback.accepted_count, 2);
    assert_eq!(
        budget_readback.decisions[2].grade,
        ObjectMicroAgentActivationGrade::ParticleBudgetExceeded
    );

    let mut tampered = evaluate_object_micro_agents(&input_fixture(1, 1, 64)).unwrap();
    tampered.readback_fingerprint = "0000".to_string();
    match validate_object_micro_agent_readback(&tampered) {
        Err(EngineError::AiReadbackMismatch { field, .. }) => {
            assert_eq!(field, "object_micro_agent.readback_fingerprint");
        }
        other => panic!("expected readback mismatch, got {other:?}"),
    }
}

fn input_fixture(
    definition_count: usize,
    signal_count: usize,
    max_particles: u32,
) -> ObjectMicroAgentInput {
    let mut budget = ObjectMicroAgentBudget::hundreds_spawnable_defaults();
    budget.max_particles_per_tick = max_particles;
    ObjectMicroAgentInput {
        run_id: "issue971-test".to_string(),
        world_id: "world.ashen_reach".to_string(),
        budget,
        definitions: (0..definition_count).map(definition).collect(),
        states: (0..definition_count)
            .map(|index| ObjectMicroAgentState {
                agent_id: format!("object.agent.{index:03}"),
                spawned_instances: 0,
                last_activation_tick: None,
            })
            .collect(),
        signals: (0..signal_count).map(signal).collect(),
    }
}

fn definition(index: usize) -> ObjectMicroAgentDefinition {
    ObjectMicroAgentDefinition {
        agent_id: format!("object.agent.{index:03}"),
        object_id: format!("object.lantern.{index:03}"),
        display_name: format!("Lantern {index:03}"),
        embedded_model_id: "model.embedded.lantern_glow_5mb".to_string(),
        embedded_model_bytes: MODEL_BYTES,
        embedded_model_sha256: MODEL_SHA.to_string(),
        target_body: SimBodyId::from_raw(index as u32),
        trigger_tags: vec!["lantern".to_string(), "proximity".to_string()],
        ward_tags: vec!["lantern".to_string(), "proximity".to_string()],
        visual_intent: ObjectMicroAgentVisualIntent::Glow,
        vfx_effect_id: "vfx.micro.lantern_glow".to_string(),
        material_variant: "mat.lantern.awake".to_string(),
        particle_budget: 8,
    }
}

fn signal(index: usize) -> CalyxObjectVisualSignal {
    CalyxObjectVisualSignal {
        signal_id: format!("signal.object.{index:03}"),
        world_id: "world.ashen_reach".to_string(),
        tick: index as u64,
        strength_milli: 900,
        tags: vec!["lantern".to_string(), "proximity".to_string()],
        summary: "player proximity wakes lantern object".to_string(),
        target_object_id: Some(format!("object.lantern.{index:03}")),
        provenance_refs: vec![format!("calyx://signal/object/{index:03}")],
    }
}

trait EffectRefs {
    fn grounding_refs_contain(&self, expected_prefix: &str) -> bool;
}

impl EffectRefs for crate::ai::ObjectMicroAgentLoweredEffect {
    fn grounding_refs_contain(&self, expected_prefix: &str) -> bool {
        self.provenance_refs
            .iter()
            .any(|reference| reference.starts_with(expected_prefix))
    }
}

fn expect_invalid<T: std::fmt::Debug>(result: Result<T, EngineError>, field: &str, reason: &str) {
    match result {
        Err(EngineError::AiInvalidConfig {
            field: actual_field,
            reason: actual_reason,
        }) => {
            assert_eq!(actual_field, format!("object_micro_agent.{field}"));
            assert!(
                actual_reason.contains(reason),
                "expected reason containing {reason}, got {actual_reason}"
            );
        }
        other => panic!("expected AiInvalidConfig, got {other:?}"),
    }
}
