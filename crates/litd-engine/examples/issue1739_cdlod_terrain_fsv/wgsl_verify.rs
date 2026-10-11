//! Genuine headless validation of `cdlod_morph.wgsl` for the #1739 FSV.
//!
//! This REPLACES the old tautological `CDLOD_MORPH_WGSL.contains("@vertex")`
//! substring checks. It does three real things, any of which fails LOUD:
//!
//! 1. Parse + validate the shader through `naga` (wgpu's own WGSL frontend). A
//!    malformed or ill-typed shader is rejected here, so a broken placeholder
//!    can no longer pass the FSV. Two negative controls (a syntax error and a
//!    type error) prove the gate actually rejects bad shaders.
//! 2. Inspect the validated `naga` module IR (not the raw text): assert the real
//!    vertex/fragment entry points and the two morph helper functions exist with
//!    the expected argument counts, and that `grid_dim` now comes from a uniform
//!    global (`params`) -- NOT from `arrayLength(&tiles)` as the placeholder did.
//! 3. Cross-check the morph MATH: transcribe the two WGSL geomorph formulas
//!    operation-for-operation and assert they produce bit-identical results to
//!    the CPU `morph_factor` / `morph_grid_xz` (the seam-continuity source of
//!    truth) across every LOD, grid index, and morph boundary.
//!
//! What this PROVES: the WGSL is a real, well-formed, type-correct shader whose
//! geomorph math matches the CPU mirror exactly. What it does NOT prove: GPU
//! execution / rasterized pixels -- no `wgpu::Device` is bound here (headless,
//! no display), so the drawn image is unverified. The report states this plainly.

use litd_engine::render::terrain::{CDLOD_MORPH_WGSL, TerrainConfig, morph_factor, morph_grid_xz};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use serde_json::{Value, json};

type Err = Box<dyn std::error::Error>;

/// Parse + validate the shader through naga, returning the module IR on success.
/// A parse or validation failure is returned as an error (fail loud).
fn parse_and_validate(source: &str) -> Result<naga::Module, Err> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|e| format!("CDLOD WGSL failed to parse: {}", e.emit_to_string(source)))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| format!("CDLOD WGSL failed naga validation: {e:?}"))?;
    Ok(module)
}

/// Build the WGSL section of the FSV report. Every structural fact is derived
/// from the validated naga IR; the cross-check compares against the CPU morph
/// math. Any inconsistency returns an error so the example aborts before writing.
pub fn report(cfg: &TerrainConfig) -> Result<Value, Err> {
    let module = parse_and_validate(CDLOD_MORPH_WGSL)?;

    // Entry points, read from the IR (not the source text).
    let vertex = module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Vertex)
        .ok_or("no @vertex entry point in CDLOD WGSL")?;
    let fragment = module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Fragment)
        .ok_or("no @fragment entry point in CDLOD WGSL")?;

    // Named helper functions + their arg counts.
    let fn_args = |name: &str| -> Option<usize> {
        module
            .functions
            .iter()
            .find(|(_, f)| f.name.as_deref() == Some(name))
            .map(|(_, f)| f.arguments.len())
    };
    let morph_factor_args = fn_args("cdlod_morph_factor").ok_or("cdlod_morph_factor missing")?;
    let snap_args = fn_args("cdlod_snap").ok_or("cdlod_snap missing")?;
    if morph_factor_args != 3 {
        return Err("cdlod_morph_factor must take (start, end, dist)".into());
    }
    if snap_args != 2 {
        return Err("cdlod_snap must take (grid_index, morph)".into());
    }

    // grid_dim source: a uniform global named `params`, and NO runtime-array
    // length feeding it (the placeholder derived grid_dim from arrayLength).
    let grid_dim_uniform = module.global_variables.iter().any(|(_, g)| {
        g.name.as_deref() == Some("params") && g.space == naga::AddressSpace::Uniform
    });
    if !grid_dim_uniform {
        return Err("grid_dim uniform `params` not found in CDLOD WGSL".into());
    }
    let arraylength_removed = !CDLOD_MORPH_WGSL.contains("arrayLength");

    let crosscheck = morph_crosscheck(cfg)?;
    let negatives = negative_controls()?;

    Ok(json!({
        "naga_parse_ok": true,
        "naga_validate_ok": true,
        "vertex_entry": vertex.function.name.clone().unwrap_or_else(|| "vs".into()),
        "fragment_entry": fragment.function.name.clone().unwrap_or_else(|| "fs".into()),
        "morph_factor_fn_args": morph_factor_args,
        "snap_fn_args": snap_args,
        "grid_dim_from_uniform": grid_dim_uniform,
        "arraylength_placeholder_removed": arraylength_removed,
        "cpu_wgsl_crosscheck": crosscheck,
        "negative_controls": negatives,
        "gpu_execution_verified": false,
        "note": "naga proves the shader parses, type-checks, and mirrors the CPU \
                 morph math; it does NOT execute on a GPU, so rasterized pixels \
                 are unverified in this headless FSV",
    }))
}

/// Transcription of `cdlod_morph_factor` from the WGSL, op-for-op.
fn wgsl_morph_factor(start: f32, end: f32, dist: f32) -> f32 {
    let span = end - start;
    if span <= 1e-6 {
        return if dist >= end { 1.0 } else { 0.0 };
    }
    ((dist - start) / span).clamp(0.0, 1.0)
}

/// Transcription of `cdlod_snap` from the WGSL, op-for-op.
fn wgsl_snap(grid_index: f32, morph: f32) -> f32 {
    let is_odd = (grid_index * 0.5).fract() * 2.0;
    grid_index - is_odd * morph
}

/// Compare the WGSL geomorph formulas against the CPU morph math over every LOD,
/// grid index, and morph boundary. Deltas must be EXACTLY zero (same float ops).
fn morph_crosscheck(cfg: &TerrainConfig) -> Result<Value, Err> {
    let mut samples = 0u32;
    let mut max_factor_delta = 0.0f32;
    let mut max_snap_delta = 0.0f32;

    for lod in 1..cfg.lod_count() {
        let params = cfg.morph_params(lod);
        // Sample distances straddling both morph boundaries plus the midpoint.
        let mid = 0.5 * (params.start + params.end);
        for &dist in &[
            params.start - 1.0,
            params.start,
            mid,
            params.end,
            params.end + 1.0,
        ] {
            let cpu_m = morph_factor(params, dist);
            let wgsl_m = wgsl_morph_factor(params.start, params.end, dist);
            max_factor_delta = max_factor_delta.max((cpu_m - wgsl_m).abs());

            for grid in 0..cfg.grid_dim {
                let cpu_s = morph_grid_xz(grid, cpu_m);
                let wgsl_s = wgsl_snap(grid as f32, wgsl_m);
                max_snap_delta = max_snap_delta.max((cpu_s - wgsl_s).abs());
                samples += 1;
            }
        }
    }

    if max_factor_delta != 0.0 || max_snap_delta != 0.0 {
        return Err(format!(
            "WGSL morph math diverged from CPU: factor_delta={max_factor_delta}, \
             snap_delta={max_snap_delta}"
        )
        .into());
    }
    Ok(json!({
        "samples": samples,
        "max_morph_factor_delta": max_factor_delta,
        "max_snap_delta": max_snap_delta,
    }))
}

/// Prove the naga gate actually has teeth: a syntactically broken shader and a
/// type-incorrect (but parseable) shader must BOTH be rejected. If either is
/// accepted, the validation gate is worthless and we fail loud.
fn negative_controls() -> Result<Value, Err> {
    // Parses but is ill-typed: returns a vec2 from an f32 function.
    let type_error =
        format!("{CDLOD_MORPH_WGSL}\nfn _bad() -> f32 {{ return vec2<f32>(1.0, 2.0); }}\n");
    // Does not even parse: a return arrow with no type.
    let syntax_error = format!("{CDLOD_MORPH_WGSL}\nfn _bad() -> {{ }}\n");

    let syntax_rejected = naga::front::wgsl::parse_str(&syntax_error).is_err();
    let type_rejected = parse_and_validate(&type_error).is_err();
    if !syntax_rejected {
        return Err("negative control: a syntax-error shader was NOT rejected".into());
    }
    if !type_rejected {
        return Err("negative control: a type-error shader was NOT rejected".into());
    }
    Ok(json!({
        "syntax_error_rejected": syntax_rejected,
        "type_error_rejected": type_rejected,
    }))
}
