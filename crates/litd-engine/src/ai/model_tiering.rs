use std::collections::BTreeSet;

use litd_hash::sha256_hex;
use serde::Serialize;

use crate::{EngineError, ai::model_tiering_types::*};

pub fn default_agent_model_registry() -> AgentModelRegistry {
    AgentModelRegistry {
        schema: AI_AGENT_MODEL_REGISTRY_SCHEMA.to_string(),
        registry_id: "agent-model-registry.rtx5090.local".to_string(),
        hardware: AgentModelHardwareProfile {
            hardware_id: "nvidia.rtx5090.32gb".to_string(),
            gpu_name: "NVIDIA GeForce RTX 5090".to_string(),
            vram_budget_mib: 32_768,
            runtime_reserved_mib: 8_192,
            warm_pool_budget_mib: 20_480,
        },
        models: vec![
            AgentModelSpec {
                model_id: "model.local.tiny_agent".to_string(),
                tier: AgentModelTier::TinyObject,
                role: AgentModelRole::ObjectVisual,
                backend: AgentModelBackend::LlamaCppGguf,
                quantization: "q4_k_m".to_string(),
                parameter_class: "16m-distilled".to_string(),
                artifact_ref: "local://models/agents/tiny-agent-q4.gguf".to_string(),
                weight_mib: 20,
                kv_cache_mib: 8,
                max_context_tokens: 1_024,
                max_concurrent_requests: 256,
                warm_pool_slots: 128,
                warm_start_millis: 12,
                cold_start_millis: 180,
                deterministic_lowering_only: true,
            },
            AgentModelSpec {
                model_id: "model.local.small_creature_agent".to_string(),
                tier: AgentModelTier::SmallCreature,
                role: AgentModelRole::CreatureCombat,
                backend: AgentModelBackend::LlamaCppGguf,
                quantization: "q4_k_m".to_string(),
                parameter_class: "350m-distilled".to_string(),
                artifact_ref: "local://models/agents/creature-combat-q4.gguf".to_string(),
                weight_mib: 420,
                kv_cache_mib: 96,
                max_context_tokens: 2_048,
                max_concurrent_requests: 64,
                warm_pool_slots: 24,
                warm_start_millis: 45,
                cold_start_millis: 850,
                deterministic_lowering_only: true,
            },
            AgentModelSpec {
                model_id: "model.local.larger_agent".to_string(),
                tier: AgentModelTier::NpcDialogue,
                role: AgentModelRole::NpcDialogue,
                backend: AgentModelBackend::TensorRtLlm,
                quantization: "int4_weight_only".to_string(),
                parameter_class: "gemma-class-4b".to_string(),
                artifact_ref: "local://models/agents/gemma-dialogue-4b-int4.engine".to_string(),
                weight_mib: 4_096,
                kv_cache_mib: 1_536,
                max_context_tokens: 8_192,
                max_concurrent_requests: 8,
                warm_pool_slots: 2,
                warm_start_millis: 140,
                cold_start_millis: 4_500,
                deterministic_lowering_only: true,
            },
            AgentModelSpec {
                model_id: "model.local.immortal_director".to_string(),
                tier: AgentModelTier::Director,
                role: AgentModelRole::ImmortalDirector,
                backend: AgentModelBackend::Vllm,
                quantization: "fp8_kv_int4_weight".to_string(),
                parameter_class: "gemma-class-12b".to_string(),
                artifact_ref: "local://models/agents/immortal-director-12b-fp8".to_string(),
                weight_mib: 12_288,
                kv_cache_mib: 3_072,
                max_context_tokens: 16_384,
                max_concurrent_requests: 4,
                warm_pool_slots: 1,
                warm_start_millis: 220,
                cold_start_millis: 8_000,
                deterministic_lowering_only: true,
            },
        ],
    }
}

pub fn select_agent_model(
    registry: &AgentModelRegistry,
    request: &AgentModelSelectionRequest,
) -> Result<AgentModelSelectionReadback, EngineError> {
    validate_agent_model_registry(registry)?;
    validate_selection_request(request)?;
    let mut rejected_candidates = Vec::new();
    let mut viable = Vec::new();
    for model in registry
        .models
        .iter()
        .filter(|model| model.role == request.role)
    {
        if let Some(reason) = rejection_reason(registry, request, model) {
            rejected_candidates.push(AgentModelCandidateRejection {
                model_id: model.model_id.clone(),
                reason,
            });
        } else {
            viable.push(model);
        }
    }
    if viable.is_empty() {
        return Err(invalid(
            "selection",
            format!(
                "no model fits role {} with context {}, concurrency {}, existing warm pool {} MiB; rejected={:?}",
                request.role.as_str(),
                request.required_context_tokens,
                request.concurrent_requests,
                request.existing_warm_pool_mib,
                rejected_candidates
            ),
        ));
    }
    viable.sort_by_key(|model| {
        (
            reservation_total_mib(model, request.concurrent_requests),
            model.tier.rank(),
            model.model_id.as_str(),
        )
    });
    let selected = viable[0];
    let reserved_total_mib = reservation_total_mib(selected, request.concurrent_requests);
    let warm_pool_after_mib = request
        .existing_warm_pool_mib
        .saturating_add(reserved_total_mib);
    let vram_used_mib = registry
        .hardware
        .runtime_reserved_mib
        .saturating_add(warm_pool_after_mib);
    let mut readback = AgentModelSelectionReadback {
        schema: AI_AGENT_MODEL_SELECTION_SCHEMA.to_string(),
        run_id: request.run_id.clone(),
        agent_id: request.agent_id.clone(),
        role: request.role,
        selected_model_id: selected.model_id.clone(),
        selected_tier: selected.tier,
        backend: selected.backend,
        quantization: selected.quantization.clone(),
        reserved_weight_mib: selected.weight_mib,
        reserved_kv_cache_mib: selected
            .kv_cache_mib
            .saturating_mul(u32::from(request.concurrent_requests)),
        reserved_total_mib,
        warm_pool_slots_reserved: request.concurrent_requests,
        existing_warm_pool_mib: request.existing_warm_pool_mib,
        warm_pool_after_mib,
        vram_budget_mib: registry.hardware.vram_budget_mib,
        runtime_reserved_mib: registry.hardware.runtime_reserved_mib,
        vram_headroom_mib: registry.hardware.vram_budget_mib.saturating_sub(vram_used_mib),
        candidate_count: registry
            .models
            .iter()
            .filter(|model| model.role == request.role)
            .count(),
        rejected_candidates,
        hot_path_policy: AI_AGENT_MODEL_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        selection_reason: "selected smallest role-compatible model that fits context, concurrency, warm-pool, and total VRAM budgets".to_string(),
        selection_fingerprint: String::new(),
    };
    readback.selection_fingerprint = compute_selection_fingerprint(&readback)?;
    validate_agent_model_selection_readback(&readback)?;
    Ok(readback)
}

pub fn validate_agent_model_registry(registry: &AgentModelRegistry) -> Result<(), EngineError> {
    validate_id("registry.registry_id", &registry.registry_id)?;
    if registry.schema != AI_AGENT_MODEL_REGISTRY_SCHEMA {
        return Err(invalid("registry.schema", "unsupported registry schema"));
    }
    validate_hardware(&registry.hardware)?;
    if registry.models.is_empty() {
        return Err(invalid(
            "registry.models",
            "must contain at least one model",
        ));
    }
    let mut ids = BTreeSet::new();
    for model in &registry.models {
        validate_model(model)?;
        if !ids.insert(model.model_id.as_str()) {
            return Err(invalid(
                "registry.model_id",
                format!("duplicate model id {}", model.model_id),
            ));
        }
        if model.weight_mib.saturating_add(model.kv_cache_mib)
            > registry
                .hardware
                .vram_budget_mib
                .saturating_sub(registry.hardware.runtime_reserved_mib)
        {
            return Err(invalid(
                "registry.model.vram",
                format!("{} cannot fit after runtime reservation", model.model_id),
            ));
        }
    }
    Ok(())
}

pub fn validate_agent_model_selection_readback(
    readback: &AgentModelSelectionReadback,
) -> Result<(), EngineError> {
    validate_id("selection.run_id", &readback.run_id)?;
    validate_id("selection.agent_id", &readback.agent_id)?;
    validate_id("selection.selected_model_id", &readback.selected_model_id)?;
    if readback.schema != AI_AGENT_MODEL_SELECTION_SCHEMA {
        return Err(invalid("selection.schema", "unsupported selection schema"));
    }
    if readback.calyx_live_query_allowed_in_hot_path {
        return Err(invalid(
            "selection.hot_path",
            "Calyx/model selection must not run in the sim hot path",
        ));
    }
    if readback.hot_path_policy != AI_AGENT_MODEL_HOT_PATH_POLICY {
        return Err(invalid(
            "selection.hot_path_policy",
            "unexpected hot-path policy",
        ));
    }
    if readback.reserved_total_mib
        != readback
            .reserved_weight_mib
            .saturating_add(readback.reserved_kv_cache_mib)
    {
        return Err(invalid(
            "selection.reserved_total_mib",
            "reservation mismatch",
        ));
    }
    if readback.warm_pool_after_mib
        != readback
            .existing_warm_pool_mib
            .saturating_add(readback.reserved_total_mib)
    {
        return Err(invalid(
            "selection.warm_pool_after_mib",
            "warm-pool mismatch",
        ));
    }
    let actual = compute_selection_fingerprint(readback)?;
    if readback.selection_fingerprint != actual {
        return Err(invalid(
            "selection.selection_fingerprint",
            format!(
                "fingerprint mismatch expected {} actual {actual}",
                readback.selection_fingerprint
            ),
        ));
    }
    Ok(())
}

fn rejection_reason(
    registry: &AgentModelRegistry,
    request: &AgentModelSelectionRequest,
    model: &AgentModelSpec,
) -> Option<String> {
    if request.required_context_tokens > model.max_context_tokens {
        return Some(format!(
            "context {} exceeds model max {}",
            request.required_context_tokens, model.max_context_tokens
        ));
    }
    if request.concurrent_requests > model.max_concurrent_requests {
        return Some(format!(
            "concurrency {} exceeds model max {}",
            request.concurrent_requests, model.max_concurrent_requests
        ));
    }
    if request.prefer_warm_pool && request.concurrent_requests > model.warm_pool_slots {
        return Some(format!(
            "warm slots {} exceed model warm pool slots {}",
            request.concurrent_requests, model.warm_pool_slots
        ));
    }
    if !model.deterministic_lowering_only {
        return Some("model allows non-deterministic hot-path effects".to_string());
    }
    let reserved = reservation_total_mib(model, request.concurrent_requests);
    let warm_after = request.existing_warm_pool_mib.saturating_add(reserved);
    if warm_after > registry.hardware.warm_pool_budget_mib {
        return Some(format!(
            "warm pool {} MiB exceeds budget {} MiB",
            warm_after, registry.hardware.warm_pool_budget_mib
        ));
    }
    if registry
        .hardware
        .runtime_reserved_mib
        .saturating_add(warm_after)
        > registry.hardware.vram_budget_mib
    {
        return Some(format!(
            "runtime plus warm pool {} MiB exceeds VRAM budget {} MiB",
            registry
                .hardware
                .runtime_reserved_mib
                .saturating_add(warm_after),
            registry.hardware.vram_budget_mib
        ));
    }
    None
}

fn validate_selection_request(request: &AgentModelSelectionRequest) -> Result<(), EngineError> {
    validate_id("selection.run_id", &request.run_id)?;
    validate_id("selection.agent_id", &request.agent_id)?;
    if request.required_context_tokens == 0 {
        return Err(invalid(
            "selection.required_context_tokens",
            "context token budget must be positive",
        ));
    }
    if request.concurrent_requests == 0 {
        return Err(invalid(
            "selection.concurrent_requests",
            "concurrency must be positive",
        ));
    }
    Ok(())
}

fn validate_hardware(hardware: &AgentModelHardwareProfile) -> Result<(), EngineError> {
    validate_id("hardware.hardware_id", &hardware.hardware_id)?;
    validate_text("hardware.gpu_name", &hardware.gpu_name)?;
    if hardware.vram_budget_mib == 0 {
        return Err(invalid("hardware.vram_budget_mib", "must be positive"));
    }
    if hardware.runtime_reserved_mib >= hardware.vram_budget_mib {
        return Err(invalid(
            "hardware.runtime_reserved_mib",
            "runtime reservation must be below total VRAM",
        ));
    }
    if hardware.warm_pool_budget_mib == 0
        || hardware.warm_pool_budget_mib > hardware.vram_budget_mib - hardware.runtime_reserved_mib
    {
        return Err(invalid(
            "hardware.warm_pool_budget_mib",
            "warm-pool budget must fit after runtime reservation",
        ));
    }
    Ok(())
}

fn validate_model(model: &AgentModelSpec) -> Result<(), EngineError> {
    validate_id("model.model_id", &model.model_id)?;
    validate_text("model.quantization", &model.quantization)?;
    validate_text("model.parameter_class", &model.parameter_class)?;
    validate_ref("model.artifact_ref", &model.artifact_ref)?;
    if model.weight_mib == 0 || model.kv_cache_mib == 0 {
        return Err(invalid(
            "model.memory",
            "weight and KV-cache reservations must be positive",
        ));
    }
    if model.max_context_tokens == 0 || model.max_concurrent_requests == 0 {
        return Err(invalid(
            "model.capacity",
            "context and concurrency limits must be positive",
        ));
    }
    if model.warm_pool_slots == 0 || model.warm_pool_slots > model.max_concurrent_requests {
        return Err(invalid(
            "model.warm_pool_slots",
            "warm-pool slots must be positive and not exceed concurrency",
        ));
    }
    if !model.deterministic_lowering_only {
        return Err(invalid(
            "model.deterministic_lowering_only",
            "agent model outputs must lower to deterministic orders only",
        ));
    }
    Ok(())
}

fn reservation_total_mib(model: &AgentModelSpec, concurrency: u16) -> u32 {
    model
        .weight_mib
        .saturating_add(model.kv_cache_mib.saturating_mul(u32::from(concurrency)))
}

fn compute_selection_fingerprint(
    readback: &AgentModelSelectionReadback,
) -> Result<String, EngineError> {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        schema: &'a str,
        run_id: &'a str,
        agent_id: &'a str,
        role: AgentModelRole,
        selected_model_id: &'a str,
        selected_tier: AgentModelTier,
        backend: AgentModelBackend,
        quantization: &'a str,
        reserved_total_mib: u32,
        warm_pool_after_mib: u32,
        vram_budget_mib: u32,
        hot_path_policy: &'a str,
        calyx_live_query_allowed_in_hot_path: bool,
    }
    serde_json::to_vec(&Fingerprint {
        schema: &readback.schema,
        run_id: &readback.run_id,
        agent_id: &readback.agent_id,
        role: readback.role,
        selected_model_id: &readback.selected_model_id,
        selected_tier: readback.selected_tier,
        backend: readback.backend,
        quantization: &readback.quantization,
        reserved_total_mib: readback.reserved_total_mib,
        warm_pool_after_mib: readback.warm_pool_after_mib,
        vram_budget_mib: readback.vram_budget_mib,
        hot_path_policy: &readback.hot_path_policy,
        calyx_live_query_allowed_in_hot_path: readback.calyx_live_query_allowed_in_hot_path,
    })
    .map(|bytes| sha256_hex(&bytes))
    .map_err(|error| EngineError::SerializationFailed {
        format: "agent-model-selection-fingerprint-json",
        reason: error.to_string(),
    })
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(invalid(field, "must be non-empty"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(invalid(
            field,
            format!("{value} contains invalid characters"),
        ));
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(invalid(field, "must be non-empty"));
    }
    Ok(())
}

fn validate_ref(field: &str, value: &str) -> Result<(), EngineError> {
    validate_text(field, value)?;
    if !(value.starts_with("local://") || value.starts_with("model://")) {
        return Err(invalid(field, "must be a local:// or model:// ref"));
    }
    Ok(())
}

fn invalid(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::AgentModelTieringInvalidConfig {
        field: format!("agent_model.{field}"),
        reason: reason.into(),
    }
}
