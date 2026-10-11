//! CF-documented per-area repop-policy overrides (#1668).
//!
//! The reset CADENCE is CF-documented globally (helpfile 98) and the repop POLICY
//! is otherwise DERIVED from each area's `AreaKind` (see `area_reset_repop.rs`).
//! This table is the escape hatch for the minority of areas whose CF page /
//! helpfile documents a repop policy that its structural kind would NOT imply --
//! exactly parallel to the #1667 `authored_natural_terrain` override table.
//!
//! An override here promotes that area's `policy_derivation` to `CfDocumented`. It
//! is deliberately CONSERVATIVE: an entry is added ONLY when a real CF source
//! documents the deviation, cited in the note. It is currently empty -- the CF
//! helpfiles document the cadence and scarcity rules globally (helpfiles 98/61) but
//! do not document a per-area repop-policy that departs from the area's kind -- so
//! every authored area's policy is honestly `DerivedFromKind`. The override
//! MECHANISM is proven by the corpus edge tests (a synthetic override round-trips
//! as `CfDocumented`); when a per-area CF source is found, add its entry here.

use super::area_reset_repop_types::RepopPolicy;

/// A CF-documented per-area repop-policy override.
pub(crate) struct CfDocumentedRepopOverride {
    pub policy: RepopPolicy,
    pub note: &'static str,
}

/// CF-documented per-area repop-policy overrides, keyed by `area_id`. Empty until a
/// per-area CF source is transcribed (see module docs); the derive path then marks
/// those areas `CfDocumented` instead of `DerivedFromKind`.
const CF_DOCUMENTED_REPOP_OVERRIDES: &[(&str, RepopPolicy, &str)] = &[];

/// Look up a CF-documented repop-policy override for an area, or `None` when the
/// area's policy is derived from its kind.
pub(crate) fn cf_documented_repop_override(area_id: &str) -> Option<CfDocumentedRepopOverride> {
    CF_DOCUMENTED_REPOP_OVERRIDES
        .iter()
        .find(|(id, _, _)| *id == area_id)
        .map(|(_, policy, note)| CfDocumentedRepopOverride {
            policy: *policy,
            note,
        })
}
