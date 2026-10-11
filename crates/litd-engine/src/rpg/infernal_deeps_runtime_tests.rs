//! Unit coverage for the #1455 Infernal Deeps runtime tuning consumer.
//!
//! The full author -> freeze -> read-tuning -> admit success path (which needs the real measurement
//! panel) is proven end-to-end by `examples/issue1455_infernal_deeps_runtime_slice_fsv.rs` against
//! the CANONICAL vault. These unit tests pin two contracts: the pure admission mechanic against a
//! known tuning, and the loud-miss error when the frozen table lacks a required row.

use super::{InfernalDeepsRuntimeTuning, WitheringRaidAdmission, admit_withering_raid};
use crate::calyx_content_runtime::{ENGINE_FROZEN_CONTENT_TABLE_SCHEMA, FrozenContentTable};

fn tuning(min_party: i64, min_level: i64) -> InfernalDeepsRuntimeTuning {
    InfernalDeepsRuntimeTuning {
        withering_minimum_party_size: min_party,
        withering_minimum_character_level: min_level,
        withering_raid_phase_count: 6,
        zone_count: 7,
        boss_count: 7,
        relic_vault_count: 2,
    }
}

#[test]
fn admits_party_meeting_both_thresholds() {
    let admission = admit_withering_raid(&tuning(8, 47), 8, 47);
    assert_eq!(admission, WitheringRaidAdmission::Admitted);
}

#[test]
fn denies_party_below_minimum_size_with_threshold() {
    let admission = admit_withering_raid(&tuning(8, 47), 7, 51);
    assert_eq!(
        admission,
        WitheringRaidAdmission::DeniedPartyTooSmall {
            required: 8,
            actual: 7,
        }
    );
}

#[test]
fn denies_underleveled_party_with_threshold() {
    // Party is large enough but the raiders are under the frozen minimum character level.
    let admission = admit_withering_raid(&tuning(8, 47), 9, 46);
    assert_eq!(
        admission,
        WitheringRaidAdmission::DeniedUnderLeveled {
            required: 47,
            actual: 46,
        }
    );
}

#[test]
fn raising_the_frozen_minimum_moves_the_admission_boundary() {
    // The same 8-person party that a min-8 tuning admits is denied once the authored minimum is
    // re-authored to 10 -- the boundary is data, not a compiled literal.
    assert_eq!(
        admit_withering_raid(&tuning(8, 47), 8, 47),
        WitheringRaidAdmission::Admitted
    );
    assert_eq!(
        admit_withering_raid(&tuning(10, 47), 8, 47),
        WitheringRaidAdmission::DeniedPartyTooSmall {
            required: 10,
            actual: 8,
        }
    );
}

#[test]
fn from_frozen_errors_loud_when_row_absent() {
    // An empty frozen table has no Infernal Deeps rows: reading tuning must fail loudly with the
    // structured field path, never hand back a default that a mechanic would silently trust.
    let empty = FrozenContentTable {
        schema: ENGINE_FROZEN_CONTENT_TABLE_SCHEMA.to_string(),
        rows: Vec::new(),
    };
    let error = InfernalDeepsRuntimeTuning::from_frozen(&empty).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("infernal_deeps_runtime")
            && message.contains("server.ashen_reach.infernal_deeps.withering_raid_path"),
        "expected a structured missing-row error, got {message}"
    );
}
