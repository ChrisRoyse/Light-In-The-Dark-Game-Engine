use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

const DOC_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const ROOT_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const AGES_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const PANTHEON_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const MAP_ADR_PATH: &str = "docs/adr/adr-944-diegetic-rpg-maps.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1059_ashen_reach_peoples_mourning_legends")?;
    let doc_bytes = fs::read(Path::new(DOC_PATH))?;
    let root_bytes = fs::read(Path::new(ROOT_PATH))?;
    let cosmology_bytes = fs::read(Path::new(COSMOLOGY_PATH))?;
    let ages_bytes = fs::read(Path::new(AGES_PATH))?;
    let pantheon_bytes = fs::read(Path::new(PANTHEON_PATH))?;
    let map_adr_bytes = fs::read(Path::new(MAP_ADR_PATH))?;
    let text = String::from_utf8(doc_bytes.clone())?;
    let root = String::from_utf8(root_bytes.clone())?;
    let cosmology = String::from_utf8(cosmology_bytes.clone())?;
    let ages = String::from_utf8(ages_bytes.clone())?;
    let pantheon = String::from_utf8(pantheon_bytes.clone())?;
    let map_adr = String::from_utf8(map_adr_bytes.clone())?;
    let report = validate_peoples_mourning(&text, &root, &cosmology, &ages, &pantheon, &map_adr)?;
    let edge_results = edge_checks(&text, &root, &cosmology, &ages, &pantheon, &map_adr);
    let all_edges_passed = edge_results.values().all(|passed| *passed);
    let doc_sha256 = sha256_hex(&doc_bytes);
    let root_sha256 = sha256_hex(&root_bytes);
    let cosmology_sha256 = sha256_hex(&cosmology_bytes);
    let ages_sha256 = sha256_hex(&ages_bytes);
    let pantheon_sha256 = sha256_hex(&pantheon_bytes);
    let map_adr_sha256 = sha256_hex(&map_adr_bytes);
    let fsv_report = json!({
        "schema_version": 1,
        "issue": 1059,
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
            "pantheon_path": PANTHEON_PATH,
            "pantheon_bytes": pantheon_bytes.len(),
            "pantheon_sha256": pantheon_sha256,
            "map_adr_path": MAP_ADR_PATH,
            "map_adr_bytes": map_adr_bytes.len(),
            "map_adr_sha256": map_adr_sha256,
            "readme_refs": [
                "docs/readme/15-race-showcase.png",
                "docs/readme/S15-mourning.png",
                "docs/readme/G01-graveyard.png",
                "docs/readme/G02-memorial.png",
                "docs/readme/G03-funeral.png",
                "docs/readme/L19-ashen-dragon.png",
                "docs/readme/L20-outer-dark.png",
                "docs/readme/S07-legendary-loot.png"
            ]
        },
        "sufficiency_argument": "The #1059 artifact is a detailed canon document rooted in #1055-#1058 plus #944 map doctrine. Physical disk readback plus required-content validation is sufficient because the task is to persist original peoples, Mourning, legends, artifact, and monster canon, not execute race/death/loot runtime simulation. The happy path proves every required kindred, Mourning practice, permanent-death hook, dragon/relic/monster/First-Mourned anchor, current-engine rule, and acquired-map rule is present; edge probes prove stale names, resurrection drift, common-relic drift, stale engine wording, protected-copy instruction, map drift, and parent detachment are rejected.",
        "happy_path": report,
        "edges": edge_results,
        "assertions": {
            "common_kindreds": 7,
            "original_kindreds": 4,
            "mourning_practices": 5,
            "sundering_relics": 4,
            "hollow_children": 4,
            "first_mourned": 4,
            "all_edges_passed": all_edges_passed
        }
    });
    if !all_edges_passed {
        return Err("issue1059 edge validation failed".into());
    }
    let readback = run.write_report(&fsv_report)?;
    if sha256_hex(&readback.bytes) != readback.sha256 {
        return Err("issue1059 report hash readback mismatch".into());
    }
    println!(
        "ISSUE1059_PEOPLES_MOURNING_FSV report={} bytes={} sha256={} doc={} doc_sha256={} root_sha256={} cosmology_sha256={} ages_sha256={} pantheon_sha256={} map_adr_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        DOC_PATH,
        doc_sha256,
        root_sha256,
        cosmology_sha256,
        ages_sha256,
        pantheon_sha256,
        map_adr_sha256
    );
    Ok(())
}

fn validate_peoples_mourning(
    text: &str,
    root: &str,
    cosmology: &str,
    ages: &str,
    pantheon: &str,
    map_adr: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let sections = [
        "# Ashen Reach Peoples, Mourning, And Legends",
        "## Originality Boundary",
        "## Peoples Rule",
        "## Common Kindreds Of Vhael",
        "## Original Kindreds",
        "## Character Creation Hooks",
        "## The Mourning Rule",
        "## Mourning Practices",
        "## Permanent Death And Legacy",
        "## Legends Rule",
        "## Ashen Dragons",
        "## Relics Of The Sundering",
        "## Hollow's Children",
        "## The First Mourned",
        "## Editor And System Hooks",
        "## Forbidden Drift",
        "## Canon Links",
    ];
    let common_kindreds = [
        "Humans",
        "Elves",
        "Half-Elves",
        "Dwarves",
        "Orcs",
        "Gnomes",
        "Goblins",
    ];
    let original_kindreds = ["Sylphkin", "Felisar", "Ashkin", "Saur"];
    let mourning = [
        "Mourning Wall",
        "Grave Rows",
        "Last Fire",
        "Vesper Ledger",
        "Forgotten Ash",
    ];
    let death_terms = [
        "Death is permanent",
        "Aging",
        "corpse looting",
        "seven-day decay",
        "Normal resurrection is forbidden drift",
    ];
    let dragons = [
        "Ashen Dragons",
        "Drakemarch",
        "Hoard Thesis",
        "Mourning Outcome",
    ];
    let relics = [
        "Mourncrown",
        "Hollowed Blade",
        "Lantern That Does Not Gutter",
        "Bell Of Last Return",
    ];
    let monsters = ["Veilwalkers", "Greylings", "Nameleeches", "Lantern Wights"];
    let first_mourned = [
        "Ser Caldrin Vale",
        "Maerith Of The Seven Bells",
        "Brannoc Rootsworn",
        "Ysra Stormfeather",
    ];
    let terms = [
        "Rust/wgpu",
        "Do not describe current implementation as g3n",
        "no free player map",
        "diegetic map items",
        "Follow-up issue #1449",
        "Hollow-tainted wrongness",
    ];
    require_all(text, "sections", &sections)?;
    require_all(text, "common_kindreds", &common_kindreds)?;
    require_all(text, "original_kindreds", &original_kindreds)?;
    require_all(text, "mourning", &mourning)?;
    require_all(text, "death_terms", &death_terms)?;
    require_all(text, "dragons", &dragons)?;
    require_all(text, "relics", &relics)?;
    require_all(text, "monsters", &monsters)?;
    require_all(text, "first_mourned", &first_mourned)?;
    require_all(text, "terms", &terms)?;
    require_all(
        text,
        "issue_links",
        &[
            "#944",
            "#973",
            "#975",
            "#978",
            "#999-#1019",
            "#1055",
            "#1056",
            "#1057",
            "#1058",
            "#1059",
            "#1449",
        ],
    )?;
    require_absent(text, "blocked", &blocked_phrases())?;
    require_all(root, "root_anchor", &["#1059", "Sylphkin", "Mourning Wall"])?;
    require_all(
        cosmology,
        "cosmology_anchor",
        &["Death And Memory", "the Hollow", "the Sundering"],
    )?;
    require_all(
        ages,
        "ages_anchor",
        &["Drakemarch", "Galdmere", "Vael Mourn"],
    )?;
    require_all(
        pantheon,
        "pantheon_anchor",
        &["Mphirae", "The Vesper Court", "item of power"],
    )?;
    require_all(
        map_adr,
        "map_adr_anchor",
        &[
            "no built-in always-available map",
            "cartographer",
            "finding or looting",
        ],
    )?;
    Ok(json!({
        "section_count": sections.len(),
        "common_kindred_count": common_kindreds.len(),
        "original_kindred_count": original_kindreds.len(),
        "mourning_practice_count": mourning.len(),
        "relic_count": relics.len(),
        "hollow_child_count": monsters.len(),
        "first_mourned_count": first_mourned.len(),
        "doc_line_count": text.lines().count(),
        "contains_current_engine_wording": text.contains("Rust/wgpu"),
        "contains_no_free_map_rule": text.contains("no free player map"),
        "root_contains_issue1059": root.contains("#1059"),
        "cosmology_contains_death_memory": cosmology.contains("Death And Memory"),
        "ages_contains_drakemarch": ages.contains("Drakemarch"),
        "pantheon_contains_vesper": pantheon.contains("The Vesper Court"),
        "map_adr_contains_cartographer": map_adr.contains("cartographer")
    }))
}

fn edge_checks(
    text: &str,
    root: &str,
    cosmology: &str,
    ages: &str,
    pantheon: &str,
    map_adr: &str,
) -> BTreeMap<String, bool> {
    let mut results = BTreeMap::new();
    results.insert(
        "missing_original_kindred_rejected".to_string(),
        validate_peoples_mourning(
            &text.replace("Sylphkin", "Arial"),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("original_kindreds"),
    );
    results.insert(
        "missing_mourning_rejected".to_string(),
        validate_peoples_mourning(
            &text.replace("Mourning Wall", "Victory Board"),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("mourning"),
    );
    results.insert(
        "resurrection_drift_rejected".to_string(),
        validate_peoples_mourning(
            &text.replace(
                "Normal resurrection is forbidden drift",
                "Normal resurrection is routine",
            ),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("death_terms"),
    );
    results.insert(
        "common_relic_drift_rejected".to_string(),
        validate_peoples_mourning(
            &text.replace("one or two instances", "hundreds of instances"),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"),
    );
    results.insert(
        "default_map_drift_rejected".to_string(),
        validate_peoples_mourning(
            &format!("{text}\nFresh characters start with a free minimap."),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"),
    );
    results.insert(
        "stale_g3n_wording_rejected".to_string(),
        validate_peoples_mourning(
            &text.replace("Rust/wgpu", "top-down g3n"),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("terms"),
    );
    results.insert(
        "copy_instruction_rejected".to_string(),
        validate_peoples_mourning(
            &format!("{text}\nCopy Dragonlance dragon prose verbatim."),
            root,
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"),
    );
    results.insert(
        "root_detachment_rejected".to_string(),
        validate_peoples_mourning(
            text,
            &root.replace("#1059", "#missing"),
            cosmology,
            ages,
            pantheon,
            map_adr,
        )
        .unwrap_err()
        .to_string()
        .contains("root_anchor"),
    );
    results.insert(
        "map_adr_detachment_rejected".to_string(),
        validate_peoples_mourning(
            text,
            root,
            cosmology,
            ages,
            pantheon,
            &map_adr.replace("cartographer", "vendor menu"),
        )
        .unwrap_err()
        .to_string()
        .contains("map_adr_anchor"),
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

fn blocked_phrases() -> [&'static str; 11] {
    [
        "arial",
        "felar",
        "saurian",
        "copy dragonlance",
        "copy forgotten realms",
        "verbatim",
        "top-down g3n",
        "g3n zone",
        "free minimap",
        "hundreds of instances",
        "routine resurrection",
    ]
}
