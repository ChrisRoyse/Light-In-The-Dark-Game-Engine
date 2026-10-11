//! Explicit Calyx AI-addon gate for editor/dev cycles (#1790/#1792/#1791).
//!
//! Heavy embedders/encoders/ORT must never be mandatory for base authoring.
//! Operators pick the mode with `LITD_CALYX_AI_ADDON`:
//!
//! * **unset / `auto`** — try the CPU-explicit content panel; unmaterialized
//!   vaults degrade LOUDLY to pending (`#1890`/`#1791`).
//! * **`0` / `off` / `absent`** — force the addon-absent pending panel (skip
//!   ORT/panel load) so edit cycles stay fast on any host.
//! * **`1` / `on` / `require`** — require a materialised panel with loadable
//!   learned lenses; missing CURRENT or all-unreachable fails LOUD with a
//!   greppable code (no silent pending success).

use std::env;

use crate::calyx_content::EngineCalyxContentError;
use crate::calyx_content_measurement::measurement_failed;

/// Env var name (also listed in `docs/env-vars.md`; enforced by `check-env-vars.ps1`).
pub const CALYX_AI_ADDON_ENV: &str = "LITD_CALYX_AI_ADDON";

/// Greppable fail code when `require` mode cannot surface a real panel.
pub const CALYX_AI_ADDON_REQUIRED_UNAVAILABLE: &str = "LITD_CALYX_AI_ADDON_REQUIRED_UNAVAILABLE";

/// Greppable log tag when force-off path is taken.
pub const CALYX_AI_ADDON_FORCE_ABSENT: &str = "LITD_CALYX_AI_ADDON_FORCE_ABSENT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalyxAiAddonMode {
    /// Default: try real panel, pending on unmaterialized vault only.
    Auto,
    /// Force pending / no ORT load for fast local cycles.
    ForceAbsent,
    /// Fail closed unless a real CPU-explicit panel loads with learned lenses.
    Require,
}

impl CalyxAiAddonMode {
    /// Parse process environment. Unknown values fail closed (not silent Auto).
    pub fn from_env() -> Result<Self, EngineCalyxContentError> {
        match env::var(CALYX_AI_ADDON_ENV) {
            Err(env::VarError::NotPresent) => Ok(Self::Auto),
            Err(env::VarError::NotUnicode(_)) => Err(measurement_failed(
                "ai_addon_gate",
                format!("{CALYX_AI_ADDON_ENV} is not valid UTF-8"),
            )),
            Ok(raw) => parse_mode(&raw),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ForceAbsent => "absent",
            Self::Require => "require",
        }
    }
}

fn parse_mode(raw: &str) -> Result<CalyxAiAddonMode, EngineCalyxContentError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(measurement_failed(
            "ai_addon_gate",
            format!("{CALYX_AI_ADDON_ENV} is set but empty; use auto|absent|require"),
        ));
    }
    let key = trimmed.to_ascii_lowercase();
    match key.as_str() {
        "auto" | "default" => Ok(CalyxAiAddonMode::Auto),
        "0" | "off" | "false" | "no" | "absent" | "disabled" | "pending" => {
            Ok(CalyxAiAddonMode::ForceAbsent)
        }
        "1" | "on" | "true" | "yes" | "require" | "required" | "full" => {
            Ok(CalyxAiAddonMode::Require)
        }
        other => Err(measurement_failed(
            "ai_addon_gate",
            format!("{CALYX_AI_ADDON_ENV}={other:?} is invalid; use auto|absent|require (or 0/1)"),
        )),
    }
}

/// Structured error when Require mode cannot load a real panel.
pub fn require_mode_unavailable(detail: impl Into<String>) -> EngineCalyxContentError {
    measurement_failed(
        "ai_addon_require",
        format!("{CALYX_AI_ADDON_REQUIRED_UNAVAILABLE}: {}", detail.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_accepts_common_aliases() {
        assert_eq!(parse_mode("auto").unwrap(), CalyxAiAddonMode::Auto);
        assert_eq!(parse_mode("OFF").unwrap(), CalyxAiAddonMode::ForceAbsent);
        assert_eq!(parse_mode("0").unwrap(), CalyxAiAddonMode::ForceAbsent);
        assert_eq!(parse_mode("require").unwrap(), CalyxAiAddonMode::Require);
        assert_eq!(parse_mode("1").unwrap(), CalyxAiAddonMode::Require);
    }

    #[test]
    fn parse_mode_rejects_unknown_loudly() {
        let err = parse_mode("maybe").unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains(CALYX_AI_ADDON_ENV),
            "expected env name in error, got {text}"
        );
        assert!(
            text.contains("invalid"),
            "expected invalid marker in error, got {text}"
        );
    }

    #[test]
    fn parse_mode_rejects_empty_loudly() {
        let err = parse_mode("   ").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn require_mode_error_is_greppable() {
        let err = require_mode_unavailable("panel missing CURRENT");
        let text = err.to_string();
        assert!(
            text.contains(CALYX_AI_ADDON_REQUIRED_UNAVAILABLE),
            "expected greppable code, got {text}"
        );
    }
}
