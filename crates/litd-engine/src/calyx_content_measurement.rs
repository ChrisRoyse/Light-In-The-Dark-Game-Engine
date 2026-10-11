//! Content measurement CONFIG (#1116/#1791).
//!
//! Owns [`EngineContentMeasurementConfig`] -- which panel vault to measure
//! against and the binding gate thresholds -- plus panel-state loading. The
//! measure-time execution (gate, degraded slot measurement, complete-vs-pending
//! classification) lives in `calyx_content_measurement_exec`.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use calyx_assay::DEFAULT_GATE_PANEL_LENSES;
use calyx_registry::{VaultPanelState, load_vault_panel_state};

use crate::{
    calyx_ai_addon_gate::{
        CALYX_AI_ADDON_FORCE_ABSENT, CalyxAiAddonMode, require_mode_unavailable,
    },
    calyx_content::EngineCalyxContentError,
    calyx_content_measurement_absent::{
        ADDON_ABSENT_CONTENT_PANEL_LENSES, addon_absent_content_panel_state,
    },
    calyx_content_measurement_panel::{
        load_cpu_explicit_panel_state, load_cpu_explicit_panel_state_with_stats,
    },
    calyx_paths::{ENGINE_CONTENT_PANEL_VAULT_NAME, resolve_engine_content_panel_vault},
};

pub(crate) use crate::calyx_content_measurement_exec::measure_content_record;

pub const ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA: &str =
    "litd.engine.calyx_content_measurement.v1";
pub const ENGINE_CONTENT_PANEL_TEMPLATE: &str = "constellation-24";
pub const ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES: usize = 2;

#[derive(Clone)]
pub struct EngineContentMeasurementConfig {
    source: EngineContentMeasurementSource,
    min_learned_lenses: usize,
    min_association_families: usize,
}

#[derive(Clone)]
enum EngineContentMeasurementSource {
    CanonicalCpuExplicitVault,
    Vault(PathBuf),
    CpuExplicitVault(PathBuf),
    Preloaded {
        label: String,
        state: Arc<VaultPanelState>,
    },
}

impl fmt::Debug for EngineContentMeasurementConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EngineContentMeasurementConfig")
            .field("source", &self.source_label())
            .field("min_learned_lenses", &self.min_learned_lenses)
            .field("min_association_families", &self.min_association_families)
            .finish()
    }
}

impl EngineContentMeasurementConfig {
    pub fn default_panel_vault() -> Self {
        Self {
            source: EngineContentMeasurementSource::CanonicalCpuExplicitVault,
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        }
    }

    pub fn from_panel_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            source: EngineContentMeasurementSource::Vault(vault_dir.into()),
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        }
    }

    pub fn from_cpu_explicit_panel_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            source: EngineContentMeasurementSource::CpuExplicitVault(vault_dir.into()),
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        }
    }

    pub fn from_loaded_panel_vault(
        vault_dir: impl Into<PathBuf>,
    ) -> Result<Self, EngineCalyxContentError> {
        let vault_dir = vault_dir.into();
        let state = load_vault_panel_state(&vault_dir).map_err(|error| {
            measurement_failed(
                "load_panel",
                format!(
                    "load measurement panel {} failed: {}: {}",
                    vault_dir.display(),
                    error.code,
                    error.message
                ),
            )
        })?;
        Ok(Self {
            source: EngineContentMeasurementSource::Preloaded {
                label: format!("vault:{}", normalized_path(&vault_dir)),
                state: Arc::new(state),
            },
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        })
    }

    pub fn from_loaded_cpu_explicit_panel_vault(
        vault_dir: impl Into<PathBuf>,
    ) -> Result<Self, EngineCalyxContentError> {
        let vault_dir = vault_dir.into();
        let state = load_cpu_explicit_panel_state(&vault_dir)?;
        Ok(Self {
            source: EngineContentMeasurementSource::Preloaded {
                label: format!("vault_cpu_explicit:{}", normalized_path(&vault_dir)),
                state: Arc::new(state),
            },
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        })
    }

    pub fn from_preloaded_state(label: impl Into<String>, state: VaultPanelState) -> Self {
        Self {
            source: EngineContentMeasurementSource::Preloaded {
                label: label.into(),
                state: Arc::new(state),
            },
            min_learned_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_association_families: ENGINE_CONTENT_MIN_ASSOCIATION_FAMILIES,
        }
    }

    pub fn with_min_learned_lenses(mut self, min_learned_lenses: usize) -> Self {
        self.min_learned_lenses = min_learned_lenses;
        self
    }

    pub(crate) fn min_learned_lenses(&self) -> usize {
        self.min_learned_lenses
    }

    pub(crate) fn min_association_families(&self) -> usize {
        self.min_association_families
    }

    pub(crate) fn load_state(
        &self,
    ) -> Result<(Arc<VaultPanelState>, String), EngineCalyxContentError> {
        match &self.source {
            EngineContentMeasurementSource::CanonicalCpuExplicitVault => {
                let vault_dir = resolve_engine_content_panel_vault().map_err(|error| {
                    measurement_failed("canonical_calyx_path", error.to_string())
                })?;
                load_cpu_explicit_or_addon_absent(&vault_dir)
            }
            EngineContentMeasurementSource::Vault(vault_dir) => {
                let state = load_vault_panel_state(vault_dir).map_err(|error| {
                    measurement_failed(
                        "load_panel",
                        format!(
                            "load measurement panel {} failed: {}: {}",
                            vault_dir.display(),
                            error.code,
                            error.message
                        ),
                    )
                })?;
                Ok((
                    Arc::new(state),
                    format!("vault:{}", normalized_path(vault_dir)),
                ))
            }
            EngineContentMeasurementSource::CpuExplicitVault(vault_dir) => {
                load_cpu_explicit_or_addon_absent(vault_dir)
            }
            EngineContentMeasurementSource::Preloaded { label, state } => {
                Ok((state.clone(), label.clone()))
            }
        }
    }

    fn source_label(&self) -> String {
        match &self.source {
            EngineContentMeasurementSource::CanonicalCpuExplicitVault => {
                format!("canonical_vault_index:{ENGINE_CONTENT_PANEL_VAULT_NAME}")
            }
            EngineContentMeasurementSource::Vault(vault_dir) => {
                format!("vault:{}", normalized_path(vault_dir))
            }
            EngineContentMeasurementSource::CpuExplicitVault(vault_dir) => {
                format!("vault_cpu_explicit:{}", normalized_path(vault_dir))
            }
            EngineContentMeasurementSource::Preloaded { label, .. } => label.clone(),
        }
    }
}

pub(crate) fn measurement_failed(
    phase: impl Into<String>,
    reason: impl Into<String>,
) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: phase.into(),
        reason: reason.into(),
    }
}

/// Load the CPU-explicit panel from `vault_dir`, honouring `LITD_CALYX_AI_ADDON`
/// (#1790/#1792): force-absent for fast cycles, require for fail-loud full AI,
/// auto for try-panel-then-pending on unmaterialized vaults (#1791/#1890).
fn load_cpu_explicit_or_addon_absent(
    vault_dir: &Path,
) -> Result<(Arc<VaultPanelState>, String), EngineCalyxContentError> {
    let mode = CalyxAiAddonMode::from_env()?;
    match mode {
        CalyxAiAddonMode::ForceAbsent => {
            eprintln!(
                "{CALYX_AI_ADDON_FORCE_ABSENT} path={} mode={} action=using_addon_absent_pending_panel \
                 (#1790/#1792 fast editor cycle)",
                vault_dir.display(),
                mode.as_str()
            );
            Ok((
                Arc::new(addon_absent_content_panel_state(
                    ADDON_ABSENT_CONTENT_PANEL_LENSES,
                )),
                format!("addon_force_absent:{}", normalized_path(vault_dir)),
            ))
        }
        CalyxAiAddonMode::Require => {
            match load_cpu_explicit_panel_state_with_stats(vault_dir) {
                Ok((_state, stats)) if stats.loaded_learned == 0 => {
                    // Panel may load with only pending (unreachable) lenses when ORT/models
                    // are missing (#1791). Require mode must NOT treat that as AI present.
                    Err(require_mode_unavailable(format!(
                        "path={} mode=require loaded_learned=0 pending_learned={} \
                         (no real AI lenses loaded; set LITD_CALYX_AI_ADDON=absent or install addon)",
                        vault_dir.display(),
                        stats.pending_learned
                    )))
                }
                Ok((state, stats)) => Ok((
                    Arc::new(state),
                    format!(
                        "vault_cpu_explicit_required:loaded={}:pending={}:{}",
                        stats.loaded_learned,
                        stats.pending_learned,
                        normalized_path(vault_dir)
                    ),
                )),
                Err(error) => Err(require_mode_unavailable(format!(
                    "path={} mode=require detail={error}",
                    vault_dir.display()
                ))),
            }
        }
        CalyxAiAddonMode::Auto => match load_cpu_explicit_panel_state(vault_dir) {
            Ok(state) => Ok((
                Arc::new(state),
                format!("vault_cpu_explicit:{}", normalized_path(vault_dir)),
            )),
            Err(error) if is_unmaterialized_panel_error(&error) => {
                // Loud diagnostic — not a silent mock: the row is explicitly
                // pending_measurement and the source label names the path.
                eprintln!(
                    "CONTENT_PANEL_UNMATERIALIZED_PENDING path={} detail={error} \
                     action=using_addon_absent_pending_panel (#1791/#1890)",
                    vault_dir.display()
                );
                Ok((
                    Arc::new(addon_absent_content_panel_state(
                        ADDON_ABSENT_CONTENT_PANEL_LENSES,
                    )),
                    format!("addon_absent_pending:{}", normalized_path(vault_dir)),
                ))
            }
            Err(error) => Err(error),
        },
    }
}

fn is_unmaterialized_panel_error(error: &EngineCalyxContentError) -> bool {
    let EngineCalyxContentError::Measurement { phase, reason } = error else {
        return false;
    };
    if phase != "load_panel" {
        return false;
    }
    let lower = reason.to_ascii_lowercase();
    // Aster empty vault: missing CURRENT pointer / not found on disk.
    lower.contains("current")
        || lower.contains("os error 2")
        || lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("the system cannot find")
}

fn normalized_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
