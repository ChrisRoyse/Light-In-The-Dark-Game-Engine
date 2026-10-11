//! Human-facing remediation guidance for every [`EngineError`] variant (#1543 origin,
//! extended by the #1466 diagnostics split).
//!
//! Each arm answers "what do I do about it?" in the fail-loud style: reject the bad input,
//! repair the source of truth, and never silently fall back. A second inherent `impl` block
//! on `EngineError`, called from `diagnostics.rs::EngineError::report`; the machine-readable
//! codes live in `diagnostics_codes.rs`.

use crate::{EngineError, error::RenderingError};

impl EngineError {
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::NoAdapter(_) => {
                "Install or enable a Vulkan/DX12-capable GPU/driver; software fallback is disabled."
            }
            Self::SurfaceCreate(_) => {
                "Create the renderer only after a live native window exists, then retry surface creation."
            }
            Self::NoSurfaceAdapter => {
                "Use an adapter that can present to the editor surface; update GPU drivers if none exists."
            }
            Self::RequestAdapter(_) => {
                "Check that a high-performance Vulkan/DX12 adapter is available for the requested surface; update GPU drivers if adapter discovery fails."
            }
            Self::RequestDevice(_) => {
                "Check requested wgpu features/limits against the selected adapter and driver."
            }
            Self::NoSurfaceConfig { .. } => {
                "Resize the window to a supported non-zero client area and re-query surface capabilities."
            }
            Self::InvalidSurfaceSize { .. } => {
                "Ignore zero-size minimize events; configure the surface only after width and height are non-zero."
            }
            Self::SurfaceFrame(_) => {
                "Reconfigure the surface on Lost/Outdated; otherwise stop the frame and report the surface error."
            }
            Self::DuplicateModule(_) => {
                "Register each engine module once with a unique stable name before startup."
            }
            Self::ModuleRegistrationClosed(_) => {
                "Move module registration before AppRuntime::start; runtime module sets are immutable while running."
            }
            Self::AppRuntimeNotStarted => {
                "Call AppRuntime::start successfully before requesting frames."
            }
            Self::DeterminismBoundaryViolation { .. } => {
                "Move the non-deterministic operation out of the hashed hot path, then pass a fingerprinted lowered artifact or fixed-point deterministic result into sim/render."
            }
            Self::AudioInvalidConfig { .. } => {
                "Reject the audio request, then provide a known bus, finite volume/pitch values, bounded tween durations, and readable local audio files."
            }
            Self::AudioBackendUnavailable { .. } => {
                "Fix the OS audio device/backend before starting runtime audio; do not continue with silent or mocked playback."
            }
            Self::AudioOperationFailed { .. } => {
                "Inspect the Kira operation, audio file path, bus capacity, and structured error reason; fail the runtime action rather than falling back."
            }
            Self::AudioReadbackMismatch { .. } => {
                "Stop using the mixer snapshot, reopen the persisted source-of-truth JSON, and reconcile it with the expected runtime audio state."
            }
            Self::ProfileInvalidConfig { .. } => {
                "Use finite non-negative timestamps, a positive GPU timestamp period, and non-empty profiler labels before capturing a frame."
            }
            Self::ProfileInvalidOperation { .. } => {
                "Keep profiler spans balanced and LIFO-scoped, then finish the frame only after every active span has ended."
            }
            Self::ProfilePersistenceFailed { .. } => {
                "Write the profiler JSON to a local writable path, then read and decode that same file before trusting the capture."
            }
            Self::ProfileReadbackMismatch { .. } => {
                "Stop using the profile export, reopen the JSON source of truth, and compare it against the in-memory frame profile."
            }
            Self::UnknownFailLoudProbe(_) => {
                "Use one of: frame-before-start, duplicate-module, registration-after-start."
            }
            Self::LoggingInvalidConfig { .. } => {
                "Use a writable local log path, create its parent directory, and initialize tracing exactly once at process startup."
            }
            Self::AnimationInvalidConfig { .. } => {
                "Reject the animation asset or sample request, then reload real glTF skin, clip, joint, keyframe, and palette data from source of truth before submitting it to the renderer."
            }
            Self::AssetVaultUnavailable(_) => {
                "Point LITD_LOCAL_ASSET_VAULT at the local-asset-vault directory. It is a large gitignored directory that exists only beside the PRIMARY checkout, so linked git worktrees do not get a copy; the error text lists every path that was searched."
            }
            Self::PhysicsInvalidConfig { .. } => {
                "Reject the physics request, then provide finite Rapier body, collider, joint, raycast, and scene-query inputs before stepping or querying the world."
            }
            Self::ForgeInvalidConfig { .. } => {
                "Use explicit finite Forge tolerances, a supported quantization level, and a declared backend."
            }
            Self::ForgeBackendUnavailable { .. } => {
                "Build with the required Forge backend feature and verify the CUDA driver/device before running the engine compute path; do not use CPU fallback."
            }
            Self::ForgeOperationFailed { .. } => {
                "Inspect the source Calyx Forge error code/remediation and fix the failing backend, shape, VRAM, or quantization source of truth."
            }
            Self::ForgeParityViolation { .. } => {
                "Stop the compute path, inspect the persisted CPU/backend outputs, and widen tolerances only with documented numerical evidence."
            }
            Self::ForgeQuantizationBoundExceeded { .. } => {
                "Reject the quantized payload or choose a gentler quantization level after verifying decoded cosine loss against real data."
            }
            Self::AnnealInvalidConfig { .. } => {
                "Use a writable Anneal vault, finite metrics, valid engine tuning params, and matching candidate/incumbent target ids."
            }
            Self::AnnealOperationFailed { .. } => {
                "Inspect the source Calyx error code/remediation and repair the Anneal vault, rollback CF, tripwire config, budget, or ledger chain."
            }
            Self::AnnealPersistenceFailed { .. } => {
                "Repair filesystem permissions or corrupt JSON under the Anneal vault before rerunning the tuning pass."
            }
            Self::AnnealReadbackMismatch { .. } => {
                "Stop promotion, read the Anneal rollback CF and config JSON files, and reconcile the live pointer with the expected config hash."
            }
            Self::OracleInvalidConfig { .. } => {
                "Use a writable Oracle vault, valid stream ids, finite observations, and bounded forecast thresholds."
            }
            Self::OracleOperationFailed { .. } => {
                "Inspect the source Calyx error code/remediation and repair the Aster recurrence rows, Oracle time predictor, Loom readback, or Assay hazard/change-point input."
            }
            Self::OraclePersistenceFailed { .. } => {
                "Repair filesystem permissions or corrupt forecast JSON/CF rows under the Oracle vault before rerunning the forecast."
            }
            Self::OracleReadbackMismatch { .. } => {
                "Stop using the forecast, read the Aster Online/Recurrence/Ledger CF rows, and reconcile the persisted record hash with the ledger payload."
            }
            Self::OracleInsufficientHistory { .. } => {
                "Ingest enough real recurrence observations for the configured minimum before requesting a live-world forecast."
            }
            Self::OracleNonStationary { .. } => {
                "Treat the old cadence as invalid, split the stream at the reported change point, then forecast only the new stable regime."
            }
            Self::OraclePreviewInsufficient { .. } => {
                "Do not guess. Add grounded Assay sufficiency rows and recurrence evidence for the domain/action before generating a what-if preview."
            }
            Self::InvalidAabb { .. } => {
                "Construct AABBs with min <= max on every axis, or use from_center_half_extents."
            }
            Self::FixedPointDivideByZero => {
                "Check denominators and divisor raw values before fixed-point division."
            }
            Self::FixedPointOverflow { .. } => {
                "Reduce operand magnitude or split the deterministic fixed-point calculation into bounded steps."
            }
            Self::SimInvalidConfig { .. } => {
                "Use a non-zero fixed tick rate and keep deterministic sim configuration inside bounded integer ranges."
            }
            Self::SimCapacityExceeded { .. } => {
                "Raise the fixed sim capacity before startup or reject the spawn request before entering the tick loop."
            }
            Self::SimOrderCapacityExceeded { .. } => {
                "Raise the fixed order capacity for the lockstep frame or split the authored input into more deterministic ticks."
            }
            Self::SimOrderPastDue { .. } => {
                "Submit sim orders at or after the next execute_tick (net_authority enforces input_delay_ticks); a past-due order signals a scheduler bug — stall, roll back, or drop the offending peer rather than dropping the order and desyncing peers."
            }
            Self::SimBodyMissing { .. } => {
                "Reject orders for stale or unknown sim bodies before the deterministic frame is executed."
            }
            Self::SimTickOverflow => {
                "End the deterministic run before u64::MAX ticks and persist/restart from a verified snapshot."
            }
            Self::NavInvalidConfig { .. } => {
                "Reject the bake input, then regenerate effective per-cell pathing, fixed-point heights, rectangular terrain coverage, and static blocker bounds from editor source of truth."
            }
            Self::NavPersistenceFailed { .. } => {
                "Write the baked navmesh JSON to a local writable path, then read and decode that same file before handing it to sim/pathfinding."
            }
            Self::NavReadbackMismatch { .. } => {
                "Stop using the navmesh, reopen the serialized source of truth, recompute its FNV-1a hash and counts, and rebake from verified terrain/pathing input."
            }
            Self::NavPathNotFound { .. } => {
                "Reject the movement order, inspect the blocked cells/source navmesh hash, and rebake or choose a reachable goal before issuing sim orders."
            }
            Self::NavPathBudgetExceeded { .. } => {
                "Do not continue inside the hot tick; split the path query across frames or raise the explicit expansion budget after profiling."
            }
            Self::AiInvalidConfig { .. } => {
                "Reject the lowered AI data, then regenerate behavior nodes, perception inputs, blackboard keys, and leaf bindings from editor/Calyx source of truth."
            }
            Self::AiPersistenceFailed { .. } => {
                "Write the AI runtime JSON to a local writable path, then read and decode that same file before handing statuses or sensed facts to the sim controller."
            }
            Self::AiReadbackMismatch { .. } => {
                "Stop using the AI runtime artifact, reopen the serialized source of truth, recompute FNV-1a hashes, and regenerate from verified lowered AI data."
            }
            Self::StreamInvalidConfig { .. } => {
                "Reject the stream request, then provide a verified Aster world record, finite in-bounds streaming anchor, positive spatial chunk size, and bounded load/unload radii."
            }
            Self::StreamPersistenceFailed { .. } => {
                "Keep world streaming off the hot tick, repair the Aster vault/manifest/chunk rows, and retry only after source-of-truth rows read back correctly."
            }
            Self::StreamReadbackMismatch { .. } => {
                "Stop using the streamed world, reopen the Aster manifest/chunk source of truth, recompute hashes and active chunk state, and reconcile the mismatch before entering the tick."
            }
            Self::AssetStreamInvalidConfig { .. } => {
                "Reject the asset stream request, then provide a real editor asset_lod or texture-mip product, complete consecutive distance bands, finite fixed-point distances, and readable source files."
            }
            Self::AssetStreamPersistenceFailed { .. } => {
                "Keep asset bytes off the hot tick, repair the asset_lod/texture-mip manifest or source file, and retry only after file bytes read back with matching hashes."
            }
            Self::AssetStreamReadbackMismatch { .. } => {
                "Stop using the streamed asset, reopen the asset manifest and selected source files, recompute SHA-256 and FNV-1a hashes, and reconcile the mismatch before render submission."
            }
            Self::EcsEntityNotAlive { .. } => {
                "Do not use stale entity handles after despawn; resolve a fresh live entity handle first."
            }
            Self::EcsEntityIndexOverflow { .. } => {
                "Stop spawning entities in this world, persist a verified snapshot, and shard or compact the session before u32 entity indices are exhausted."
            }
            Self::EcsEntityGenerationOverflow { .. } => {
                "Stop recycling this entity slot; persist and restart from a verified snapshot before a stale handle can become valid again."
            }
            Self::EcsFreeListCorrupt { .. } => {
                "Treat this as an engine bug: inspect the ECS slot/free-list source of truth and reject the spawn before reusing a live or out-of-range slot."
            }
            Self::EcsMissingComponent { .. } => {
                "Spawn the entity with the component, or query only archetypes that contain it."
            }
            Self::EcsDuplicateComponent { .. } => {
                "Build bundles with one value per component type; split duplicate state into distinct component types."
            }
            Self::EcsQueryDuplicateComponent { .. } => {
                "Use a single-component query or different component types to avoid aliasing the same column."
            }
            Self::EcsComponentStorageMismatch { .. } => {
                "Treat this as an engine bug: component TypeId and column storage must be created together."
            }
            Self::SceneDuplicateNode { .. } => {
                "Insert each live entity into the scene graph once, then mutate its existing node."
            }
            Self::SceneNodeMissing { .. } => {
                "Insert the entity into the scene graph before parenting, transforming, or reading it."
            }
            Self::SceneParentCycle { .. } => {
                "Choose a parent that is not the child itself or one of the child's descendants."
            }
            Self::ObjectRequirementCycle { .. } => {
                "Break the requirement cycle named in the error: a tech cannot transitively require itself."
            }
            Self::ReflectionDuplicateType { .. } => {
                "Register each reflected type once with a stable fully-qualified type name."
            }
            Self::SerializationFailed { .. } => {
                "Fix the reflected type's serde implementation or reject the malformed serialized document."
            }
            Self::SerializedTypeMismatch { .. } => {
                "Deserialize the document as the type named in its header, or reject it before loading."
            }
            Self::SerializedFingerprintMismatch { .. } => {
                "Reject the document and reload from a trusted source; serialized payload bytes changed."
            }
            Self::GamePackageInvalidConfig { .. } => {
                "Reject the package, verify the .litdgame path, LitD runtime id, lock-in policy, manifest fingerprint, and asset hashes, then rebuild through the editor package tool."
            }
            Self::RpgStatsInvalidConfig { .. }
            | Self::RpgAbilityInvalidConfig { .. }
            | Self::RpgAdvancementInvalidConfig { .. }
            | Self::RpgCombatInvalidConfig { .. }
            | Self::RpgThemeInvalidConfig { .. }
            | Self::RpgImmortalInvalidConfig { .. }
            | Self::RpgFactionInvalidConfig { .. } => {
                "Reject invalid RPG data, then provide valid stats/abilities, gates, formulas, prerequisites, costs, timing, targets, and effects before runtime/editor/MCP use."
            }
            Self::AgentModelTieringInvalidConfig { .. } => {
                "Fix the local agent model registry, role mapping, context budget, concurrency, warm-pool reservation, or VRAM budget before selecting an agent model."
            }
            Self::Rendering(RenderingError::PbrMaterialInvalidFactor { .. }) => {
                "Clamp authoring UI controls to the valid PBR range before submitting material data."
            }
            Self::Rendering(RenderingError::PbrInvalidVector { .. }) => {
                "Normalize non-zero lighting vectors before evaluating or uploading PBR shading data."
            }
            Self::Rendering(RenderingError::ClusteredLightingInvalidConfig { .. }) => {
                "Use positive cluster dimensions, finite view bounds, valid light radii, and normalized non-zero light directions."
            }
            Self::Rendering(RenderingError::ClusteredLightingClusterOverflow { .. }) => {
                "Increase max_lights_per_cluster, reduce overlapping dynamic lights, or split the view into more clusters."
            }
            Self::Rendering(RenderingError::ClusteredLightingGpuReadbackMismatch { .. }) => {
                "Stop using the frame, inspect the persisted GPU readback buffers and compare them to the CPU clustered-light plan."
            }
            Self::ShadowInvalidConfig { .. } => {
                "Use finite camera/light vectors, valid cascade settings, and atlas tile sizes that divide the atlas."
            }
            Self::ShadowAtlasOverflow { .. } => {
                "Increase the shadow atlas size, reduce tile size, reduce cascade count, or disable shadows on lower-priority lights."
            }
            Self::PostProcessInvalidConfig { .. } => {
                "Use finite HDR exposure, valid bloom settings, and color-grade/LUT controls inside their authored ranges."
            }
            Self::ScreenSpaceInvalidConfig { .. } => {
                "Use valid SSAO kernels, supported MSAA sample counts, finite TAA settings, and bounded SSR controls."
            }
            Self::UpscaleInvalidConfig { .. } => {
                "Select a supported FSR quality mode, keep sharpness/feedback/disocclusion inside 0..=1, and match the input render resolution to the mode's scale factor."
            }
            Self::BatchingInvalidConfig { .. } => {
                "Submit finite instance bounds, normalized screen rectangles, valid HZB depths, and non-empty mesh/material keys."
            }
            Self::BatchingDrawLimitExceeded { .. } => {
                "Reduce material/mesh fragmentation, raise the draw batch budget, or merge compatible renderer batches."
            }
            Self::Rendering(RenderingError::ShaderInvalidConfig { .. }) => {
                "Provide non-empty WGSL source, valid entry points, bounded variant axes, and readable shader file paths."
            }
            Self::Rendering(RenderingError::ShaderBindingValidationFailed { .. }) => {
                "Keep WGSL @group/@binding declarations, resource kinds, and pipeline layout declarations in lockstep."
            }
            Self::Rendering(RenderingError::StylizedShaderInvalidConfig { .. }) => {
                "Use known stylized presets, complete tweak parameters, finite values, and values inside each preset range."
            }
            Self::Rendering(RenderingError::MaterialCatalogInvalidConfig { .. }) => {
                "Submit unique catalog record ids, valid shader variant metadata, finite material parameters, and unique texture slots before building reuse groups."
            }
            Self::Rendering(RenderingError::CameraInvalidConfig { .. }) => {
                "Use finite camera values, non-zero viewport dimensions, valid clipping planes, and ordered world/cinematic bounds."
            }
            Self::UiRenderInvalidConfig { .. } => {
                "Use non-zero viewport and atlas sizes, finite UI rectangles/colors, valid fog grids, and bounded minimap world coordinates."
            }
            Self::UiRenderPersistenceFailed { .. } => {
                "Write UI/font artifacts to a local writable path, then read and decode that same file before using the atlas."
            }
            Self::UiRenderReadbackMismatch { .. } => {
                "Stop using the UI/font artifact, reopen the source-of-truth file, and reconcile the persisted hash or dimensions with the expected atlas."
            }
            Self::TextShapingInvalidConfig { .. } => {
                "Reject the shape request, then provide a readable real font, non-empty single-line Unicode text, explicit direction, and covered glyphs."
            }
            Self::TextShapingPersistenceFailed { .. } => {
                "Write the shaped text JSON to a local writable path, then read and decode that same file before using glyph positions."
            }
            Self::TextShapingReadbackMismatch { .. } => {
                "Stop using the shaped text run, reopen the source-of-truth JSON, recompute glyph positions and FNV-1a, and reconcile the persisted artifact."
            }
            Self::ReferenceSceneInvalidConfig { .. } => {
                "Use a non-zero visual target size, valid GPU render/readback resources, and keep reference-scene metrics inside the README-quality comparison bands."
            }
            Self::EditorWorldRendererInvalidConfig { .. } => {
                "Use a non-zero viewport inside the current surface, finite bird-eye camera values, valid indexed mesh buffers, and a live wgpu depth/render pipeline."
            }
            Self::VfxInvalidConfig { .. } => {
                "Fix the VFX effect asset schema, content-store descriptor, render cue reference, emitter lifecycle modules, particle caps, curves, gradients, or render outputs before saving or playing the effect."
            }
            Self::ProcGenInvalidConfig { .. } => {
                "Fix the procedural geometry spec: use non-zero primitive extents, subdivision counts inside the supported range, at least a 2x2 heightfield with matching sample count, and a positive scatter count with weighted prototypes before generating."
            }
            Self::ProcGenPersistenceFailed { .. } => {
                "Ensure the content-addressed procgen store root is writable and the mesh content hash is canonical before storing or loading a frozen mesh."
            }
            Self::ProcGenReadbackMismatch { .. } => {
                "The frozen mesh bytes on disk no longer match their content hash; regenerate the mesh and re-store it, then read it back by hash to reconcile the digests."
            }
            Self::TerrainRuntimeInvalidConfig { .. } => {
                "Fix the terrain runtime config: use a power-of-two leaf count, a grid_dim >= 2, a positive lod0 range, a morph_start_ratio in [0,1), and a heightfield whose (size-1) is divisible by the leaf count before selecting a frame."
            }
        }
    }
}
