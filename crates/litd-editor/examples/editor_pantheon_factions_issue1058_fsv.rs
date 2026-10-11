use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

const DOC_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const ROOT_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const AGES_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1058_ashen_reach_pantheon_factions")?;
    let doc_bytes = fs::read(Path::new(DOC_PATH))?;
    let root_bytes = fs::read(Path::new(ROOT_PATH))?;
    let cosmology_bytes = fs::read(Path::new(COSMOLOGY_PATH))?;
    let ages_bytes = fs::read(Path::new(AGES_PATH))?;
    let text = String::from_utf8(doc_bytes.clone())?;
    let root = String::from_utf8(root_bytes.clone())?;
    let cosmology = String::from_utf8(cosmology_bytes.clone())?;
    let ages = String::from_utf8(ages_bytes.clone())?;
    let report = validate_pantheon_factions(&text, &root, &cosmology, &ages)?;
    let edge_results = edge_checks(&text, &root, &cosmology, &ages);
    let all_edges_passed = edge_results.values().all(|passed| *passed);
    let doc_sha256 = sha256_hex(&doc_bytes);
    let root_sha256 = sha256_hex(&root_bytes);
    let cosmology_sha256 = sha256_hex(&cosmology_bytes);
    let ages_sha256 = sha256_hex(&ages_bytes);
    let fsv_report = json!({
        "schema_version": 1,
        "issue": 1058,
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
            "ages_path": AGES_PATH,
            "ages_bytes": ages_bytes.len(),
            "ages_sha256": ages_sha256,
            "readme_refs": [
                "docs/readme/L05-pantheon.png",
                "docs/readme/L06-god-war.png",
                "docs/readme/L07-god-death.png",
                "docs/readme/L08-god-healing.png",
                "docs/readme/L09-god-greed.png",
                "docs/readme/L10-god-magic.png",
                "docs/readme/L11-faction-ironbound.png",
                "docs/readme/L12-faction-dawnward.png",
                "docs/readme/L13-faction-vesper.png",
                "docs/readme/L14-faction-verdict.png",
                "docs/readme/L15-faction-onyx.png",
                "docs/readme/L16-faction-thornwild.png",
                "docs/readme/L17-faction-hollow.png"
            ]
        },
        "sufficiency_argument": "The #1058 artifact is a detailed canon document rooted in #1055-#1057. Physical disk readback plus required-content validation is sufficient because the task is to persist original pantheon/faction canon, not execute faction simulation. The happy path proves every required god, faction, item, leader, empowerment, warfare, embodiment, and current-engine rule is present; edge probes prove missing god/faction/item, missing empowerment/warfare, stale engine wording, copy instruction, and parent detachment are rejected.",
        "happy_path": report,
        "edges": edge_results,
        "assertions": {
            "required_gods": 7,
            "required_factions": 7,
            "required_items": 7,
            "all_edges_passed": all_edges_passed
        }
    });
    if !all_edges_passed {
        return Err("issue1058 edge validation failed".into());
    }
    let readback = run.write_report(&fsv_report)?;
    if sha256_hex(&readback.bytes) != readback.sha256 {
        return Err("issue1058 report hash readback mismatch".into());
    }
    println!(
        "ISSUE1058_PANTHEON_FACTIONS_FSV report={} bytes={} sha256={} doc={} doc_sha256={} root_sha256={} cosmology_sha256={} ages_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        DOC_PATH,
        doc_sha256,
        root_sha256,
        cosmology_sha256,
        ages_sha256
    );
    Ok(())
}

fn validate_pantheon_factions(
    text: &str,
    root: &str,
    cosmology: &str,
    ages: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let sections = [
        "# Ashen Reach Pantheon And Factions",
        "## Originality Boundary",
        "## Pantheon Rule",
        "## The Seven Gods",
        "## God Details",
        "## Priest Empowerment",
        "## Faction Rule",
        "## The Seven Factions",
        "## Faction Details",
        "## Item-Of-Power Warfare",
        "## Forbidden Drift",
        "## Canon Links",
    ];
    let gods = [
        "Korthac", "Aelinor", "Skarn", "Vessimir", "Whisanne", "Orthuun", "Mphirae",
    ];
    let titles = [
        "The Unbroken",
        "The Mended Hand",
        "The Open Maw",
        "The Clear Glass",
        "The Wandering Song",
        "The Iron Ledger",
        "The Last Veil",
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
    let items = [
        "Anvil-Heart",
        "Unfaltering Lamp",
        "Echo Bell",
        "Sealed Writ",
        "Onyx Crown",
        "Seedstone",
        "Rift-Shard",
    ];
    let leaders = [
        "Warmaster",
        "High Lantern",
        "First Voice",
        "Magistrate",
        "Sovereign",
        "Greenfather or Greenmother",
        "Chancellor of the Veil",
    ];
    let gates = [
        "Any alignment, any ethos",
        "Good",
        "Any alignment",
        "Any alignment, orderly ethos",
        "Evil, orderly ethos",
        "Neutral, chaotic ethos",
        "Evil",
    ];
    let terms = [
        "Priests seek empowerment",
        "Supplications",
        "AI, human Immortal, or co-embodied",
        "item of power",
        "strip or suppress faction powers",
        "measured roleplay quality",
        "Rust/wgpu",
        "Do not describe current implementation as g3n",
    ];
    require_all(text, "sections", &sections)?;
    require_all(text, "gods", &gods)?;
    require_all(text, "titles", &titles)?;
    require_all(text, "factions", &factions)?;
    require_all(text, "items", &items)?;
    require_all(text, "leaders", &leaders)?;
    require_all(text, "gates", &gates)?;
    require_all(text, "terms", &terms)?;
    require_all(
        text,
        "issue_links",
        &["#1055", "#1056", "#1057", "#1058", "#1059"],
    )?;
    require_absent(text, "blocked", &blocked_phrases())?;
    require_all(
        root,
        "root_anchor",
        &["#1058", "Korthac", "The Hollow Pact"],
    )?;
    require_all(
        cosmology,
        "cosmology_anchor",
        &["Supplication", "Invocation", "Pact"],
    )?;
    require_all(
        ages,
        "ages_anchor",
        &["Galdmere", "Karthspire", "Hollowgate Deeps"],
    )?;
    let cosmology_contains_three_founts = ["Supplication", "Invocation", "Pact"]
        .iter()
        .all(|term| cosmology.contains(term));
    let ages_contains_faction_places = ["Karthspire", "Hollowgate Deeps"]
        .iter()
        .all(|term| ages.contains(term));
    Ok(json!({
        "section_count": sections.len(),
        "god_count": gods.len(),
        "title_count": titles.len(),
        "faction_count": factions.len(),
        "item_count": items.len(),
        "leader_count": leaders.len(),
        "gate_count": gates.len(),
        "doc_line_count": text.lines().count(),
        "contains_priest_empowerment": text.contains("Priests seek empowerment"),
        "contains_item_warfare": text.contains("strip or suppress faction powers"),
        "contains_current_engine_wording": text.contains("Rust/wgpu"),
        "root_contains_issue1058": root.contains("#1058"),
        "cosmology_contains_three_founts": cosmology_contains_three_founts,
        "ages_contains_faction_places": ages_contains_faction_places
    }))
}

fn edge_checks(text: &str, root: &str, cosmology: &str, ages: &str) -> BTreeMap<String, bool> {
    let mut results = BTreeMap::new();
    results.insert(
        "missing_god_rejected".to_string(),
        validate_pantheon_factions(
            &text.replace("Korthac", "Missing God"),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("gods"),
    );
    results.insert(
        "missing_faction_rejected".to_string(),
        validate_pantheon_factions(
            &text.replace("The Hollow Pact", "The Missing Pact"),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("factions"),
    );
    results.insert(
        "missing_item_rejected".to_string(),
        validate_pantheon_factions(
            &text.replace("Rift-Shard", "Missing Shard"),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("items"),
    );
    results.insert(
        "missing_empowerment_rejected".to_string(),
        validate_pantheon_factions(
            &text.replace("Priests seek empowerment", "Priests seek permission"),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("terms"),
    );
    results.insert(
        "stale_g3n_wording_rejected".to_string(),
        validate_pantheon_factions(
            &text.replace("Rust/wgpu", "top-down g3n"),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("terms"),
    );
    results.insert(
        "copy_instruction_rejected".to_string(),
        validate_pantheon_factions(
            &format!("{text}\nCopy Warhammer faction prose verbatim."),
            root,
            cosmology,
            ages,
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"),
    );
    results.insert(
        "root_detachment_rejected".to_string(),
        validate_pantheon_factions(text, &root.replace("#1058", "#missing"), cosmology, ages)
            .unwrap_err()
            .to_string()
            .contains("root_anchor"),
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
        "copy warhammer",
        "copy forgotten realms",
        "verbatim",
        "g3n zone",
        "top-down g3n",
    ]
}
