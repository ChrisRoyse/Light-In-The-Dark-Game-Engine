//! Fail-loud engine error taxonomy. No fallbacks, no silent failures: every error
//! says exactly what failed and (where useful) how to fix it.

use thiserror::Error;

#[path = "rendering_error.rs"]
pub mod rendering_error;
pub use rendering_error::RenderingError;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(
        "no compatible GPU adapter found on backends {0:?} — a Vulkan or DX12 capable GPU is \
         required; there is NO software fallback by design"
    )]
    NoAdapter(wgpu::Backends),

    #[error("failed to create a render surface for the editor window — {0}")]
    SurfaceCreate(#[from] wgpu::CreateSurfaceError),

    #[error(
        "no GPU adapter can present to the editor surface — Vulkan or DX12 presentation is \
         required; there is NO software fallback by design"
    )]
    NoSurfaceAdapter,

    #[error("failed to request a high-performance wgpu adapter — {0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error("failed to create the wgpu device/queue — {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error("editor surface has no supported swapchain configuration for {width}x{height}")]
    NoSurfaceConfig { width: u32, height: u32 },

    #[error("editor surface size is invalid ({width}x{height}); resize to a non-zero area")]
    InvalidSurfaceSize { width: u32, height: u32 },

    #[error("failed to acquire/present the current surface texture — {0}")]
    SurfaceFrame(#[from] wgpu::SurfaceError),

    #[error("engine module '{0}' is already registered")]
    DuplicateModule(String),

    #[error("cannot register engine module '{0}' after app startup")]
    ModuleRegistrationClosed(String),

    #[error("app runtime frame requested before module startup completed")]
    AppRuntimeNotStarted,

    #[error(
        "non-deterministic operation '{operation}' attempted inside deterministic hot path phase '{phase}'"
    )]
    DeterminismBoundaryViolation { phase: String, operation: String },

    #[error("audio config field {field} is invalid: {reason}")]
    AudioInvalidConfig { field: String, reason: String },

    #[error("audio backend '{backend}' unavailable: {reason}")]
    AudioBackendUnavailable { backend: String, reason: String },

    #[error("audio operation '{op}' failed: {reason}")]
    AudioOperationFailed { op: String, reason: String },

    #[error(
        "audio source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    AudioReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("frame profiler config field {field} is invalid: {reason}")]
    ProfileInvalidConfig { field: String, reason: String },

    #[error("frame profiler operation '{op}' is invalid: {reason}")]
    ProfileInvalidOperation { op: String, reason: String },

    #[error("frame profiler persistence operation '{op}' failed: {reason}")]
    ProfilePersistenceFailed { op: String, reason: String },

    #[error(
        "frame profiler source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ProfileReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("unknown fail-loud probe '{0}'")]
    UnknownFailLoudProbe(String),

    #[error("logging config field {field} is invalid: {reason}")]
    LoggingInvalidConfig { field: String, reason: String },

    #[error("animation field {field} is invalid: {reason}")]
    AnimationInvalidConfig { field: String, reason: String },

    // #1543: one structured failure when the out-of-git-tree `local-asset-vault` cannot be
    // resolved, instead of a scatter of `os error 3` import failures from every asset call site.
    #[error("{0}")]
    AssetVaultUnavailable(String),

    #[error("physics field {field} is invalid: {reason}")]
    PhysicsInvalidConfig { field: String, reason: String },

    #[error("forge compute config field {field} is invalid: {reason}")]
    ForgeInvalidConfig { field: String, reason: String },

    #[error("forge backend '{backend}' unavailable: {reason}")]
    ForgeBackendUnavailable { backend: String, reason: String },

    #[error("forge operation '{op}' failed: {source}")]
    ForgeOperationFailed {
        op: String,
        // Boxed: ForgeError is ~104 B and, embedded by value, made EngineError exactly 128 B —
        // clippy's result_large_err threshold — so every Result<_, EngineError> in the workspace
        // tripped the lint and moved a 128 B payload on its Ok path (incl. per-tick AI). Boxing
        // the embedded external error decouples EngineError's size from the Calyx error crates
        // (#1480 item 1).
        #[source]
        source: Box<calyx_forge::ForgeError>,
    },

    #[error("forge parity check '{op}' failed: {detail}")]
    ForgeParityViolation { op: String, detail: String },

    #[error(
        "forge quantization level {level} exceeded cosine-loss bound: loss={cosine_loss} bound={bound}"
    )]
    ForgeQuantizationBoundExceeded {
        level: String,
        cosine_loss: f32,
        bound: f32,
    },

    #[error("anneal config field {field} is invalid: {reason}")]
    AnnealInvalidConfig { field: String, reason: String },

    #[error("anneal operation '{op}' failed: {source}")]
    AnnealOperationFailed {
        op: String,
        // Boxed for the same reason as ForgeOperationFailed: keep EngineError's size independent
        // of the embedded Calyx error type (#1480 item 1).
        #[source]
        source: Box<calyx_core::CalyxError>,
    },

    #[error("anneal persistence operation '{op}' failed: {reason}")]
    AnnealPersistenceFailed { op: String, reason: String },

    #[error(
        "anneal source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    AnnealReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("oracle forecast config field {field} is invalid: {reason}")]
    OracleInvalidConfig { field: String, reason: String },

    #[error("oracle forecast operation '{op}' failed: {source}")]
    OracleOperationFailed {
        op: String,
        // Boxed for the same reason as ForgeOperationFailed: keep EngineError's size independent
        // of the embedded Calyx error type (#1480 item 1).
        #[source]
        source: Box<calyx_core::CalyxError>,
    },

    #[error("oracle forecast persistence operation '{op}' failed: {reason}")]
    OraclePersistenceFailed { op: String, reason: String },

    #[error(
        "oracle forecast source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    OracleReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error(
        "oracle forecast stream '{stream_id}' has insufficient history: support={support} min={min_support}"
    )]
    OracleInsufficientHistory {
        stream_id: String,
        support: usize,
        min_support: usize,
    },

    #[error(
        "oracle forecast stream '{stream_id}' is non-stationary: {direction} change at occurrence {occurrence_index}"
    )]
    OracleNonStationary {
        stream_id: String,
        direction: String,
        occurrence_index: usize,
    },

    #[error(
        "oracle what-if preview for action '{action_id}' in domain '{domain_id}' is insufficient: I_panel_oracle={i_panel_oracle} dpi_ceiling={dpi_ceiling}"
    )]
    OraclePreviewInsufficient {
        domain_id: String,
        action_id: String,
        i_panel_oracle: f32,
        dpi_ceiling: f32,
    },

    #[error("invalid AABB: min {min:?} must be <= max {max:?} on every axis")]
    InvalidAabb { min: [f32; 3], max: [f32; 3] },

    #[error("fixed-point division by zero")]
    FixedPointDivideByZero,

    #[error("fixed-point overflow during {operation}")]
    FixedPointOverflow { operation: &'static str },

    #[error("sim config field {field} is invalid: {reason}")]
    SimInvalidConfig { field: String, reason: String },

    #[error("sim body capacity exceeded: capacity is {capacity}")]
    SimCapacityExceeded { capacity: usize },

    #[error("sim order capacity exceeded: capacity is {capacity}")]
    SimOrderCapacityExceeded { capacity: usize },

    #[error(
        "sim order past due: {orders_past_due} order(s) were scheduled before execute_tick \
         {execute_tick} (earliest at {earliest_execute_tick}); a past-due order is a scheduler \
         bug and cannot be silently dropped without risking cross-machine desync"
    )]
    SimOrderPastDue {
        execute_tick: u64,
        earliest_execute_tick: u64,
        orders_past_due: usize,
    },

    #[error("sim body {id} does not exist")]
    SimBodyMissing { id: u32 },

    #[error("sim tick counter overflowed")]
    SimTickOverflow,

    #[error("nav bake config field {field} is invalid: {reason}")]
    NavInvalidConfig { field: String, reason: String },

    #[error("nav persistence operation '{op}' failed: {reason}")]
    NavPersistenceFailed { op: String, reason: String },

    #[error(
        "nav source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    NavReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("nav path from {from} to {to} was not found after {expansions} expansions")]
    NavPathNotFound {
        from: String,
        to: String,
        expansions: usize,
    },

    #[error("nav path expansion budget exceeded: max={max_expansions} expanded={expanded}")]
    NavPathBudgetExceeded {
        max_expansions: usize,
        expanded: usize,
    },

    #[error("AI runtime config field {field} is invalid: {reason}")]
    AiInvalidConfig { field: String, reason: String },

    #[error("AI runtime persistence operation '{op}' failed: {reason}")]
    AiPersistenceFailed { op: String, reason: String },

    #[error(
        "AI runtime source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    AiReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("world stream config field {field} is invalid: {reason}")]
    StreamInvalidConfig { field: String, reason: String },

    #[error("world stream persistence operation '{op}' failed: {reason}")]
    StreamPersistenceFailed { op: String, reason: String },

    #[error(
        "world stream source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    StreamReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("asset stream config field {field} is invalid: {reason}")]
    AssetStreamInvalidConfig { field: String, reason: String },

    #[error("asset stream persistence operation '{op}' failed: {reason}")]
    AssetStreamPersistenceFailed { op: String, reason: String },

    #[error(
        "asset stream source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    AssetStreamReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("ECS entity #{index}v{generation} is not alive")]
    EcsEntityNotAlive { index: u32, generation: u32 },

    #[error("ECS entity slot index capacity exceeded: slot_count={slot_count}")]
    EcsEntityIndexOverflow { slot_count: usize },

    #[error("ECS entity #{index}v{generation} cannot despawn because generation would overflow")]
    EcsEntityGenerationOverflow { index: u32, generation: u32 },

    #[error("ECS entity free-list is corrupt at index {index}: {reason}")]
    EcsFreeListCorrupt { index: u32, reason: String },

    #[error("ECS entity {entity} does not have component {component}")]
    EcsMissingComponent {
        entity: String,
        component: &'static str,
    },

    #[error("ECS bundle contains duplicate component {component}")]
    EcsDuplicateComponent { component: &'static str },

    #[error("ECS query requests component {component} more than once")]
    EcsQueryDuplicateComponent { component: &'static str },

    #[error("ECS component column storage mismatch for {component}")]
    EcsComponentStorageMismatch { component: &'static str },

    #[error("scene graph already contains entity {entity}")]
    SceneDuplicateNode { entity: String },

    #[error("scene graph does not contain entity {entity}")]
    SceneNodeMissing { entity: String },

    #[error("scene graph parent cycle: setting {parent} as parent of {child} would create a loop")]
    SceneParentCycle { child: String, parent: String },

    #[error("object requirement graph has a cycle: {cycle}")]
    ObjectRequirementCycle { cycle: String },

    #[error("reflection type {type_name} is already registered")]
    ReflectionDuplicateType { type_name: String },

    #[error("{format} serialization failed: {reason}")]
    SerializationFailed {
        format: &'static str,
        reason: String,
    },

    #[error("serialized document type mismatch: expected {expected}, found {actual}")]
    SerializedTypeMismatch { expected: String, actual: String },

    #[error("serialized document fingerprint mismatch: expected {expected}, found {actual}")]
    SerializedFingerprintMismatch { expected: String, actual: String },

    #[error("game package field {field} is invalid: {reason}")]
    GamePackageInvalidConfig { field: String, reason: String },

    #[error("RPG stats field {field} is invalid: {reason}")]
    RpgStatsInvalidConfig { field: String, reason: String },

    #[error("RPG ability field {field} is invalid: {reason}")]
    RpgAbilityInvalidConfig { field: String, reason: String },

    #[error("RPG advancement field {field} is invalid: {reason}")]
    RpgAdvancementInvalidConfig { field: String, reason: String },

    #[error("RPG combat field {field} is invalid: {reason}")]
    RpgCombatInvalidConfig { field: String, reason: String },

    #[error("RPG theme field {field} is invalid: {reason}")]
    RpgThemeInvalidConfig { field: String, reason: String },

    #[error("RPG immortal field {field} is invalid: {reason}")]
    RpgImmortalInvalidConfig { field: String, reason: String },

    #[error("RPG faction-leader field {field} is invalid: {reason}")]
    RpgFactionInvalidConfig { field: String, reason: String },

    #[error("agent model tiering field {field} is invalid: {reason}")]
    AgentModelTieringInvalidConfig { field: String, reason: String },

    #[error("post-process config field {field} is invalid: {reason}")]
    PostProcessInvalidConfig { field: String, reason: String },

    #[error("screen-space config field {field} is invalid: {reason}")]
    ScreenSpaceInvalidConfig { field: String, reason: String },

    #[error("upscale config field {field} is invalid: {reason}")]
    UpscaleInvalidConfig { field: String, reason: String },

    #[error("UI render config field {field} is invalid: {reason}")]
    UiRenderInvalidConfig { field: String, reason: String },

    #[error("UI render persistence operation '{op}' failed: {reason}")]
    UiRenderPersistenceFailed { op: String, reason: String },

    #[error(
        "UI render source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    UiRenderReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("text shaping field {field} is invalid: {reason}")]
    TextShapingInvalidConfig { field: String, reason: String },

    #[error("text shaping persistence operation '{op}' failed: {reason}")]
    TextShapingPersistenceFailed { op: String, reason: String },

    #[error(
        "text shaping source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    TextShapingReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("reference scene field {field} is invalid: {reason}")]
    ReferenceSceneInvalidConfig { field: String, reason: String },

    #[error("editor world renderer field {field} is invalid: {reason}")]
    EditorWorldRendererInvalidConfig { field: String, reason: String },

    #[error("VFX effect field {field} is invalid: {reason}")]
    VfxInvalidConfig { field: String, reason: String },

    #[error("procedural geometry field {field} is invalid: {reason}")]
    ProcGenInvalidConfig { field: String, reason: String },

    #[error("procedural geometry persistence operation '{op}' failed: {reason}")]
    ProcGenPersistenceFailed { op: String, reason: String },

    #[error(
        "procedural geometry source-of-truth readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ProcGenReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("terrain runtime field {field} is invalid: {reason}")]
    TerrainRuntimeInvalidConfig { field: String, reason: String },

    #[error("shadow config field {field} is invalid: {reason}")]
    ShadowInvalidConfig { field: String, reason: String },

    #[error("shadow atlas needs {required_tiles} tiles but only {available_tiles} are available")]
    ShadowAtlasOverflow {
        required_tiles: usize,
        available_tiles: usize,
    },

    #[error("batching config field {field} is invalid: {reason}")]
    BatchingInvalidConfig { field: String, reason: String },

    #[error("batching produced {draw_count} draw batches but the frame budget allows {max_draws}")]
    BatchingDrawLimitExceeded { draw_count: usize, max_draws: usize },

    #[error(transparent)]
    Rendering(#[from] RenderingError),
}

#[cfg(test)]
#[path = "error_size_tests.rs"]
mod issue1480_error_size;
