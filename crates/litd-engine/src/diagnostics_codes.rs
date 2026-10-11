//! Stable machine-readable error codes for every [`EngineError`] variant (#1466 split of
//! `diagnostics.rs`).
//!
//! One `LITD-<SUBSYSTEM>-<FAULT>` code per variant. These strings are a contract: logs,
//! tests and tooling match on them, so a code may be added but never silently renamed.
//! The human-facing fix-it text lives in `diagnostics_remediation.rs`.

use crate::{EngineError, error::RenderingError};

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoAdapter(_) => "LITD-GPU-NO-ADAPTER",
            Self::SurfaceCreate(_) => "LITD-RENDER-SURFACE-CREATE",
            Self::NoSurfaceAdapter => "LITD-RENDER-NO-SURFACE-ADAPTER",
            Self::RequestAdapter(_) => "LITD-RENDER-REQUEST-ADAPTER",
            Self::RequestDevice(_) => "LITD-RENDER-REQUEST-DEVICE",
            Self::NoSurfaceConfig { .. } => "LITD-RENDER-NO-SURFACE-CONFIG",
            Self::InvalidSurfaceSize { .. } => "LITD-RENDER-INVALID-SURFACE-SIZE",
            Self::SurfaceFrame(_) => "LITD-RENDER-SURFACE-FRAME",
            Self::DuplicateModule(_) => "LITD-APP-DUPLICATE-MODULE",
            Self::ModuleRegistrationClosed(_) => "LITD-APP-REGISTRATION-CLOSED",
            Self::AppRuntimeNotStarted => "LITD-APP-FRAME-BEFORE-START",
            Self::DeterminismBoundaryViolation { .. } => "LITD-DETERMINISM-BOUNDARY-VIOLATION",
            Self::AudioInvalidConfig { .. } => "LITD-AUDIO-INVALID-CONFIG",
            Self::AudioBackendUnavailable { .. } => "LITD-AUDIO-BACKEND-UNAVAILABLE",
            Self::AudioOperationFailed { .. } => "LITD-AUDIO-OPERATION-FAILED",
            Self::AudioReadbackMismatch { .. } => "LITD-AUDIO-READBACK-MISMATCH",
            Self::ProfileInvalidConfig { .. } => "LITD-PROFILE-INVALID-CONFIG",
            Self::ProfileInvalidOperation { .. } => "LITD-PROFILE-INVALID-OPERATION",
            Self::ProfilePersistenceFailed { .. } => "LITD-PROFILE-PERSISTENCE-FAILED",
            Self::ProfileReadbackMismatch { .. } => "LITD-PROFILE-READBACK-MISMATCH",
            Self::UnknownFailLoudProbe(_) => "LITD-FOUNDATION-UNKNOWN-FAIL-LOUD-PROBE",
            Self::LoggingInvalidConfig { .. } => "LITD-FOUNDATION-LOGGING-INVALID-CONFIG",
            Self::AnimationInvalidConfig { .. } => "LITD-ANIMATION-INVALID-CONFIG",
            Self::AssetVaultUnavailable(_) => "LITD-ASSET-VAULT-UNAVAILABLE",
            Self::PhysicsInvalidConfig { .. } => "LITD-PHYSICS-INVALID-CONFIG",
            Self::ForgeInvalidConfig { .. } => "LITD-FORGE-INVALID-CONFIG",
            Self::ForgeBackendUnavailable { .. } => "LITD-FORGE-BACKEND-UNAVAILABLE",
            Self::ForgeOperationFailed { .. } => "LITD-FORGE-OPERATION-FAILED",
            Self::ForgeParityViolation { .. } => "LITD-FORGE-PARITY-VIOLATION",
            Self::ForgeQuantizationBoundExceeded { .. } => "LITD-FORGE-QUANT-BOUND-EXCEEDED",
            Self::AnnealInvalidConfig { .. } => "LITD-ANNEAL-INVALID-CONFIG",
            Self::AnnealOperationFailed { .. } => "LITD-ANNEAL-OPERATION-FAILED",
            Self::AnnealPersistenceFailed { .. } => "LITD-ANNEAL-PERSISTENCE-FAILED",
            Self::AnnealReadbackMismatch { .. } => "LITD-ANNEAL-READBACK-MISMATCH",
            Self::OracleInvalidConfig { .. } => "LITD-ORACLE-INVALID-CONFIG",
            Self::OracleOperationFailed { .. } => "LITD-ORACLE-OPERATION-FAILED",
            Self::OraclePersistenceFailed { .. } => "LITD-ORACLE-PERSISTENCE-FAILED",
            Self::OracleReadbackMismatch { .. } => "LITD-ORACLE-READBACK-MISMATCH",
            Self::OracleInsufficientHistory { .. } => "LITD-ORACLE-INSUFFICIENT-HISTORY",
            Self::OracleNonStationary { .. } => "LITD-ORACLE-NON-STATIONARY",
            Self::OraclePreviewInsufficient { .. } => "LITD-ORACLE-PREVIEW-INSUFFICIENT",
            Self::InvalidAabb { .. } => "LITD-MATH-INVALID-AABB",
            Self::FixedPointDivideByZero => "LITD-MATH-FIXED-DIVIDE-BY-ZERO",
            Self::FixedPointOverflow { .. } => "LITD-MATH-FIXED-OVERFLOW",
            Self::SimInvalidConfig { .. } => "LITD-SIM-INVALID-CONFIG",
            Self::SimCapacityExceeded { .. } => "LITD-SIM-CAPACITY-EXCEEDED",
            Self::SimOrderCapacityExceeded { .. } => "LITD-SIM-ORDER-CAPACITY-EXCEEDED",
            Self::SimOrderPastDue { .. } => "LITD-SIM-ORDER-PAST-DUE",
            Self::SimBodyMissing { .. } => "LITD-SIM-BODY-MISSING",
            Self::SimTickOverflow => "LITD-SIM-TICK-OVERFLOW",
            Self::NavInvalidConfig { .. } => "LITD-NAV-INVALID-CONFIG",
            Self::NavPersistenceFailed { .. } => "LITD-NAV-PERSISTENCE-FAILED",
            Self::NavReadbackMismatch { .. } => "LITD-NAV-READBACK-MISMATCH",
            Self::NavPathNotFound { .. } => "LITD-NAV-PATH-NOT-FOUND",
            Self::NavPathBudgetExceeded { .. } => "LITD-NAV-BUDGET-EXCEEDED",
            Self::AiInvalidConfig { .. } => "LITD-AI-INVALID-CONFIG",
            Self::AiPersistenceFailed { .. } => "LITD-AI-PERSISTENCE-FAILED",
            Self::AiReadbackMismatch { .. } => "LITD-AI-READBACK-MISMATCH",
            Self::StreamInvalidConfig { .. } => "LITD-STREAM-INVALID-CONFIG",
            Self::StreamPersistenceFailed { .. } => "LITD-STREAM-PERSISTENCE-FAILED",
            Self::StreamReadbackMismatch { .. } => "LITD-STREAM-READBACK-MISMATCH",
            Self::AssetStreamInvalidConfig { .. } => "LITD-ASSET-STREAM-INVALID-CONFIG",
            Self::AssetStreamPersistenceFailed { .. } => "LITD-ASSET-STREAM-PERSISTENCE-FAILED",
            Self::AssetStreamReadbackMismatch { .. } => "LITD-ASSET-STREAM-READBACK-MISMATCH",
            Self::EcsEntityNotAlive { .. } => "LITD-ECS-ENTITY-NOT-ALIVE",
            Self::EcsEntityIndexOverflow { .. } => "LITD-ECS-ENTITY-INDEX-OVERFLOW",
            Self::EcsEntityGenerationOverflow { .. } => "LITD-ECS-ENTITY-GENERATION-OVERFLOW",
            Self::EcsFreeListCorrupt { .. } => "LITD-ECS-FREE-LIST-CORRUPT",
            Self::EcsMissingComponent { .. } => "LITD-ECS-MISSING-COMPONENT",
            Self::EcsDuplicateComponent { .. } => "LITD-ECS-DUPLICATE-COMPONENT",
            Self::EcsQueryDuplicateComponent { .. } => "LITD-ECS-DUPLICATE-QUERY-COMPONENT",
            Self::EcsComponentStorageMismatch { .. } => "LITD-ECS-STORAGE-MISMATCH",
            Self::SceneDuplicateNode { .. } => "LITD-SCENE-DUPLICATE-NODE",
            Self::SceneNodeMissing { .. } => "LITD-SCENE-NODE-MISSING",
            Self::SceneParentCycle { .. } => "LITD-SCENE-PARENT-CYCLE",
            Self::ObjectRequirementCycle { .. } => "LITD-OBJECT-REQUIREMENT-CYCLE",
            Self::ReflectionDuplicateType { .. } => "LITD-REFLECT-DUPLICATE-TYPE",
            Self::SerializationFailed { .. } => "LITD-REFLECT-SERIALIZATION-FAILED",
            Self::SerializedTypeMismatch { .. } => "LITD-REFLECT-TYPE-MISMATCH",
            Self::SerializedFingerprintMismatch { .. } => "LITD-REFLECT-FINGERPRINT-MISMATCH",
            Self::GamePackageInvalidConfig { .. } => "LITD-GAME-PACKAGE-INVALID-CONFIG",
            Self::RpgStatsInvalidConfig { .. } => "LITD-RPG-STATS-INVALID-CONFIG",
            Self::RpgAbilityInvalidConfig { .. } => "LITD-RPG-ABILITY-INVALID-CONFIG",
            Self::RpgAdvancementInvalidConfig { .. } => "LITD-RPG-ADVANCEMENT-INVALID-CONFIG",
            Self::RpgCombatInvalidConfig { .. } => "LITD-RPG-COMBAT-INVALID-CONFIG",
            Self::RpgThemeInvalidConfig { .. } => "LITD-RPG-THEME-INVALID-CONFIG",
            Self::RpgImmortalInvalidConfig { .. } => "LITD-RPG-IMMORTAL-INVALID-CONFIG",
            Self::RpgFactionInvalidConfig { .. } => "LITD-RPG-FACTION-INVALID-CONFIG",
            Self::AgentModelTieringInvalidConfig { .. } => {
                "LITD-AI-AGENT-MODEL-TIERING-INVALID-CONFIG"
            }
            Self::Rendering(RenderingError::PbrMaterialInvalidFactor { .. }) => {
                "LITD-RENDER-PBR-INVALID-FACTOR"
            }
            Self::Rendering(RenderingError::PbrInvalidVector { .. }) => {
                "LITD-RENDER-PBR-INVALID-VECTOR"
            }
            Self::Rendering(RenderingError::ClusteredLightingInvalidConfig { .. }) => {
                "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG"
            }
            Self::Rendering(RenderingError::ClusteredLightingClusterOverflow { .. }) => {
                "LITD-RENDER-CLUSTERED-LIGHTING-OVERFLOW"
            }
            Self::Rendering(RenderingError::ClusteredLightingGpuReadbackMismatch { .. }) => {
                "LITD-RENDER-CLUSTERED-LIGHTING-GPU-READBACK-MISMATCH"
            }
            Self::ShadowInvalidConfig { .. } => "LITD-RENDER-SHADOW-INVALID-CONFIG",
            Self::ShadowAtlasOverflow { .. } => "LITD-RENDER-SHADOW-ATLAS-OVERFLOW",
            Self::PostProcessInvalidConfig { .. } => "LITD-RENDER-POST-INVALID-CONFIG",
            Self::ScreenSpaceInvalidConfig { .. } => "LITD-RENDER-SCREEN-SPACE-INVALID-CONFIG",
            Self::UpscaleInvalidConfig { .. } => "LITD-RENDER-UPSCALE-INVALID-CONFIG",
            Self::BatchingInvalidConfig { .. } => "LITD-RENDER-BATCHING-INVALID-CONFIG",
            Self::BatchingDrawLimitExceeded { .. } => "LITD-RENDER-BATCHING-DRAW-LIMIT",
            Self::Rendering(RenderingError::ShaderInvalidConfig { .. }) => {
                "LITD-RENDER-SHADER-INVALID-CONFIG"
            }
            Self::Rendering(RenderingError::ShaderBindingValidationFailed { .. }) => {
                "LITD-RENDER-SHADER-BINDING-VALIDATION"
            }
            Self::Rendering(RenderingError::StylizedShaderInvalidConfig { .. }) => {
                "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG"
            }
            Self::Rendering(RenderingError::MaterialCatalogInvalidConfig { .. }) => {
                "LITD-RENDER-MATERIAL-CATALOG-INVALID-CONFIG"
            }
            Self::Rendering(RenderingError::CameraInvalidConfig { .. }) => {
                "LITD-RENDER-CAMERA-INVALID-CONFIG"
            }
            Self::UiRenderInvalidConfig { .. } => "LITD-RENDER-UI-INVALID-CONFIG",
            Self::UiRenderPersistenceFailed { .. } => "LITD-RENDER-UI-PERSISTENCE-FAILED",
            Self::UiRenderReadbackMismatch { .. } => "LITD-RENDER-UI-READBACK-MISMATCH",
            Self::TextShapingInvalidConfig { .. } => "LITD-RENDER-TEXT-SHAPING-INVALID-CONFIG",
            Self::TextShapingPersistenceFailed { .. } => {
                "LITD-RENDER-TEXT-SHAPING-PERSISTENCE-FAILED"
            }
            Self::TextShapingReadbackMismatch { .. } => {
                "LITD-RENDER-TEXT-SHAPING-READBACK-MISMATCH"
            }
            Self::ReferenceSceneInvalidConfig { .. } => {
                "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
            }
            Self::EditorWorldRendererInvalidConfig { .. } => {
                "LITD-RENDER-EDITOR-WORLD-INVALID-CONFIG"
            }
            Self::VfxInvalidConfig { .. } => "LITD-RENDER-VFX-INVALID-CONFIG",
            Self::ProcGenInvalidConfig { .. } => "LITD-PROCGEN-INVALID-CONFIG",
            Self::ProcGenPersistenceFailed { .. } => "LITD-PROCGEN-PERSISTENCE-FAILED",
            Self::ProcGenReadbackMismatch { .. } => "LITD-PROCGEN-READBACK-MISMATCH",
            Self::TerrainRuntimeInvalidConfig { .. } => "LITD-TERRAIN-RUNTIME-INVALID-CONFIG",
        }
    }
}
