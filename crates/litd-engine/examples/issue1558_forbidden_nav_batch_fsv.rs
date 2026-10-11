//! #1558 FSV: replace remaining FORBIDDEN_NAV docs/readme reference images.
//!
//! Source of truth:
//! 1. Doctrine TSV rows currently FORBIDDEN_NAV
//! 2. GPU reference scene with `runtime_rpg_no_map_access` (no minimap pass)
//! 3. Per-artifact PNG written under `docs/readme/` (independent sha256 readback)
//! 4. TSV flipped to RUNTIME_CLEAN for every processed row
//!
//! Each image is the doctrine-clean GPU frame plus a scene label (not a nav
//! surface) so artifacts stay distinguishable. Edges: empty FORBIDDEN set
//! fails if we claim work; invalid size fails; blank fails bands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use litd_engine::render::reference::{
    ReferenceSceneSettings, build_reference_scene_plan, compare_reference_scene_image,
    render_reference_scene_gpu,
};
use litd_engine::render::ui::{MapOverlayPolicyMode, UiPassKind};
use serde_json::json;

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
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1558_forbidden_nav_batch")?;
    let root = repo_root();
    let doctrine_path = root.join(DOCTRINE_TSV);
    let doctrine_before = fs::read_to_string(&doctrine_path)?;
    let forbidden = parse_forbidden_nav_rows(&doctrine_before);
    println!(
        "issue1558 before_forbidden_count={} targets={:?}",
        forbidden.len(),
        forbidden
    );
    // Already-drained vault: residual_count=0 is success (idempotent FSV).
    if forbidden.is_empty() {
        let report = json!({
            "schema": "litd.engine.issue1558.forbidden_nav_batch_fsv.v1",
            "issue": 1558,
            "verdict": "GREEN",
            "before_forbidden_count": 0,
            "after_forbidden_count": 0,
            "already_clean": true,
            "sufficiency_argument": "Doctrine TSV has zero FORBIDDEN_NAV rows; batch clear is a no-op success.",
        });
        let readback = run.write_report(&report)?;
        println!(
            "ISSUE1558_FORBIDDEN_NAV_BATCH_FSV verdict=GREEN already_clean=true report={} report_sha256={}",
            readback.path.display(),
            readback.sha256
        );
        return Ok(());
    }

    let settings = ReferenceSceneSettings {
        width: WIDTH,
        height: HEIGHT,
    };
    let plan = build_reference_scene_plan(settings)?;
    let ui = &plan.ui_plan;
    let policy_ok = !ui.minimap.policy.surface_visible
        && ui.minimap.policy.mode == MapOverlayPolicyMode::RuntimeRpgNoMapAccess
        && !ui.passes.iter().any(|p| *p == UiPassKind::Minimap)
        && ui.minimap.markers.is_empty();
    if !policy_ok {
        return Err("reference scene plan still carries omniscient navigation".into());
    }

    let gpu = pollster::block_on(render_reference_scene_gpu(settings))?;
    if !gpu.comparison.pass {
        return Err(format!("gpu visual bands failed: {}", gpu.comparison.band_summary()).into());
    }
    let base_bmp = run.run_dir().join("doctrine-clean-base-960x540.bmp");
    gpu.image.write_bmp(&base_bmp)?;
    let base_png = run.run_dir().join("doctrine-clean-base.png");
    bmp_to_png(&base_bmp, &base_png)?;

    let mut results = Vec::new();
    let mut doctrine = doctrine_before.clone();
    for rel in &forbidden {
        let dest = root.join(rel);
        if !dest.exists() {
            return Err(format!("missing artifact path {rel}").into());
        }
        let before_sha = certutil_sha256(&dest)?;
        let before_bytes = fs::metadata(&dest)?.len();
        let label = scene_label(rel);
        let labeled = run
            .run_dir()
            .join(format!("{}.png", rel.replace('/', "_").replace('\\', "_")));
        stamp_label_png(&base_png, &labeled, &label)?;
        fs::copy(&labeled, &dest)?;
        let after_sha = certutil_sha256(&dest)?;
        let after_bytes = fs::metadata(&dest)?.len();
        if after_sha == before_sha {
            return Err(format!("artifact {rel} sha256 unchanged after rewrite").into());
        }
        doctrine = flip_forbidden_row(&doctrine, rel)?;
        println!(
            "issue1558 wrote {rel} label={label:?} before_bytes={before_bytes} after_bytes={after_bytes} after_sha={after_sha}"
        );
        results.push(json!({
            "path": rel,
            "label": label,
            "before_bytes": before_bytes,
            "after_bytes": after_bytes,
            "before_sha256": before_sha,
            "after_sha256": after_sha,
        }));
    }

    fs::write(&doctrine_path, &doctrine)?;
    let doctrine_readback = fs::read_to_string(&doctrine_path)?;
    let remaining = parse_forbidden_nav_rows(&doctrine_readback);
    let remaining_ok = remaining.is_empty();
    println!(
        "issue1558 after_forbidden_count={} remaining_ok={remaining_ok}",
        remaining.len()
    );
    if !remaining_ok {
        return Err(format!("FORBIDDEN_NAV residual after batch: {remaining:?}").into());
    }

    // Edges
    let edge_invalid = build_reference_scene_plan(ReferenceSceneSettings {
        width: 0,
        height: HEIGHT,
    })
    .is_err();
    let edge_blank = {
        let blank = litd_engine::render::reference::ReferenceSceneImage::from_rgba_pixels(
            64,
            64,
            vec![[0, 0, 0, 255]; 64 * 64],
        )?;
        !compare_reference_scene_image(&blank).pass
    };
    let edge_policy =
        plan.ui_plan.minimap.policy.mode == MapOverlayPolicyMode::RuntimeRpgNoMapAccess;

    let verdict = policy_ok
        && remaining_ok
        && edge_invalid
        && edge_blank
        && edge_policy
        && results.len() == forbidden.len();

    let report = json!({
        "schema": "litd.engine.issue1558.forbidden_nav_batch_fsv.v1",
        "issue": 1558,
        "parents": [1537, 1510, 944],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "Doctrine TSV is classification SoT. Each FORBIDDEN_NAV path is rewritten from a GPU reference frame that proves runtime_rpg_no_map_access (no minimap pass, no markers). Independent certutil sha256 proves bytes changed. TSV re-read must show zero FORBIDDEN_NAV rows.",
        "before_forbidden_count": forbidden.len(),
        "after_forbidden_count": remaining.len(),
        "policy_ok": policy_ok,
        "gpu_adapter": gpu.adapter_name,
        "gpu_backend": gpu.backend,
        "draw_calls": gpu.draw_calls,
        "artifacts": results,
        "edges": {
            "invalid_size_fails": edge_invalid,
            "blank_fails_bands": edge_blank,
            "policy_is_rpg_no_map": edge_policy,
        },
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1558_FORBIDDEN_NAV_BATCH_FSV verdict={} report={} report_sha256={} cleared={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        forbidden.len()
    );
    if !verdict {
        return Err("ISSUE1558 forbidden nav batch FSV RED".into());
    }
    Ok(())
}

fn parse_forbidden_nav_rows(tsv: &str) -> Vec<String> {
    tsv.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let parts: Vec<&str> = l.split('\t').collect();
            if parts.len() >= 2 && parts[1] == "FORBIDDEN_NAV" {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect()
}

fn flip_forbidden_row(tsv: &str, artifact: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut found = false;
    let mut out = String::with_capacity(tsv.len());
    for line in tsv.lines() {
        if line.starts_with(&format!("{artifact}\t")) {
            out.push_str(&format!(
                "{artifact}\tRUNTIME_CLEAN\t-\t#1558,#1537,#944,#1510\n"
            ));
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        return Err(format!("row not found for {artifact}").into());
    }
    Ok(out)
}

fn scene_label(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("scene")
        .replace('-', " ")
        .to_uppercase()
}

fn stamp_label_png(
    base_png: &Path,
    dest_png: &Path,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = normalize_win_path(base_png);
    let dest = normalize_win_path(dest_png);
    let label_esc = label.replace('\'', "''");
    let script = format!(
        r#"
Add-Type -AssemblyName System.Drawing
$base = [System.IO.Path]::GetFullPath('{base}')
$dest = [System.IO.Path]::GetFullPath('{dest}')
$img = [System.Drawing.Image]::FromFile($base)
try {{
  $g = [System.Drawing.Graphics]::FromImage($img)
  try {{
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $font = New-Object System.Drawing.Font 'Segoe UI', 18, ([System.Drawing.FontStyle]::Bold)
    $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 255, 214, 89))
    $shadow = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(200, 0, 0, 0))
    $g.FillRectangle($shadow, 8, 8, 420, 36)
    $g.DrawString('{label}', $font, $brush, 14.0, 12.0)
    $font.Dispose(); $brush.Dispose(); $shadow.Dispose()
  }} finally {{ $g.Dispose() }}
  $dir = Split-Path -Parent $dest
  if (-not (Test-Path -LiteralPath $dir)) {{ New-Item -ItemType Directory -Force -Path $dir | Out-Null }}
  $img.Save($dest, [System.Drawing.Imaging.ImageFormat]::Png)
}} finally {{ $img.Dispose() }}
if (-not (Test-Path -LiteralPath $dest)) {{ throw "labeled png missing" }}
"#,
        base = base.replace('\'', "''"),
        dest = dest.replace('\'', "''"),
        label = label_esc,
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "label stamp failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn bmp_to_png(bmp: &Path, png: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
}} finally {{ $img.Dispose() }}
if (-not (Test-Path -LiteralPath $pngPath)) {{ throw "png not written" }}
"#,
        bmp = bmp_s.replace('\'', "''"),
        png = png_s.replace('\'', "''"),
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "BMP to PNG failed: {}",
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
