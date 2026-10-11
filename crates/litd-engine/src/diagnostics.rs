//! Structured fail-loud diagnostics for engine errors.
//!
//! Split (#1466, extending the #1543 remediation split) into three cohesive modules:
//!   - `diagnostics.rs`             — the `ErrorReport` shape, `report()` and the tracing sink
//!   - `diagnostics_codes.rs`       — `EngineError::code()`, the stable error-code taxonomy
//!   - `diagnostics_remediation.rs` — `EngineError::remediation()`, the fix-it guidance
//!
//! `code()` and `remediation()` are inherent methods on `EngineError`, so they stay callable
//! as `error.code()` / `error.remediation()` from anywhere in the crate exactly as before.

use std::error::Error;

use crate::EngineError;

#[path = "diagnostics_codes.rs"]
mod diagnostics_codes;
#[path = "diagnostics_remediation.rs"]
mod diagnostics_remediation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReport {
    pub code: &'static str,
    pub message: String,
    pub remediation: &'static str,
    pub source_error: Option<String>,
}

impl ErrorReport {
    pub fn as_key_values(&self) -> String {
        format!(
            "error_code={}\nerror_message={}\nremediation={}\nsource_error={}\n",
            self.code,
            self.message,
            self.remediation,
            self.source_error.as_deref().unwrap_or("")
        )
    }
}

impl EngineError {
    pub fn report(&self) -> ErrorReport {
        ErrorReport {
            code: self.code(),
            message: self.to_string(),
            remediation: self.remediation(),
            source_error: self.source().map(ToString::to_string),
        }
    }
}

pub fn log_engine_error(context: &'static str, error: &EngineError) -> ErrorReport {
    let report = error.report();
    tracing::error!(
        context,
        error_code = report.code,
        error_message = %report.message,
        remediation = report.remediation,
        source_error = report.source_error.as_deref().unwrap_or(""),
        "engine failure"
    );
    report
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
