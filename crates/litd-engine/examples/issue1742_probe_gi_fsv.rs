//! FSV harness for #1742: baked probe-grid GI (DDGI irradiance + visibility volumes).
//!
//! Bakes an analytic scene -- a red emissive panel over a lit floor in an otherwise dark,
//! open-topped room, with an occluder wall splitting a lit region from a shadowed one -- for
//! two lighting states (day/night). It lowers each bake to a fingerprinted artifact, WRITES
//! the artifacts to disk, READS them back, verifies the fingerprint against the on-disk bytes,
//! DECODES the runtime-consumed volume, and asserts three edge-case verdicts on it:
//!
//!   1. Occlusion / no-leak: light level beside the panel strictly exceeds light level behind
//!      the occluder wall (the Chebyshev visibility term rejects the leak).
//!   2. Interpolation: the light level at a point midway between two probes is strictly between
//!      the two bracketing probe-node samples (trilinear cage interpolation).
//!   3. Day/night swap: the day bake reads strictly brighter than the night bake, in both the
//!      stealth light-level query and the renderer's directional irradiance sample.
//!
//! Every acceptance boolean is asserted (process fails non-zero) BEFORE the report is written,
//! so a false verdict can never produce a green artifact. The verdict artifact under
//! target/fsv/issue1742-probe-gi/ is the source of truth; it is read back and its sha256 is
//! reported.

use std::error::Error;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    math::{Aabb, Vec3},
    render::probe_gi::{
        BakeConfig, BakeScene, EmissivePanel, LoweredProbeVolume, PanelAxis, ProbeGridSpec,
        ProbeSampleConfig, ProbeVolume, SkyLighting, bake_probe_volume, light_level_at,
        lower_probe_volume, sample_irradiance, verify_lowered_probe_volume,
    },
};
use serde_json::json;

const MARGIN: f32 = 1.0e-3;

fn spec() -> ProbeGridSpec {
    ProbeGridSpec {
        origin: Vec3::new(0.0, 0.0, 0.0),
        spacing: Vec3::splat(1.0),
        counts: [7, 3, 5],
        irradiance_dim: 8,
        visibility_dim: 8,
    }
}

fn scene(state: &str, sky: Vec3) -> Result<BakeScene, Box<dyn Error>> {
    Ok(BakeScene::new(
        state,
        vec![EmissivePanel::new(
            PanelAxis::X,
            -0.5,
            [-1.0, -1.0],
            [3.0, 5.0],
            Vec3::new(4.0, 0.15, 0.15),
        )],
        // Occluder wall at x ~ 3.5 spanning the full cross-section: lit region is x < 3.4,
        // shadowed region is x > 3.6.
        vec![Aabb::new(
            Vec3::new(3.4, -1.0, -1.0),
            Vec3::new(3.6, 3.0, 5.0),
        )?],
        SkyLighting::new(sky, sky * 0.5),
    ))
}

fn light_level(volume: &ProbeVolume, pos: Vec3) -> Result<f32, Box<dyn Error>> {
    Ok(light_level_at(volume, pos, ProbeSampleConfig::default())?.light_level)
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1742-probe-gi")?;

    let day_sky = Vec3::new(0.30, 0.34, 0.45);
    let night_sky = Vec3::new(0.012, 0.014, 0.020);
    let config = BakeConfig::default();

    // --- Bake both lighting states offline ------------------------------------------------
    let day_bake = bake_probe_volume(spec(), &scene("day", day_sky)?, config)?;
    let night_bake = bake_probe_volume(spec(), &scene("night", night_sky)?, config)?;

    // --- Lower to fingerprinted artifacts and persist them --------------------------------
    let day_low = lower_probe_volume(&day_bake)?;
    let night_low = lower_probe_volume(&night_bake)?;
    let day_low_rb =
        run.write_json_artifact("probe-volume-day.json", &serde_json::to_value(&day_low)?)?;
    let night_low_rb = run.write_json_artifact(
        "probe-volume-night.json",
        &serde_json::to_value(&night_low)?,
    )?;

    // --- Read the artifacts back FROM DISK, verify fingerprint, decode runtime volumes ----
    let day_disk: LoweredProbeVolume = serde_json::from_slice(&std::fs::read(&day_low_rb.path)?)?;
    let night_disk: LoweredProbeVolume =
        serde_json::from_slice(&std::fs::read(&night_low_rb.path)?)?;
    let day_verify = verify_lowered_probe_volume(&day_disk)?;
    let night_verify = verify_lowered_probe_volume(&night_disk)?;
    let day = day_disk.decode()?;
    let night = night_disk.decode()?;

    // --- Determinism: an independent re-bake+re-lower reproduces the fingerprint -----------
    let day_rebake_fp =
        lower_probe_volume(&bake_probe_volume(spec(), &scene("day", day_sky)?, config)?)?
            .fingerprint;
    let fingerprint_reproducible = day_rebake_fp == day_disk.fingerprint;

    // --- Edge 1: occlusion / no-leak -------------------------------------------------------
    let p_lit = Vec3::new(0.5, 1.0, 2.0);
    let p_shadow = Vec3::new(5.0, 1.0, 2.0);
    let ll_lit = light_level(&day, p_lit)?;
    let ll_shadow = light_level(&day, p_shadow)?;
    let occlusion_no_leak = ll_lit > ll_shadow + MARGIN;

    // --- Edge 2: trilinear interpolation between probes ------------------------------------
    let p_a = Vec3::new(2.0, 1.0, 2.0);
    let p_mid = Vec3::new(2.5, 1.0, 2.0);
    let p_b = Vec3::new(3.0, 1.0, 2.0);
    let ll_a = light_level(&day, p_a)?;
    let ll_mid = light_level(&day, p_mid)?;
    let ll_b = light_level(&day, p_b)?;
    let interpolation_between =
        ll_a > ll_mid + MARGIN && ll_mid > ll_b + MARGIN && ll_mid < ll_a && ll_mid > ll_b;

    // --- Edge 3: day/night swap ------------------------------------------------------------
    let ll_day_shadow = light_level(&day, p_shadow)?;
    let ll_night_shadow = light_level(&night, p_shadow)?;
    let irr_day_up = sample_irradiance(&day, p_a, Vec3::Y, ProbeSampleConfig::default())?;
    let irr_night_up = sample_irradiance(&night, p_a, Vec3::Y, ProbeSampleConfig::default())?;
    let day_brighter_than_night = ll_day_shadow > ll_night_shadow + MARGIN
        && irr_day_up.length() > irr_night_up.length() + MARGIN;

    // --- Directional irradiance: red toward the panel dominates ----------------------------
    let irr_toward = sample_irradiance(&day, p_lit, -Vec3::X, ProbeSampleConfig::default())?;
    let panel_is_red = irr_toward.x > irr_toward.y + MARGIN && irr_toward.x > irr_toward.z + MARGIN;

    // --- Gate the process on EVERY verdict BEFORE writing the report ----------------------
    assert!(
        day_verify.fingerprint == day_low.fingerprint
            && night_verify.fingerprint == night_low.fingerprint,
        "on-disk lowered artifact fingerprint must match the in-memory lower()"
    );
    assert!(
        fingerprint_reproducible,
        "re-bake fingerprint {day_rebake_fp} must equal persisted {}",
        day_disk.fingerprint
    );
    assert!(
        occlusion_no_leak,
        "edge1 leak: lit {ll_lit} must exceed shadowed {ll_shadow}"
    );
    assert!(
        interpolation_between,
        "edge2 interp: mid {ll_mid} must lie strictly between {ll_a} and {ll_b}"
    );
    assert!(
        day_brighter_than_night,
        "edge3 day/night: day (ll {ll_day_shadow}, irr {}) must exceed night (ll {ll_night_shadow}, irr {})",
        irr_day_up.length(),
        irr_night_up.length()
    );
    assert!(
        panel_is_red,
        "directional irradiance toward the red panel must be red-dominant: {irr_toward:?}"
    );

    // --- Write the verdict artifact, then read it back ------------------------------------
    let report = json!({
        "issue": 1742,
        "schema": "litd.fsv.issue1742_probe_gi.v1",
        "grid": {
            "counts": day.spec.counts,
            "probe_count": day.probes.len(),
            "irradiance_dim": day.spec.irradiance_dim,
            "visibility_dim": day.spec.visibility_dim,
        },
        "lowered": {
            "day_fingerprint": day_disk.fingerprint,
            "night_fingerprint": night_disk.fingerprint,
            "day_artifact_sha256": day_low_rb.sha256,
            "night_artifact_sha256": night_low_rb.sha256,
            "fingerprint_reproducible": fingerprint_reproducible,
            "day_irradiance_words": day_verify.irradiance_word_count,
            "day_visibility_words": day_verify.visibility_word_count,
        },
        "edge1_occlusion_no_leak": {
            "pass": occlusion_no_leak,
            "light_level_beside_panel": ll_lit,
            "light_level_behind_wall": ll_shadow,
        },
        "edge2_interpolation": {
            "pass": interpolation_between,
            "light_level_probe_a": ll_a,
            "light_level_midpoint": ll_mid,
            "light_level_probe_b": ll_b,
        },
        "edge3_day_night_swap": {
            "pass": day_brighter_than_night,
            "light_level_day": ll_day_shadow,
            "light_level_night": ll_night_shadow,
            "irradiance_up_day": irr_day_up.length(),
            "irradiance_up_night": irr_night_up.length(),
        },
        "directional_irradiance_toward_panel": [irr_toward.x, irr_toward.y, irr_toward.z],
        "all_verdicts_pass": true,
    });
    let readback = run.write_report(&report)?;

    // Independent re-read of the persisted verdict.
    let persisted: serde_json::Value = serde_json::from_slice(&std::fs::read(&readback.path)?)?;
    if persisted["all_verdicts_pass"] != json!(true) {
        return Err("verdict artifact readback did not confirm all_verdicts_pass".into());
    }

    println!(
        "ISSUE1742_PROBE_GI_FSV report={} sha256={} probes={} \
         edge1_no_leak={} edge2_interp={} edge3_day_night={} fingerprint_reproducible={}",
        readback.path.display(),
        readback.sha256,
        day.probes.len(),
        occlusion_no_leak,
        interpolation_between,
        day_brighter_than_night,
        fingerprint_reproducible,
    );
    Ok(())
}
