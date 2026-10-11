//! Deterministic lowered AI runtime.
//!
//! Editor/Calyx author behavior data. This module executes the frozen,
//! fixed-point/hashable data consumed inside the sim boundary.

mod behavior_tree;
mod behavior_tree_hash;
mod behavior_tree_store;
mod behavior_tree_validate;
mod blackboard;
mod controller;
mod controller_hash;
mod controller_store;
mod controller_validate;
mod local_dialogue;
mod local_dialogue_hash;
mod local_dialogue_types;
mod local_dialogue_validate;
mod model_tiering;
mod model_tiering_types;
mod npc_agents;
mod npc_agents_hash;
mod npc_agents_types;
mod npc_agents_validate;
mod object_micro_agents;
mod object_micro_agents_hash;
mod object_micro_agents_types;
mod object_micro_agents_validate;
mod perception;
mod perception_hash;
mod perception_spatial;
mod perception_store;
mod perception_validate;
mod perception_verify;

pub use behavior_tree::{
    BEHAVIOR_TREE_RUN_SCHEMA, BEHAVIOR_TREE_SCHEMA, BEHAVIOR_TREE_TICK_SCHEMA, BehaviorDecorator,
    BehaviorLeafState, BehaviorNode, BehaviorNodeKind, BehaviorRunReport, BehaviorStatus,
    BehaviorTickReport, BehaviorTraceEntry, BehaviorTree, tick_behavior_tree,
    verify_behavior_run_integrity, verify_behavior_tick_integrity,
};
pub use behavior_tree_hash::{
    compute_behavior_run_fnv1a64, compute_behavior_tick_fnv1a64, compute_behavior_tree_fnv1a64,
};
pub use behavior_tree_store::{
    BehaviorTreeRunFileReadback, load_behavior_run_json, write_behavior_run_json,
};
pub use blackboard::{
    AI_BLACKBOARD_SCHEMA, AgentBlackboard, BlackboardValue, compute_blackboard_fnv1a64,
    verify_blackboard_integrity,
};
pub use controller::{
    AI_CONTROLLER_RUN_SCHEMA, AiControllerAction, AiControllerActionBindings,
    AiControllerAgentInput, AiControllerDecision, AiControllerRunReport,
    lower_ai_controller_orders,
};
pub use controller_hash::compute_controller_run_fnv1a64;
pub use controller_store::{
    AiControllerRunFileReadback, load_ai_controller_run_json, write_ai_controller_run_json,
};
pub use controller_validate::verify_ai_controller_run_integrity;
pub use local_dialogue::{build_local_npc_dialogue_prompt, lower_local_npc_dialogue_response};
pub use local_dialogue_types::{
    AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY, AI_LOCAL_NPC_DIALOGUE_SCHEMA, CalyxDialogueFact,
    LocalNpcDialogueInput, LocalNpcDialogueIntent, LocalNpcDialogueLoweredOrder,
    LocalNpcDialogueModelResponse, LocalNpcDialogueReadback, LocalNpcDialogueTelemetry,
};
pub use local_dialogue_validate::validate_local_npc_dialogue_readback;
pub use model_tiering::{
    default_agent_model_registry, select_agent_model, validate_agent_model_registry,
    validate_agent_model_selection_readback,
};
pub use model_tiering_types::{
    AI_AGENT_MODEL_HOT_PATH_POLICY, AI_AGENT_MODEL_REGISTRY_SCHEMA,
    AI_AGENT_MODEL_SELECTION_SCHEMA, AgentModelBackend, AgentModelCandidateRejection,
    AgentModelHardwareProfile, AgentModelRegistry, AgentModelRole, AgentModelSelectionReadback,
    AgentModelSelectionRequest, AgentModelSpec, AgentModelTier,
};
pub use npc_agents::{
    compute_npc_agent_activation_fnv1a64, evaluate_npc_agent_activations,
    verify_npc_agent_activation_integrity,
};
pub use npc_agents_types::{
    AI_NPC_AGENT_ACTIVATION_SCHEMA, AI_NPC_AGENT_HOT_PATH_POLICY, NpcAgentActivationDecision,
    NpcAgentActivationGrade, NpcAgentActivationInput, NpcAgentActivationReadback,
    NpcAgentDefinition, NpcAgentIntent, NpcAgentRules, NpcAgentRuntimeState, NpcAgentSignalKind,
    NpcWorldSignal,
};
pub use object_micro_agents::evaluate_object_micro_agents;
pub use object_micro_agents_types::{
    AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY, AI_OBJECT_MICRO_AGENT_SCHEMA, CalyxObjectVisualSignal,
    ObjectMicroAgentActivationGrade, ObjectMicroAgentBudget, ObjectMicroAgentDecision,
    ObjectMicroAgentDefinition, ObjectMicroAgentInput, ObjectMicroAgentLoweredEffect,
    ObjectMicroAgentReadback, ObjectMicroAgentState, ObjectMicroAgentVisualIntent,
};
pub use object_micro_agents_validate::validate_object_micro_agent_readback;
pub use perception::{
    AI_PERCEPTION_RUN_SCHEMA, AI_PERCEPTION_SNAPSHOT_SCHEMA, AgentPerception, PerceivedEnemy,
    PerceptionAgent, PerceptionAgentInput, PerceptionBlocker, PerceptionEntity,
    PerceptionEntityInput, PerceptionRunReport, PerceptionSnapshot, sense_perception,
    snapshot_from_sim, snapshot_from_sim_states, verify_perception_run_integrity,
    verify_perception_snapshot_integrity,
};
pub use perception_hash::{compute_perception_run_fnv1a64, compute_snapshot_fnv1a64};
pub use perception_store::{
    PerceptionRunFileReadback, load_perception_run_json, write_perception_run_json,
};

#[cfg(test)]
mod behavior_tree_tests;
#[cfg(test)]
mod controller_tests;
#[cfg(test)]
mod local_dialogue_tests;
#[cfg(test)]
mod model_tiering_tests;
#[cfg(test)]
mod npc_agents_tests;
#[cfg(test)]
mod object_micro_agents_tests;
#[cfg(test)]
mod perception_tests;
