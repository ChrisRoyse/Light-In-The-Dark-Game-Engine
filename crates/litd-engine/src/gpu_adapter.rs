//! Explicit, logged GPU adapter selection (#1793 / #1790 sec 3).
//!
//! wgpu's `request_adapter` picks an adapter through an OPAQUE `PowerPreference`
//! whose result we cannot inspect or justify. On a laptop with both a discrete and
//! an integrated GPU that silently deprioritizes the integrated adapter, and it
//! gives no record of what was rejected and why. This module replaces that with an
//! EXPLICIT policy: enumerate every adapter, rank by a caller-chosen preference,
//! log the chosen adapter plus every rejected one, and fail loud with a typed error
//! when a REQUIRED adapter class is absent. Discrete is a preference, never a
//! requirement: `DiscretePreferred` (the default) still selects an integrated GPU
//! when that is all the machine has, so Intel Iris Xe / AMD APU class hardware
//! acquires and renders the full editor.

use crate::error::EngineError;

/// Environment override for adapter selection: `discrete`, `integrated`,
/// `force-integrated`, or `auto` (the default).
pub const ADAPTER_PREFERENCE_ENV: &str = "LITD_GPU_ADAPTER";

/// How to choose among the adapters wgpu enumerates. Discrete is only ever a
/// *preference*; the integrated path is always a first-class supported target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterPreference {
    /// Prefer discrete, then integrated, then virtual, then anything. Never fails
    /// merely because no discrete GPU exists (the min-spec integrated path).
    DiscretePreferred,
    /// Prefer an integrated GPU; fall back to the discrete-preferred order only if
    /// no integrated adapter is present. Power-saving / min-spec validation.
    IntegratedPreferred,
    /// Require an integrated GPU and fail loud if none exists. Used to force the
    /// integrated adapter on a discrete-primary machine (e.g. the RTX 5060 box)
    /// so the integrated path is exercised in reality.
    ForceIntegrated,
}

impl AdapterPreference {
    /// Read the preference from `LITD_GPU_ADAPTER`. Unknown/empty values fail loud
    /// rather than silently defaulting -- a typo must not degrade selection.
    pub fn from_env() -> Result<Self, EngineError> {
        match std::env::var(ADAPTER_PREFERENCE_ENV) {
            Err(std::env::VarError::NotPresent) => Ok(Self::DiscretePreferred),
            Ok(raw) => Self::parse(raw.trim()),
            Err(error) => Err(adapter_error(
                "gpu.adapter.env",
                format!("{ADAPTER_PREFERENCE_ENV} is not readable: {error}"),
            )),
        }
    }

    /// Parse a preference token. ASCII-case-insensitive; `-`/`_` interchangeable.
    pub fn parse(raw: &str) -> Result<Self, EngineError> {
        match raw.to_ascii_lowercase().replace('_', "-").as_str() {
            "" | "auto" | "discrete-preferred" | "discrete" => Ok(Self::DiscretePreferred),
            "integrated-preferred" | "integrated" => Ok(Self::IntegratedPreferred),
            "force-integrated" => Ok(Self::ForceIntegrated),
            other => Err(adapter_error(
                "gpu.adapter.env",
                format!(
                    "unknown {ADAPTER_PREFERENCE_ENV} value '{other}'; expected auto|discrete|integrated|force-integrated"
                ),
            )),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::DiscretePreferred => "discrete-preferred",
            Self::IntegratedPreferred => "integrated-preferred",
            Self::ForceIntegrated => "force-integrated",
        }
    }
}

/// The outcome of a selection over an enumerated adapter list: the winning index
/// plus a human/log-friendly record of every rejected candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSelection {
    pub chosen_index: usize,
    pub chosen_label: String,
    pub preference: &'static str,
    pub rejected: Vec<String>,
}

/// Rank an adapter for a preference: `(device_rank, backend_tiebreak)`, higher
/// wins. Backend tie-break keeps our Vulkan-first Windows doctrine (#928 render).
fn rank_for(pref: AdapterPreference, info: &wgpu::AdapterInfo) -> (u8, u8) {
    let device = match (pref, info.device_type) {
        (AdapterPreference::IntegratedPreferred, wgpu::DeviceType::IntegratedGpu) => 4,
        (AdapterPreference::IntegratedPreferred, wgpu::DeviceType::DiscreteGpu) => 3,
        (_, wgpu::DeviceType::DiscreteGpu) => 3,
        (_, wgpu::DeviceType::IntegratedGpu) => 2,
        (_, wgpu::DeviceType::VirtualGpu) => 1,
        _ => 0,
    };
    let backend = match info.backend {
        wgpu::Backend::Vulkan => 2,
        wgpu::Backend::Dx12 => 1,
        _ => 0,
    };
    (device, backend)
}

fn label_of(info: &wgpu::AdapterInfo) -> String {
    format!("{} ({:?}/{:?})", info.name, info.device_type, info.backend)
}

/// Select the adapter index for `pref` over the enumerated `infos` (already
/// filtered to surface-compatible adapters by the caller). Fails loud with a
/// typed error when the list is empty or a REQUIRED class is absent -- never a
/// silent fallback. Returns the chosen index and the rejected candidates.
pub fn select_adapter_index(
    pref: AdapterPreference,
    infos: &[wgpu::AdapterInfo],
) -> Result<AdapterSelection, EngineError> {
    if infos.is_empty() {
        return Err(adapter_error(
            "gpu.adapter.select",
            "no candidate adapters were enumerated",
        ));
    }
    if pref == AdapterPreference::ForceIntegrated
        && !infos
            .iter()
            .any(|info| info.device_type == wgpu::DeviceType::IntegratedGpu)
    {
        let present = infos.iter().map(label_of).collect::<Vec<_>>().join(", ");
        return Err(adapter_error(
            "gpu.adapter.force_integrated",
            format!(
                "force-integrated requested but no IntegratedGpu adapter is present; enumerated: [{present}]"
            ),
        ));
    }
    let eligible: Vec<usize> = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| {
            pref != AdapterPreference::ForceIntegrated
                || info.device_type == wgpu::DeviceType::IntegratedGpu
        })
        .map(|(index, _)| index)
        .collect();
    let chosen_index = *eligible
        .iter()
        .max_by_key(|&&index| rank_for(pref, &infos[index]))
        .expect("eligible is non-empty: infos non-empty and force-integrated case checked above");
    let rejected = infos
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != chosen_index)
        .map(|(_, info)| label_of(info))
        .collect();
    Ok(AdapterSelection {
        chosen_index,
        chosen_label: label_of(&infos[chosen_index]),
        preference: pref.label(),
        rejected,
    })
}

fn adapter_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(
        name: &str,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: name.to_string(),
            vendor: 0,
            device: 0,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend,
        }
    }

    #[test]
    fn discrete_preferred_still_selects_integrated_when_only_option() {
        // #1793 core: discrete is a preference, never a requirement.
        let infos = [info(
            "Iris Xe",
            wgpu::DeviceType::IntegratedGpu,
            wgpu::Backend::Vulkan,
        )];
        let sel = select_adapter_index(AdapterPreference::DiscretePreferred, &infos).unwrap();
        assert_eq!(sel.chosen_index, 0);
        assert!(sel.rejected.is_empty());
    }

    #[test]
    fn discrete_preferred_picks_discrete_over_integrated() {
        let infos = [
            info(
                "Iris Xe",
                wgpu::DeviceType::IntegratedGpu,
                wgpu::Backend::Vulkan,
            ),
            info(
                "RTX 5060",
                wgpu::DeviceType::DiscreteGpu,
                wgpu::Backend::Vulkan,
            ),
        ];
        let sel = select_adapter_index(AdapterPreference::DiscretePreferred, &infos).unwrap();
        assert_eq!(sel.chosen_index, 1);
        assert_eq!(sel.rejected.len(), 1);
        assert!(sel.rejected[0].contains("Iris Xe"));
    }

    #[test]
    fn integrated_preferred_picks_integrated_over_discrete() {
        let infos = [
            info(
                "RTX 5060",
                wgpu::DeviceType::DiscreteGpu,
                wgpu::Backend::Vulkan,
            ),
            info(
                "Iris Xe",
                wgpu::DeviceType::IntegratedGpu,
                wgpu::Backend::Vulkan,
            ),
        ];
        let sel = select_adapter_index(AdapterPreference::IntegratedPreferred, &infos).unwrap();
        assert_eq!(sel.chosen_index, 1);
    }

    #[test]
    fn vulkan_tiebreak_wins_within_same_device_class() {
        let infos = [
            info(
                "RTX 5060 DX12",
                wgpu::DeviceType::DiscreteGpu,
                wgpu::Backend::Dx12,
            ),
            info(
                "RTX 5060 VK",
                wgpu::DeviceType::DiscreteGpu,
                wgpu::Backend::Vulkan,
            ),
        ];
        let sel = select_adapter_index(AdapterPreference::DiscretePreferred, &infos).unwrap();
        assert_eq!(sel.chosen_index, 1);
    }

    #[test]
    fn force_integrated_fails_loud_when_absent() {
        // #1793 fail-loud edge: forcing integrated on a discrete-only machine.
        let infos = [info(
            "RTX 5060",
            wgpu::DeviceType::DiscreteGpu,
            wgpu::Backend::Vulkan,
        )];
        match select_adapter_index(AdapterPreference::ForceIntegrated, &infos) {
            Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
                assert_eq!(field, "gpu.adapter.force_integrated");
            }
            other => panic!("expected fail-loud force_integrated error, got {other:?}"),
        }
    }

    #[test]
    fn empty_adapter_list_fails_loud() {
        match select_adapter_index(AdapterPreference::DiscretePreferred, &[]) {
            Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
                assert_eq!(field, "gpu.adapter.select");
            }
            other => panic!("expected empty-list error, got {other:?}"),
        }
    }

    #[test]
    fn env_parse_rejects_typos_loudly() {
        assert_eq!(
            AdapterPreference::parse("auto").unwrap(),
            AdapterPreference::DiscretePreferred
        );
        assert_eq!(
            AdapterPreference::parse("force-integrated").unwrap(),
            AdapterPreference::ForceIntegrated
        );
        assert_eq!(
            AdapterPreference::parse("INTEGRATED").unwrap(),
            AdapterPreference::IntegratedPreferred
        );
        assert!(AdapterPreference::parse("discreet").is_err());
    }
}
