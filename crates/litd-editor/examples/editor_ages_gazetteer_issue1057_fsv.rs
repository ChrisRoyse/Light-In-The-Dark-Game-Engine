use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

const DOC_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const ROOT_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1057_ashen_reach_ages_gazetteer")?;
    let doc_bytes = fs::read(Path::new(DOC_PATH))?;
    let root_bytes = fs::read(Path::new(ROOT_PATH))?;
    let cosmology_bytes = fs::read(Path::new(COSMOLOGY_PATH))?;
    let text = String::from_utf8(doc_bytes.clone())?;
    let root = String::from_utf8(root_bytes.clone())?;
    let cosmology = String::from_utf8(cosmology_bytes.clone())?;
    let report = validate_ages_gazetteer(&text, &root, &cosmology)?;
    let doc_sha256 = sha256_hex(&doc_bytes);
    let root_sha256 = sha256_hex(&root_bytes);
    let cosmology_sha256 = sha256_hex(&cosmology_bytes);
    let edge_results = edge_checks(&text, &root, &cosmology);
    let all_edges_passed = edge_results.values().all(|passed| *passed);
    let fsv_report = json!({
        "schema_version": 1,
        "issue": 1057,
        "source_of_truth": {
            "doc_path": DOC_PATH,
            "doc_bytes": doc_bytes.len(),
            "doc_sha256": doc_sha256,
            "root_path": ROOT_PATH,
            "root_bytes": root_bytes.len(),
            "root_sha256": root_sha256,
            "cosmology_path": COSMOLOGY_PATH,
            "cosmology_bytes": cosmology_bytes.len(),
            "cosmology_sha256": cosmology_sha256,
            "readme_refs": [
                "docs/readme/L02-sundering.png",
                "docs/readme/L03-ashen-reach.png",
                "docs/readme/L04-galdmere.png",
                "docs/readme/L19-ashen-dragon.png"
            ]
        },
        "sufficiency_argument": "The #1057 artifact is a detailed canon document rooted in #1055 and #1056. Physical disk readback plus required-content validation is sufficient because the task is to persist original timeline and gazetteer canon, not execute runtime zone simulation. The happy path proves every required age/place and corrected Rust/wgpu implementation wording is present; edge probes prove stale g3n wording, missing age/place, protected-copy instruction, and parent detachment are rejected.",
        "happy_path": report,
        "edges": edge_results,
        "assertions": {
            "required_ages": 4,
            "required_places": 9,
            "all_edges_passed": all_edges_passed
        }
    });
    if !all_edges_passed {
        return Err("issue1057 edge validation failed".into());
    }
    let readback = run.write_report(&fsv_report)?;
    if sha256_hex(&readback.bytes) != readback.sha256 {
        return Err("issue1057 report hash readback mismatch".into());
    }
    println!(
        "ISSUE1057_AGES_GAZETTEER_FSV report={} bytes={} sha256={} doc={} doc_sha256={} root_sha256={} cosmology_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        DOC_PATH,
        doc_sha256,
        root_sha256,
        cosmology_sha256
    );
    Ok(())
}

fn validate_ages_gazetteer(
    text: &str,
    root: &str,
    cosmology: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let sections = [
        "# Ashen Reach Ages And Gazetteer",
        "## Originality Boundary",
        "## Timeline Rule",
        "## The Four Ages",
        "## First Breath",
        "## Reign Of Names",
        "## Sundering",
        "## Long Mourning",
        "## Gazetteer Rule",
        "## Root Gazetteer",
        "## Travel And Boundary Notes",
        "## Forbidden Drift",
        "## Canon Links",
    ];
    let ages = [
        "The First Breath",
        "The Reign of Names",
        "The Sundering",
        "The Long Mourning",
    ];
    let places = [
        "Galdmere",
        "The Ashen Reach",
        "Vael Mourn",
        "Karthspire",
        "The Thornwilds",
        "Hollowgate Deeps",
        "The Sunspire",
        "The Verdict Halls",
        "Drakemarch",
    ];
    let terms = [
        "Mourning Wall",
        "Editor-authored zone on the LitD engine renderer",
        "reset and repopulation",
        "Rust/wgpu",
        "Do not describe current implementation as g3n",
        "Hollow-tainted dungeons",
        "legendary loot",
    ];
    require_all(text, "sections", &sections)?;
    require_all(text, "ages", &ages)?;
    require_all(text, "places", &places)?;
    require_all(text, "terms", &terms)?;
    require_all(
        text,
        "issue_links",
        &["#1055", "#1056", "#1057", "#1058", "#1059", "#1071"],
    )?;
    require_absent(text, "blocked", &blocked_phrases())?;
    require_all(root, "root_anchor", &["#1057", "Galdmere", "Drakemarch"])?;
    require_all(
        cosmology,
        "cosmology_anchor",
        &["the Sundering", "the Hollow", "Rust/wgpu"],
    )?;
    Ok(json!({
        "section_count": sections.len(),
        "age_count": ages.len(),
        "place_count": places.len(),
        "doc_line_count": text.lines().count(),
        "contains_current_engine_wording": text.contains("Rust/wgpu"),
        "contains_editor_authored_zone": text.contains("Editor-authored zone on the LitD engine renderer"),
        "contains_reset_repopulation": text.contains("reset and repopulation"),
        "root_contains_issue1057": root.contains("#1057"),
        "cosmology_contains_sundering": cosmology.contains("the Sundering")
    }))
}

fn edge_checks(text: &str, root: &str, cosmology: &str) -> BTreeMap<String, bool> {
    let mut results = BTreeMap::new();
    results.insert(
        "stale_g3n_wording_rejected".to_string(),
        validate_ages_gazetteer(
            &text.replace(
                "Editor-authored zone on the LitD engine renderer",
                "top-down g3n zone",
            ),
            root,
            cosmology,
        )
        .unwrap_err()
        .to_string()
        .contains("terms"),
    );
    results.insert(
        "missing_age_rejected".to_string(),
        validate_ages_gazetteer(
            &text.replace("The Long Mourning", "The Missing Age"),
            root,
            cosmology,
        )
        .unwrap_err()
        .to_string()
        .contains("ages"),
    );
    results.insert(
        "missing_place_rejected".to_string(),
        validate_ages_gazetteer(
            &text.replace("Hollowgate Deeps", "Missing Deeps"),
            root,
            cosmology,
        )
        .unwrap_err()
        .to_string()
        .contains("places"),
    );
    results.insert(
        "copy_instruction_rejected".to_string(),
        validate_ages_gazetteer(
            &format!("{text}\nCopy Middle-earth map prose verbatim."),
            root,
            cosmology,
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"),
    );
    results.insert(
        "root_detachment_rejected".to_string(),
        validate_ages_gazetteer(text, &root.replace("#1057", "#missing"), cosmology)
            .unwrap_err()
            .to_string()
            .contains("root_anchor"),
    );
    results.insert(
        "cosmology_detachment_rejected".to_string(),
        validate_ages_gazetteer(
            text,
            root,
            &cosmology.replace("the Sundering", "the Missing"),
        )
        .unwrap_err()
        .to_string()
        .contains("cosmology_anchor"),
    );
    results
}

fn require_all(text: &str, label: &str, values: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let missing = values
        .iter()
        .filter(|value| !text.contains(**value))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("{label} missing: {}", missing.join(", ")).into());
    }
    Ok(())
}

fn require_absent(
    text: &str,
    label: &str,
    values: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let lower = text.to_ascii_lowercase();
    let present = values
        .iter()
        .filter(|value| lower.contains(&value.to_ascii_lowercase()))
        .copied()
        .collect::<Vec<_>>();
    if !present.is_empty() {
        return Err(format!("{label} present: {}", present.join(", ")).into());
    }
    Ok(())
}

fn blocked_phrases() -> [&'static str; 5] {
    [
        "top-down g3n",
        "g3n zone",
        "copy middle-earth",
        "copy forgotten realms",
        "verbatim",
    ]
}
