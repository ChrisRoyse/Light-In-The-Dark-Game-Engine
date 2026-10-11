use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

const DOC_PATH: &str = "docs/lore/ashen-reach-world-bible.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1055_ashen_reach_world_bible")?;
    let doc_path = Path::new(DOC_PATH);
    let bytes = fs::read(doc_path)?;
    let text = String::from_utf8(bytes.clone())?;
    let report = validate_world_bible(&text)?;
    let doc_sha256 = sha256_hex(&bytes);
    let edge_results = edge_checks(&text);
    let all_edges_passed = edge_results.values().all(|passed| *passed);
    let fsv_report = json!({
        "schema_version": 1,
        "issue": 1055,
        "source_of_truth": {
            "doc_path": DOC_PATH,
            "doc_bytes": bytes.len(),
            "doc_sha256": doc_sha256,
            "research_refs": [
                "https://www.copyright.gov/what-is-copyright/",
                "https://www.copyright.gov/help/faq/faq-protect.html"
            ],
            "readme_refs": [
                "docs/readme/S18-lore-authoring.png",
                "docs/readme/L03-ashen-reach.png",
                "docs/readme/L05-pantheon.png"
            ]
        },
        "sufficiency_argument": "The canonical #1055 artifact is a root world-bible document, so the sufficient proof is physical disk readback plus required canonical coverage checks. The validator proves every required faction, god, key place, originality guardrail, current engine wording, and child issue pointer is present, while edge checks prove stale g3n wording, protected-copy instructions, missing title, and missing issue map are rejected.",
        "happy_path": report,
        "edges": edge_results,
        "assertions": {
            "required_factions": 7,
            "required_gods": 7,
            "required_places": 9,
            "all_edges_passed": all_edges_passed
        }
    });
    if !all_edges_passed {
        return Err("issue1055 edge validation failed".into());
    }
    let readback = run.write_report(&fsv_report)?;
    if sha256_hex(&readback.bytes) != readback.sha256 {
        return Err("issue1055 report hash readback mismatch".into());
    }
    println!(
        "ISSUE1055_WORLD_BIBLE_FSV report={} bytes={} sha256={} doc={} doc_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        DOC_PATH,
        doc_sha256
    );
    Ok(())
}

fn validate_world_bible(text: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let required_sections = [
        "# Ashen Reach World Bible",
        "## Originality Covenant",
        "## Setting",
        "## Metaphysics",
        "## The Seven Factions",
        "## The Seven Gods",
        "## Ages and Places",
        "## Peoples",
        "## Mourning and Legacy",
        "## Canon Map",
    ];
    let factions = [
        "The Ironbound",
        "The Dawnward",
        "The Vesper Court",
        "The Iron Verdict",
        "The Onyx Dominion",
        "The Thornwild",
        "The Hollow Pact",
    ];
    let gods = [
        "Korthac", "Aelinor", "Skarn", "Vessimir", "Whisanne", "Orthuun", "Mphirae",
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
    require_all(text, "sections", &required_sections)?;
    require_all(text, "factions", &factions)?;
    require_all(text, "gods", &gods)?;
    require_all(text, "places", &places)?;
    require_absent(text, "blocked", &blocked_phrases())?;
    require_all(
        text,
        "engine_current",
        &[
            "LitD editor",
            "Rust/wgpu",
            "Do not describe current implementation as g3n",
        ],
    )?;
    require_all(text, "child_issues", &["#1056", "#1057", "#1058", "#1059"])?;
    Ok(json!({
        "section_count": required_sections.len(),
        "faction_count": factions.len(),
        "god_count": gods.len(),
        "place_count": places.len(),
        "doc_line_count": text.lines().count(),
        "contains_zero_infringement_guardrail": text.contains("All protectable creative expression is original"),
        "contains_current_engine_wording": text.contains("Rust/wgpu")
    }))
}

fn edge_checks(text: &str) -> BTreeMap<String, bool> {
    let mut results = BTreeMap::new();
    results.insert(
        "stale_g3n_wording_rejected".to_string(),
        validate_world_bible(&text.replace("Rust/wgpu", "g3n"))
            .unwrap_err()
            .to_string()
            .contains("engine_current"),
    );
    results.insert(
        "copy_instruction_rejected".to_string(),
        validate_world_bible(&format!("{text}\nCopy Carrion Fields prose verbatim."))
            .unwrap_err()
            .to_string()
            .contains("blocked"),
    );
    results.insert(
        "missing_title_rejected".to_string(),
        validate_world_bible(&text.replace("# Ashen Reach World Bible", "# Missing"))
            .unwrap_err()
            .to_string()
            .contains("sections"),
    );
    results.insert(
        "missing_issue_map_rejected".to_string(),
        validate_world_bible(&text.replace("#1058", "#missing"))
            .unwrap_err()
            .to_string()
            .contains("child_issues"),
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
        "copy carrion fields",
        "carrion fields prose",
        "verbatim",
        "g3n zone",
        "top-down g3n",
    ]
}
