use std::{collections::BTreeMap, path::Path};

use calyx_assay::DEFAULT_GATE_PANEL_LENSES;
use calyx_core::SlotState;
use calyx_registry::{
    CapabilitySignalKind, Registry, VaultPanelState, load_vault_panel_state, signal_kind_from_spec,
};

use crate::calyx_content::EngineCalyxContentError;

#[path = "calyx_content_measurement_panel_register.rs"]
mod register;
use register::{
    cpu_explicit_text_candidate, cpu_explicit_text_runtime, is_lens_unreachable,
    lens_runtime_label, register_cpu_explicit_text_lens, register_pending_unavailable_lens,
};

/// How many learned-encoder text lenses actually loaded vs addon-pending stubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CpuExplicitPanelLoadStats {
    pub loaded_learned: usize,
    pub pending_learned: usize,
}

pub(crate) fn load_cpu_explicit_panel_state(
    vault_dir: &Path,
) -> Result<VaultPanelState, EngineCalyxContentError> {
    let (state, _stats) = load_cpu_explicit_panel_state_with_stats(vault_dir)?;
    Ok(state)
}

pub(crate) fn load_cpu_explicit_panel_state_with_stats(
    vault_dir: &Path,
) -> Result<(VaultPanelState, CpuExplicitPanelLoadStats), EngineCalyxContentError> {
    let source = load_vault_panel_state(vault_dir).map_err(|error| {
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
    let snapshot = source.registry_snapshot.clone().ok_or_else(|| {
        measurement_failed(
            "load_panel",
            format!(
                "measurement panel {} has no manifest-backed registry snapshot",
                vault_dir.display()
            ),
        )
    })?;
    let snapshots = snapshot
        .lenses
        .iter()
        .map(|lens| (lens.lens_id, lens))
        .collect::<BTreeMap<_, _>>();
    let mut panel = source.panel.clone();
    let mut registry = Registry::new();
    let mut loaded_learned = 0usize;
    let mut pending_learned = 0usize;

    for slot in &mut panel.slots {
        if !cpu_explicit_text_candidate(slot) {
            continue;
        }
        let snapshot = snapshots.get(&slot.lens_id).ok_or_else(|| {
            measurement_failed(
                "load_cpu_explicit_panel",
                format!(
                    "active text slot {} lens {} has no registry snapshot",
                    slot.slot_key.key(),
                    slot.lens_id
                ),
            )
        })?;
        let spec = snapshot.spec.as_ref().ok_or_else(|| {
            measurement_failed(
                "load_cpu_explicit_panel",
                format!(
                    "active text slot {} lens {} has no structured lens spec",
                    slot.slot_key.key(),
                    slot.lens_id
                ),
            )
        })?;
        if signal_kind_from_spec(spec) != CapabilitySignalKind::LearnedEncoder {
            slot.state = SlotState::Parked;
            continue;
        }
        // The CPU-explicit content panel is a derived runtime view. A source
        // slot parked for the default GPU/no-fallback runtime may be admitted
        // here only after the explicit CPU lens actually loads.
        //
        // Multi-embedder mandate (#1348 / #1479 / #1775): a learned-encoder text
        // family present in the source panel but not surfaceable by the CPU-explicit
        // content panel must FAIL LOUD -- never silently parked/omitted from the
        // derived A37 roster. Every LOCALLY-runnable learned text family loads through
        // its explicit CPU runtime below: dense-ONNX, fastembed-sparse, BGE-M3,
        // ONNX-ColBERT, fastembed-qwen3 + candle-local (candle CPU) and static-lookup
        // (mmap CPU). The families that stay fail-loud here are non-local or non-
        // content-embedding: tei-http (remote endpoint), multimodal-adapter (external
        // cross-modal subprocess bridge, no in-process CPU device policy) and
        // fastembed-reranker (cross-encoder Dense(1) score, not a content vector).
        // external-cmd is Unknown (not learned) and is parked above before here.
        let Some(runtime) = cpu_explicit_text_runtime(spec) else {
            return Err(measurement_failed(
                "load_cpu_explicit_panel",
                format!(
                    "learned-encoder text slot {} lens {} family (output {:?}, runtime {}) has no CPU-explicit content-panel surfacing; the multi-embedder mandate forbids silently excluding an available A37 family",
                    slot.slot_key.key(),
                    slot.lens_id,
                    spec.output,
                    lens_runtime_label(&spec.runtime),
                ),
            ));
        };
        match register_cpu_explicit_text_lens(&mut registry, snapshot, spec, runtime) {
            Ok(()) => {
                slot.state = SlotState::Active;
                loaded_learned += 1;
            }
            Err(error) if is_lens_unreachable(&error) => {
                // The AI addon (ONNX runtime) is unavailable for THIS lens. Rather
                // than fail the whole authoring persist (#1791), register a pending
                // stub carrying the exact frozen contract + spec so the declared-
                // lens gate and the A37 roster identity still hold, and the measure
                // path degrades this slot to absent(LensUnavailable) for addon-time
                // backfill. Mirrors calyx-mcp ingest:385 degradation semantics.
                register_pending_unavailable_lens(&mut registry, snapshot, spec)?;
                slot.state = SlotState::Active;
                pending_learned += 1;
            }
            Err(error) => return Err(error),
        }
    }

    let declared_learned = loaded_learned + pending_learned;
    if declared_learned < DEFAULT_GATE_PANEL_LENSES {
        return Err(measurement_failed(
            "load_cpu_explicit_panel",
            format!(
                "content ingest requires a panel that DECLARES at least {DEFAULT_GATE_PANEL_LENSES} CPU-explicit text learned-encoder lenses; panel v{} declares {declared_learned} ({loaded_learned} loaded, {pending_learned} pending addon)",
                panel.version
            ),
        ));
    }

    Ok((
        VaultPanelState {
            panel,
            registry,
            registry_snapshot: Some(snapshot),
        },
        CpuExplicitPanelLoadStats {
            loaded_learned,
            pending_learned,
        },
    ))
}

pub(super) fn measurement_failed(
    phase: impl Into<String>,
    reason: impl Into<String>,
) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: phase.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "calyx_content_measurement_panel_tests.rs"]
mod tests;
