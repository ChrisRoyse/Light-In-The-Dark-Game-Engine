//! Unit tests for the editor min-spec model (#1793). Pure, no GPU required.

use super::*;

fn budget(
    cold_start_ns: u128,
    frame_p50: u128,
    working_set: u64,
    vram: u64,
) -> EditorMinSpecBudget {
    let degrade = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    EditorMinSpecBudget {
        adapter_class: AdapterClass::Integrated,
        adapter_name: "Test Integrated".to_string(),
        backend: "Vulkan".to_string(),
        forced_preference: "force-integrated".to_string(),
        degrade,
        base_surface_width: 1600,
        base_surface_height: 900,
        internal_render_width: 800,
        internal_render_height: 450,
        cold_start_ns,
        startup_working_set_bytes: working_set,
        derived_vram_footprint_bytes: vram,
        interactive_frame_ns_p50: frame_p50,
        interactive_frame_ns_max: frame_p50 + 1,
        structural_edit_apply_ns: frame_p50 * 2,
        frames_measured: 30,
        ceilings: EditorMinSpecCeilings::editor_min_spec(),
    }
}

#[test]
fn adapter_class_classifies_device_type_strings() {
    assert_eq!(
        AdapterClass::from_device_type_str("DiscreteGpu"),
        AdapterClass::Discrete
    );
    assert_eq!(
        AdapterClass::from_device_type_str("IntegratedGpu"),
        AdapterClass::Integrated
    );
    assert_eq!(
        AdapterClass::from_device_type_str("VirtualGpu"),
        AdapterClass::Virtual
    );
    assert_eq!(AdapterClass::from_device_type_str("Cpu"), AdapterClass::Cpu);
    assert_eq!(
        AdapterClass::from_device_type_str("Other"),
        AdapterClass::Unknown
    );
}

#[test]
fn discrete_defaults_are_full_quality() {
    let d = EditorDegradeSettings::defaults_for(AdapterClass::Discrete);
    d.validate().unwrap();
    assert_eq!(d.shadow_atlas_size, 4096);
    assert_eq!(d.viewport_resolution_scale_percent, 100);
}

#[test]
fn non_discrete_classes_default_to_min_spec_floor() {
    for class in [
        AdapterClass::Integrated,
        AdapterClass::Cpu,
        AdapterClass::Virtual,
        AdapterClass::Unknown,
    ] {
        let d = EditorDegradeSettings::defaults_for(class);
        d.validate().unwrap();
        assert_eq!(d.shadow_atlas_size, 1024, "class {class:?}");
        assert_eq!(d.viewport_resolution_scale_percent, 75, "class {class:?}");
    }
}

#[test]
fn forced_low_tier_is_the_degrade_floor() {
    let d = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    d.validate().unwrap();
    assert_eq!(d.shadow_atlas_size, 512);
    assert_eq!(d.viewport_resolution_scale_percent, 50);
}

#[test]
fn non_power_of_two_atlas_fails_loud() {
    let d = EditorDegradeSettings {
        adapter_class: AdapterClass::Integrated,
        shadow_atlas_size: 1000,
        viewport_resolution_scale_percent: 75,
    };
    match d.validate() {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert_eq!(field, "editor_min_spec.degrade.shadow_atlas_size");
        }
        other => panic!("expected fail-loud, got {other:?}"),
    }
}

#[test]
fn out_of_range_atlas_fails_loud() {
    let d = EditorDegradeSettings {
        adapter_class: AdapterClass::Integrated,
        shadow_atlas_size: 16384,
        viewport_resolution_scale_percent: 75,
    };
    assert!(d.validate().is_err());
}

#[test]
fn out_of_range_viewport_scale_fails_loud() {
    for bad in [0, 24, 101, 200] {
        let d = EditorDegradeSettings {
            adapter_class: AdapterClass::Integrated,
            shadow_atlas_size: 1024,
            viewport_resolution_scale_percent: bad,
        };
        match d.validate() {
            Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
                assert_eq!(
                    field,
                    "editor_min_spec.degrade.viewport_resolution_scale_percent"
                );
            }
            other => panic!("expected fail-loud for {bad}, got {other:?}"),
        }
    }
}

#[test]
fn scaled_dimensions_apply_the_percent() {
    let d = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    assert_eq!(d.scaled_dimensions(1600, 900).unwrap(), (800, 450));
    let full = EditorDegradeSettings::defaults_for(AdapterClass::Discrete);
    assert_eq!(full.scaled_dimensions(1600, 900).unwrap(), (1600, 900));
}

#[test]
fn scaled_dimensions_reject_zero_surface() {
    let d = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    assert!(d.scaled_dimensions(0, 900).is_err());
}

#[test]
fn derived_vram_footprint_is_deterministic_and_scale_sensitive() {
    let low = EditorDegradeSettings::forced_low_tier(AdapterClass::Integrated);
    let high = EditorDegradeSettings::defaults_for(AdapterClass::Discrete);
    let low_bytes = low.derived_vram_footprint_bytes(1600, 900).unwrap();
    let high_bytes = high.derived_vram_footprint_bytes(1600, 900).unwrap();
    // Forced low tier must reserve strictly less VRAM than full discrete quality.
    assert!(
        low_bytes < high_bytes,
        "low {low_bytes} >= high {high_bytes}"
    );
    // Deterministic: 512 atlas + 800x450 color/depth/hdr.
    let atlas = 512u64 * 512 * 4;
    let pixels = 800u64 * 450;
    assert_eq!(low_bytes, atlas + pixels * 4 + pixels * 4 + pixels * 8);
}

#[test]
fn budget_within_ceilings_passes() {
    let b = budget(
        2_000_000_000,
        5_000_000,
        512 * 1024 * 1024,
        64 * 1024 * 1024,
    );
    b.validate_within_budget().unwrap();
}

#[test]
fn budget_over_frame_ceiling_fails_loud() {
    let b = budget(
        2_000_000_000,
        40_000_000,
        512 * 1024 * 1024,
        64 * 1024 * 1024,
    );
    match b.validate_within_budget() {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert_eq!(field, "editor_min_spec.budget.interactive_frame_ns_p50");
        }
        other => panic!("expected frame-budget breach, got {other:?}"),
    }
}

#[test]
fn budget_over_working_set_ceiling_fails_loud() {
    let b = budget(
        2_000_000_000,
        5_000_000,
        4 * 1024 * 1024 * 1024,
        64 * 1024 * 1024,
    );
    assert!(b.validate_within_budget().is_err());
}

#[cfg(windows)]
#[test]
fn process_working_set_is_a_real_positive_number() {
    let bytes = process_working_set_bytes().unwrap();
    assert!(bytes > 0, "working set must be a real positive measurement");
}
