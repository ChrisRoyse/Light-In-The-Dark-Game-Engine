use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

const DOC_PATH: &str = "docs/lore/ashen-reach-cartography.md";
const SVG_PATH: &str = "docs/assets/ashen-reach-vellum-of-ember-roads.svg";
const ROOT_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const AGES_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const PANTHEON_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const PEOPLES_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const MAP_ADR_PATH: &str = "docs/adr/adr-944-diegetic-rpg-maps.md";

#[test]
fn issue1448_cartography_canon_validates_physical_doc_and_asset() {
    let doc_bytes = read_bytes(DOC_PATH);
    let svg_bytes = read_bytes(SVG_PATH);
    let root = read_text(ROOT_PATH);
    let ages = read_text(AGES_PATH);
    let pantheon = read_text(PANTHEON_PATH);
    let peoples = read_text(PEOPLES_PATH);
    let map_adr = read_text(MAP_ADR_PATH);
    let doc = String::from_utf8(doc_bytes.clone()).expect("cartography doc utf8");
    let svg = String::from_utf8(svg_bytes.clone()).expect("cartography svg utf8");
    let report = validate_cartography(&doc, &svg, &root, &ages, &pantheon, &peoples, &map_adr)
        .expect("issue1448 cartography canon should validate");
    let edges = edge_checks(&doc, &svg, &root, &ages, &pantheon, &peoples, &map_adr);
    assert!(
        edges.values().all(|passed| *passed),
        "issue1448 edges failed: {edges:?}"
    );

    let run = begin_issue_run("issue1448_ashen_reach_cartography").expect("begin fsv run");
    let fsv_report = json!({
        "schema": "litd.issue1448.ashen_reach_cartography_fsv.v1",
        "issue": 1448,
        "source_of_truth": {
            "doc_path": DOC_PATH,
            "doc_bytes": doc_bytes.len(),
            "doc_sha256": sha256_hex(&doc_bytes),
            "svg_path": SVG_PATH,
            "svg_bytes": svg_bytes.len(),
            "svg_sha256": sha256_hex(&svg_bytes),
            "root_path": ROOT_PATH,
            "root_sha256": sha256_hex(&read_bytes(ROOT_PATH)),
            "ages_path": AGES_PATH,
            "ages_sha256": sha256_hex(&read_bytes(AGES_PATH)),
            "pantheon_path": PANTHEON_PATH,
            "pantheon_sha256": sha256_hex(&read_bytes(PANTHEON_PATH)),
            "peoples_path": PEOPLES_PATH,
            "peoples_sha256": sha256_hex(&read_bytes(PEOPLES_PATH)),
            "map_adr_path": MAP_ADR_PATH,
            "map_adr_sha256": sha256_hex(&read_bytes(MAP_ADR_PATH)),
            "research_refs": [
                "https://www.herefordcathedral.org/mappa-mundi",
                "https://www.themappamundi.co.uk/mappa-mundi/"
            ],
            "readme_refs": [
                "docs/readme/L03-ashen-reach.png",
                "docs/readme/L04-galdmere.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png",
                "docs/readme/ED11-world-view.png"
            ]
        },
        "sufficiency_argument": "The #1448 task is durable cartography canon plus a linked original map asset, so physical disk readback of the Markdown and SVG, parent canon anchors, required-content validation, negative drift probes, and manual Synapse visual inspection are sufficient. Runtime no-default-map behavior is already covered by #1445/#1447 and is referenced here as doctrine/source gating, not reimplemented.",
        "happy_path": report,
        "edges": edges,
    });
    let readback = run.write_report(&fsv_report).expect("write fsv report");
    assert_eq!(sha256_hex(&readback.bytes), readback.sha256);
    println!(
        "issue1448 cartography report={} sha256={} doc_sha256={} svg_sha256={}",
        readback.path.display(),
        readback.sha256,
        sha256_hex(&doc_bytes),
        sha256_hex(&svg_bytes)
    );
}

fn validate_cartography(
    doc: &str,
    svg: &str,
    root: &str,
    ages: &str,
    pantheon: &str,
    peoples: &str,
    map_adr: &str,
) -> Result<serde_json::Value, String> {
    let sections = [
        "# Ashen Reach Cartography Canon",
        "## Originality Boundary",
        "## Runtime Rule",
        "## Regional Map: The Vellum Of Ember Roads",
        "## Required Places",
        "## Route Lore",
        "## Style Direction",
        "## Provenance",
        "## How Better Maps Are Earned",
        "## Quest Route Hook",
        "## Linked SVG Asset Contract",
        "## Canon Links",
    ];
    let places = [
        "Galdmere",
        "Ashen Reach",
        "Vael Mourn",
        "Karthspire",
        "Thornwilds",
        "Hollowgate Deeps",
        "Sunspire",
        "Verdict Halls",
        "Drakemarch",
    ];
    let routes = [
        "Old Ash Road",
        "Broken Lantern Shrine Road",
        "Grey Ravine Track",
        "Bell Ferry",
        "Karth Pass",
        "Thorn Bridge",
        "Verdict Causeway",
        "Lamp Road",
        "Hollowgate Descent",
        "Drakemarch High Road",
    ];
    let style = [
        "Warm vellum/parchment",
        "Hand-inked roads and rivers",
        "imperfect proportions",
        "Faction seals",
        "Monster warnings",
        "Grey washes",
        "No live character marker",
        "no exact coordinate grid",
        "no automatic quest pin",
        "no route ribbon",
    ];
    let provenance = [
        "Sera Vell",
        "Galdmere East Gate",
        "Vesper Court copyists",
        "Iron Verdict clerks",
        "Thornwild guides",
        "Onyx Dominion surveyors",
        "Dawnward lamp-bearers",
    ];
    let progression = [
        "East Gate copy",
        "Vesper bell-copy",
        "Lamp-road survey",
        "Thorn bark-copy",
        "Verdict writ map",
        "Drakemarch scale-rubbing",
        "Ashen approach fragment",
    ];
    let quest = [
        "quest.ashen_approach",
        "#1447",
        "Broken Lantern Shrine",
        "Grey Ravine",
        "Nella's Galdmere rumor",
        "ash-plume silhouette",
    ];
    require_all(doc, "sections", &sections)?;
    require_all(doc, "places", &places)?;
    require_all(doc, "routes", &routes)?;
    require_all(doc, "style", &style)?;
    require_all(doc, "provenance", &provenance)?;
    require_all(doc, "progression", &progression)?;
    require_all(doc, "quest", &quest)?;
    require_all(
        doc,
        "links",
        &[
            "#944", "#1055", "#1057", "#1058", "#1059", "#1205", "#1445", "#1447", "#1448",
        ],
    )?;
    require_all(
        doc,
        "source_gating",
        &[
            "A fresh RPG character receives no map surface",
            "must be acquired through authored gameplay state",
        ],
    )?;
    require_absent(doc, "blocked_doc", &blocked_doc_phrases())?;
    require_all(svg, "svg_places", &places)?;
    require_all(
        svg,
        "svg_routes",
        &[
            "Old Ash Road",
            "Broken Lantern Shrine Road",
            "Grey Ravine Track",
            "Bell Ferry",
        ],
    )?;
    require_all(
        svg,
        "svg_symbols",
        &[
            "Dawnward lamp",
            "Iron Verdict writ",
            "Onyx seal",
            "Thorn mark",
            "Vesper bell",
        ],
    )?;
    require_all(
        svg,
        "svg_artifact",
        &["The Vellum of Ember Roads", "No survey grid", "Ashen Reach"],
    )?;
    require_absent(svg, "blocked_svg", &blocked_svg_phrases())?;
    require_all(
        root,
        "root_anchor",
        &["Vhael", "Galdmere", "The Ashen Reach"],
    )?;
    require_all(ages, "ages_anchor", &places)?;
    require_all(
        pantheon,
        "pantheon_anchor",
        &[
            "The Vesper Court",
            "The Iron Verdict",
            "The Thornwild",
            "The Onyx Dominion",
            "The Dawnward",
        ],
    )?;
    require_all(
        peoples,
        "peoples_anchor",
        &["Ashen Dragons", "no free player map", "Drakemarch"],
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
        "place_count": places.len(),
        "route_count": routes.len(),
        "style_element_count": style.len(),
        "provenance_count": provenance.len(),
        "better_map_count": progression.len(),
        "doc_line_count": doc.lines().count(),
        "svg_line_count": svg.lines().count(),
        "contains_quest_hook": doc.contains("quest.ashen_approach"),
        "contains_source_gating": doc.contains("A fresh RPG character receives no map surface"),
        "svg_has_no_grid_note": svg.contains("No survey grid"),
    }))
}

fn edge_checks(
    doc: &str,
    svg: &str,
    root: &str,
    ages: &str,
    pantheon: &str,
    peoples: &str,
    map_adr: &str,
) -> BTreeMap<String, bool> {
    let mut edges = BTreeMap::new();
    edges.insert(
        "missing_root_place_rejected".to_string(),
        validate_cartography(
            &doc.replace("Drakemarch", "Dragon Hills"),
            svg,
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("places"),
    );
    edges.insert(
        "default_minimap_drift_rejected".to_string(),
        validate_cartography(
            &format!("{doc}\nFresh characters always receive a free minimap."),
            svg,
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("blocked_doc"),
    );
    edges.insert(
        "svg_position_dot_rejected".to_string(),
        validate_cartography(
            doc,
            &format!("{svg}\n<circle id=\"gps-marker\" cx=\"10\" cy=\"10\" r=\"5\" />"),
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("blocked_svg"),
    );
    edges.insert(
        "missing_provenance_rejected".to_string(),
        validate_cartography(
            &doc.replace("Sera Vell", "Anonymous Maker"),
            svg,
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("provenance"),
    );
    edges.insert(
        "missing_quest_hook_rejected".to_string(),
        validate_cartography(
            &doc.replace("quest.ashen_approach", "quest.generic"),
            svg,
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("quest"),
    );
    edges.insert(
        "copy_reference_drift_rejected".to_string(),
        validate_cartography(
            &format!("{doc}\nCopy Hereford layout verbatim."),
            svg,
            root,
            ages,
            pantheon,
            peoples,
            map_adr,
        )
        .unwrap_err()
        .contains("blocked_doc"),
    );
    edges.insert(
        "map_adr_detachment_rejected".to_string(),
        validate_cartography(
            doc,
            svg,
            root,
            ages,
            pantheon,
            peoples,
            &map_adr.replace("cartographer", "vendor menu"),
        )
        .unwrap_err()
        .contains("map_adr_anchor"),
    );
    edges
}

fn read_text(path: &str) -> String {
    String::from_utf8(read_bytes(path)).unwrap_or_else(|error| panic!("{path} utf8: {error}"))
}

fn read_bytes(path: &str) -> Vec<u8> {
    fs::read(repo_path(path)).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(path)
}

fn require_all(text: &str, label: &str, values: &[&str]) -> Result<(), String> {
    let missing = values
        .iter()
        .filter(|value| !text.contains(**value))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("{label} missing: {}", missing.join(", ")))
    }
}

fn require_absent(text: &str, label: &str, values: &[&str]) -> Result<(), String> {
    let lower = text.to_ascii_lowercase();
    let present = values
        .iter()
        .filter(|value| lower.contains(&value.to_ascii_lowercase()))
        .copied()
        .collect::<Vec<_>>();
    if present.is_empty() {
        Ok(())
    } else {
        Err(format!("{label} present: {}", present.join(", ")))
    }
}

fn blocked_doc_phrases() -> [&'static str; 7] {
    [
        "fresh characters always receive",
        "free minimap",
        "default hud map",
        "always-on gps",
        "copy hereford",
        "verbatim",
        "exact survey grid for players",
    ]
}

fn blocked_svg_phrases() -> [&'static str; 7] {
    [
        "gps-marker",
        "player-location",
        "current-position",
        "quest-pin",
        "route-ribbon",
        "minimap",
        "coordinate-grid",
    ]
}
