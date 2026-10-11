//! #1537 FSV: replace ED12-play-topdown.png without omniscient navigation.
//!
//! Source of truth:
//! 1. `build_reference_scene_plan` UI overlay policy must be RPG no-map
//! 2. GPU/CPU render of the reference scene (real render path)
//! 3. Written PNG at `docs/readme/ED12-play-topdown.png` (independent re-read)
//! 4. Doctrine TSV row flipped to RUNTIME_CLEAN
//!
//! Edges: invalid size fails loud; blank image fails quality bands; policy
//! surface_visible must be false for no-map.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use litd_engine::render::reference::{
    ReferenceSceneSettings, build_reference_scene_plan, compare_reference_scene_image,
    render_reference_scene_gpu, render_reference_scene_image,
};
use litd_engine::render::ui::{MapOverlayPolicyMode, UiPassKind};
use serde_json::json;

const ED12_REL: &str = "docs/readme/ED12-play-topdown.png";
const DOCTRINE_TSV: &str = "scripts/reference-artifact-doctrine.tsv";
const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1537_ed12_doctrine_nav")?;
    let root = repo_root();
    let ed12_path = root.join(ED12_REL);
    let doctrine_path = root.join(DOCTRINE_TSV);
    let before_bytes = if ed12_path.exists() {
        fs::metadata(&ed12_path)?.len()
    } else {
        0
    };
    let before_sha = if ed12_path.exists() {
        certutil_sha256(&ed12_path)?
    } else {
        String::new()
    };

    // Happy path: plan uses RPG no-map policy (minimap pass suppressed).
    let settings = ReferenceSceneSettings {
        width: WIDTH,
        height: HEIGHT,
    };
    let plan = build_reference_scene_plan(settings)?;
    let ui = &plan.ui_plan;
    let no_minimap_pass = !ui.passes.iter().any(|p| *p == UiPassKind::Minimap);
    let surface_visible = ui.minimap.policy.surface_visible;
    let policy_mode = ui.minimap.policy.mode;
    let policy_ok = !surface_visible
        && policy_mode == MapOverlayPolicyMode::RuntimeRpgNoMapAccess
        && no_minimap_pass
        && ui.minimap.markers.is_empty();
    println!(
        "issue1537 plan_policy mode={policy_mode:?} surface_visible={surface_visible} \
         no_minimap_pass={no_minimap_pass} marker_count={} policy_ok={policy_ok}",
        ui.minimap.markers.len()
    );
    if !policy_ok {
        return Err("reference scene still carries omniscient navigation".into());
    }

    // CPU render + quality bands.
    let cpu_image = render_reference_scene_image(&plan)?;
    let cpu_cmp = compare_reference_scene_image(&cpu_image);
    if !cpu_cmp.pass {
        return Err(format!("cpu visual bands failed: {}", cpu_cmp.band_summary()).into());
    }
    let cpu_bmp = run.run_dir().join("ed12-cpu-960x540.bmp");
    cpu_image.write_bmp(&cpu_bmp)?;

    // GPU render (real adapter path).
    let gpu = pollster::block_on(render_reference_scene_gpu(settings))?;
    if !gpu.comparison.pass {
        return Err(format!("gpu visual bands failed: {}", gpu.comparison.band_summary()).into());
    }
    let gpu_bmp = run.run_dir().join("ed12-gpu-960x540.bmp");
    gpu.image.write_bmp(&gpu_bmp)?;

    // Write PNG into docs/readme (SoT for README floor) via System.Drawing.
    let staging_png = run.run_dir().join("ED12-play-topdown.staging.png");
    bmp_to_png(&gpu_bmp, &staging_png)?;
    if !staging_png.exists() || fs::metadata(&staging_png)?.len() < 1024 {
        return Err("staging PNG missing or too small".into());
    }
    fs::copy(&staging_png, &ed12_path)?;
    let after_bytes = fs::metadata(&ed12_path)?.len();
    let after_sha = certutil_sha256(&ed12_path)?;
    let staging_sha = certutil_sha256(&staging_png)?;
    if after_sha != staging_sha {
        return Err("ED12 path sha256 does not match staging write".into());
    }
    println!(
        "issue1537 wrote {ED12_REL} before_bytes={before_bytes} after_bytes={after_bytes} \
         before_sha={before_sha} after_sha={after_sha}"
    );

    // Doctrine TSV flip to RUNTIME_CLEAN for ED12.
    let doctrine_before = fs::read_to_string(&doctrine_path)?;
    let doctrine_after = flip_ed12_doctrine_row(&doctrine_before)?;
    if doctrine_after != doctrine_before {
        fs::write(&doctrine_path, &doctrine_after)?;
    }
    let doctrine_readback = fs::read_to_string(&doctrine_path)?;
    let ed12_row = doctrine_readback
        .lines()
        .find(|l| l.starts_with("docs/readme/ED12-play-topdown.png\t"))
        .ok_or("ED12 row missing from doctrine TSV")?
        .to_string();
    let doctrine_ok = ed12_row.contains("\tRUNTIME_CLEAN\t");
    println!("issue1537 doctrine_row={ed12_row} doctrine_ok={doctrine_ok}");
    if !doctrine_ok {
        return Err("ED12 doctrine row not RUNTIME_CLEAN after flip".into());
    }

    // Edges
    let edge_invalid = build_reference_scene_plan(ReferenceSceneSettings {
        width: 0,
        height: HEIGHT,
    });
    let edge_invalid_ok = edge_invalid.is_err();
    let edge_blank = {
        let blank = litd_engine::render::reference::ReferenceSceneImage::from_rgba_pixels(
            320,
            180,
            vec![[0, 0, 0, 255]; 320 * 180],
        )?;
        !compare_reference_scene_image(&blank).pass
    };
    let edge_policy = policy_mode == MapOverlayPolicyMode::RuntimeRpgNoMapAccess;

    let report = json!({
        "schema": "litd.engine.issue1537.ed12_doctrine_nav_fsv.v1",
        "issue": 1537,
        "parents": [944, 1510, 1558, 1147],
        "verdict": if policy_ok && doctrine_ok && edge_invalid_ok && edge_blank && edge_policy { "GREEN" } else { "RED" },
        "sufficiency_argument": "Reference scene plan policy is the engine SoT for HUD nav surfaces. GPU-rendered PNG replaces docs/readme/ED12-play-topdown.png; independent certutil sha256 of the written file matches staging. Doctrine TSV is the classification SoT and is flipped to RUNTIME_CLEAN only after the PNG write.",
        "plan": {
            "policy_mode": format!("{policy_mode:?}"),
            "surface_visible": surface_visible,
            "no_minimap_pass": no_minimap_pass,
            "marker_count": ui.minimap.markers.len(),
            "policy_ok": policy_ok,
        },
        "render": {
            "cpu_bands_pass": cpu_cmp.pass,
            "gpu_bands_pass": gpu.comparison.pass,
            "gpu_adapter": gpu.adapter_name,
            "gpu_backend": gpu.backend,
            "draw_calls": gpu.draw_calls,
            "cpu_bmp": cpu_bmp.display().to_string(),
            "gpu_bmp": gpu_bmp.display().to_string(),
        },
        "png_sot": {
            "path": ed12_path.display().to_string(),
            "before_bytes": before_bytes,
            "after_bytes": after_bytes,
            "before_sha256": before_sha,
            "after_sha256": after_sha,
            "staging_sha256": staging_sha,
        },
        "doctrine": {
            "path": doctrine_path.display().to_string(),
            "row": ed12_row,
            "runtime_clean": doctrine_ok,
        },
        "edges": {
            "invalid_size_fails": edge_invalid_ok,
            "blank_fails_bands": edge_blank,
            "policy_is_rpg_no_map": edge_policy,
        },
    });
    let readback = run.write_report(&report)?;
    let verdict = report["verdict"].as_str() == Some("GREEN");
    println!(
        "ISSUE1537_ED12_DOCTRINE_NAV_FSV verdict={} report={} report_sha256={} ed12_sha256={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        after_sha
    );
    if !verdict {
        return Err("ISSUE1537 ED12 doctrine nav FSV RED".into());
    }
    Ok(())
}

fn flip_ed12_doctrine_row(tsv: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut found = false;
    let mut out = String::with_capacity(tsv.len());
    for line in tsv.lines() {
        if line.starts_with("docs/readme/ED12-play-topdown.png\t") {
            // artifact TAB status TAB annotation TAB refs
            out.push_str("docs/readme/ED12-play-topdown.png\tRUNTIME_CLEAN\t-\t#1537,#944,#1510\n");
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        return Err("ED12 row not found in doctrine TSV".into());
    }
    Ok(out)
}

fn bmp_to_png(bmp: &Path, png: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // System.Drawing rejects `\\?\` extended paths; normalize to plain absolute paths.
    let bmp_s = normalize_win_path(bmp);
    let png_s = normalize_win_path(png);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Drawing
$bmpPath = [System.IO.Path]::GetFullPath('{bmp}')
$pngPath = [System.IO.Path]::GetFullPath('{png}')
$img = [System.Drawing.Image]::FromFile($bmpPath)
try {{
  $dir = Split-Path -Parent $pngPath
  if (-not (Test-Path -LiteralPath $dir)) {{ New-Item -ItemType Directory -Force -Path $dir | Out-Null }}
  $img.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
}} finally {{
  $img.Dispose()
}}
if (-not (Test-Path -LiteralPath $pngPath)) {{ throw "png not written: $pngPath" }}
Write-Output "PNG_OK bytes=$((Get-Item -LiteralPath $pngPath).Length)"
"#,
        bmp = bmp_s.replace('\'', "''"),
        png = png_s.replace('\'', "''"),
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "System.Drawing BMP to PNG failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn normalize_win_path(path: &Path) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).replace('/', "\\")
}

fn certutil_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()?;
    if !out.status.success() {
        return Err(format!("certutil failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    let text = String::from_utf8(out.stdout)?;
    let hash = text
        .lines()
        .nth(1)
        .ok_or("certutil missing hash line")?
        .split_whitespace()
        .collect::<String>()
        .to_lowercase();
    Ok(hash)
}
