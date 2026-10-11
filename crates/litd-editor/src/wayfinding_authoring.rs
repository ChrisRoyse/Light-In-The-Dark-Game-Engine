#[path = "wayfinding_authoring_store.rs"]
mod wayfinding_authoring_store;
#[path = "wayfinding_authoring_types.rs"]
mod wayfinding_authoring_types;
#[path = "wayfinding_authoring_validate.rs"]
mod wayfinding_authoring_validate;

pub use wayfinding_authoring_store::{read_wayfinding_ledger, write_wayfinding_ledger};
pub use wayfinding_authoring_types::{
    WAYFINDING_AUTHORING_SCHEMA, WayfindingAuthoringLedger, WayfindingAuthoringReadback,
    WayfindingAuthoringRequest, WayfindingChain, WayfindingCueKind, WayfindingMarkerPolicy,
    WayfindingObjective, WayfindingRouteStep, WayfindingUiProjection, WayfindingValidationReport,
    WayfindingValidationStatus,
};
pub(crate) use wayfinding_authoring_validate::wayfinding_error;
pub use wayfinding_authoring_validate::{
    build_wayfinding_ledger, validate_wayfinding_ledger, validate_wayfinding_request,
};

#[cfg(test)]
#[path = "wayfinding_authoring_tests.rs"]
mod tests;
