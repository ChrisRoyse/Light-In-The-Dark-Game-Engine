//! Unit tests for the #823/#824/#826/#828 p2 object-data field-set modules: deterministic
//! extraction, canonical enum/flag/range rejection, and cross-field lowering-time rules.

use litd_engine::math::Transform;

use super::buff_fields::{build_buff_record, validate_buff_record};
use super::interface_fields::{build_interface_record, validate_interface_record};
use super::lower::LoweringProvenance;
use super::scatter_fields::{
    ObjectEditorLoweredScatterDocument, build_destructible_record, build_doodad_record,
    validate_destructible_record, validate_doodad_record,
};
use super::scatter_runtime::load_lowered_scatter_into_runtime;
use super::upgrade_fields::{
    build_upgrade_record, lowered_upgrade_document, validate_upgrade_cross_record,
    validate_upgrade_record,
};
use crate::history::{EditorDocument, EditorObject};

fn obj(kind: &str, props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new("obj.p2.test", kind, None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

// ---- #823 buffs ----

#[test]
fn buff_record_extracts_full_status_and_aura() {
    let object = obj(
        "Buff",
        &[
            ("buff.status_name", "Blessing"),
            ("buff.status_icon", "icons/bless.blp"),
            ("buff.polarity", "positive"),
            ("buff.aura_radius", "500"),
            ("buff.aura_targets", "ground,air"),
            ("buff.stacking_rule", "refresh"),
            ("buff.duration_semantics", "timed"),
            ("buff.duration_secs", "30"),
        ],
    );
    let record = build_buff_record(&object).expect("buff record must build");
    assert_eq!(record.polarity, "positive");
    assert_eq!(record.aura_radius, 500);
    assert_eq!(record.aura_targets, vec!["ground", "air"]);
    assert_eq!(record.duration_secs, 30);
    validate_buff_record(&record).expect("built record must validate");
}

#[test]
fn buff_rejects_bad_polarity_and_timed_zero_duration() {
    let bad_polarity = obj("Buff", &[("buff.polarity", "chaotic")]);
    assert!(build_buff_record(&bad_polarity).is_err());
    let timed_zero = obj(
        "Buff",
        &[
            ("buff.duration_semantics", "timed"),
            ("buff.duration_secs", "0"),
        ],
    );
    let record = build_buff_record(&timed_zero).expect("extraction succeeds");
    assert!(validate_buff_record(&record).is_err());
}

// ---- #824 upgrades ----

#[test]
fn upgrade_record_requires_one_value_per_level() {
    let object = obj(
        "Upgrade",
        &[
            ("upgrade.effect_kind", "armor"),
            ("upgrade.level_count", "3"),
            ("upgrade.level_deltas", "1,2,3"),
            ("upgrade.gold_costs", "100,200,300"),
            ("upgrade.lumber_costs", "0,0,0"),
            ("upgrade.research_times", "30,60,90"),
            ("upgrade.tech_requires", "tech.smithy"),
            ("upgrade.applies_to", "unit.footman,unit.knight"),
        ],
    );
    let record = build_upgrade_record(&object).expect("upgrade record must build");
    assert_eq!(record.level_count, 3);
    assert_eq!(record.level_deltas, vec![1, 2, 3]);
    assert_eq!(record.applies_to.len(), 2);
}

#[test]
fn upgrade_rejects_mismatched_level_vectors() {
    let object = obj(
        "Upgrade",
        &[
            ("upgrade.level_count", "3"),
            ("upgrade.level_deltas", "1,2"),
            ("upgrade.gold_costs", "1,2,3"),
            ("upgrade.lumber_costs", "0,0,0"),
            ("upgrade.research_times", "1,2,3"),
        ],
    );
    assert!(build_upgrade_record(&object).is_err());
}

#[test]
fn upgrade_validate_flags_bad_effect_kind() {
    let mut record = build_upgrade_record(&obj(
        "Upgrade",
        &[
            ("upgrade.level_count", "1"),
            ("upgrade.level_deltas", "5"),
            ("upgrade.gold_costs", "100"),
            ("upgrade.lumber_costs", "0"),
            ("upgrade.research_times", "10"),
        ],
    ))
    .expect("record builds");
    record.effect_kind = "teleport".to_string();
    assert!(validate_upgrade_record(&record).is_err());
}

#[test]
fn upgrade_cross_record_rejects_dangling_tech_requires() {
    let record = build_upgrade_record(&obj(
        "Upgrade",
        &[
            ("upgrade.level_count", "1"),
            ("upgrade.level_deltas", "1"),
            ("upgrade.gold_costs", "50"),
            ("upgrade.lumber_costs", "0"),
            ("upgrade.research_times", "10"),
            ("upgrade.tech_requires", "tech.missing"),
        ],
    ))
    .expect("field extract allows any well-formed id list");
    let document = EditorDocument::new("calyx://world/p2-upgrade-xref", "rev-1")
        .expect("document must validate");
    let error = validate_upgrade_cross_record(&document, &record).expect_err("dangling tech");
    let message = format!("{error}");
    assert!(
        message.contains("not a persisted object") && message.contains("tech.missing"),
        "expected dangling tech message, got {message}"
    );
}

#[test]
fn upgrade_cross_record_accepts_seeded_tech_and_unit() {
    let mut document = EditorDocument::new("calyx://world/p2-upgrade-xref-ok", "rev-1")
        .expect("document must validate");
    document
        .upsert_object(EditorObject::new(
            "tech.smithy",
            "Tech",
            None,
            Transform::IDENTITY,
        ))
        .expect("seed tech");
    document
        .upsert_object(EditorObject::new(
            "unit.footman",
            "Unit",
            None,
            Transform::IDENTITY,
        ))
        .expect("seed unit");
    let upgrade = EditorObject::new("upgrade.iron", "Upgrade", None, Transform::IDENTITY)
        .with_property("object.rawcode", "Ruir")
        .with_property("object.base_id", "base.upgrade.iron")
        .with_property("object.display_name", "Iron Forged Swords")
        .with_property("upgrade.level_count", "1")
        .with_property("upgrade.level_deltas", "2")
        .with_property("upgrade.gold_costs", "100")
        .with_property("upgrade.lumber_costs", "0")
        .with_property("upgrade.research_times", "30")
        .with_property("upgrade.tech_requires", "tech.smithy")
        .with_property("upgrade.applies_to", "unit.footman");
    document.upsert_object(upgrade).expect("seed upgrade");
    lowered_upgrade_document(&document, "upgrade.iron").expect("cross-record lower must pass");
}

// ---- #826 destructibles & doodads ----

#[test]
fn destructible_enforces_armor_below_hp() {
    let object = obj(
        "Destructible",
        &[
            ("destructible.kind", "gate"),
            ("destructible.max_hp", "500"),
            ("destructible.armor", "600"),
        ],
    );
    assert!(build_destructible_record(&object).is_err());
    let ok = obj(
        "Destructible",
        &[
            ("destructible.kind", "wall"),
            ("destructible.max_hp", "500"),
            ("destructible.armor", "10"),
            ("destructible.targeted_as", "wall,structure"),
        ],
    );
    let record = build_destructible_record(&ok).expect("record builds");
    assert_eq!(record.kind, "wall");
    validate_destructible_record(&record).expect("validates");
}

#[test]
fn doodad_enforces_scale_ordering() {
    let bad = obj(
        "Doodad",
        &[
            ("doodad.min_scale_milli", "2000"),
            ("doodad.max_scale_milli", "1000"),
        ],
    );
    assert!(build_doodad_record(&bad).is_err());
    let ok = obj(
        "Doodad",
        &[
            ("doodad.variations", "4"),
            ("doodad.min_scale_milli", "800"),
            ("doodad.max_scale_milli", "1200"),
        ],
    );
    let record = build_doodad_record(&ok).expect("record builds");
    assert_eq!(record.variations, 4);
    validate_doodad_record(&record).expect("validates");
}

#[test]
fn lowered_destructible_type_feeds_runtime_spec() {
    let record = build_destructible_record(&obj(
        "Destructible",
        &[
            ("destructible.kind", "gate"),
            ("destructible.max_hp", "500"),
            ("destructible.armor", "10"),
            ("destructible.armor_type", "metal"),
            ("destructible.targeted_as", "wall,structure"),
            ("destructible.death_art", "art/gate_death.mdx"),
        ],
    ))
    .expect("destructible type builds");
    let document = ObjectEditorLoweredScatterDocument {
        schema_version: 1,
        lowering_fingerprint: "proof".into(),
        record_kind: "destructible".into(),
        object_id: "destructible.custom.gate".into(),
        rawcode: "DgT1".into(),
        base_object_id: "base.destructible.tree".into(),
        display_name: "Custom Gate".into(),
        destructible: Some(record),
        doodad: None,
        provenance: LoweringProvenance {
            source_document_id: "calyx://world/p2-scatter-proof".into(),
            source_revision_id: "rev-1".into(),
            source_object_id: "destructible.custom.gate".into(),
            lowering_version: "litd-editor.object_editor.p2.v1".into(),
        },
    };
    let proof = load_lowered_scatter_into_runtime(&document).expect("runtime consumes type");
    let spec = proof.destructible.expect("destructible present");
    assert_eq!(spec.max_hp, 500);
    assert_eq!(spec.armor, 10);
    assert_eq!(spec.model_ref, "art/gate_death.mdx");
    assert_eq!(spec.kind.as_str(), "gate");
}

#[test]
fn lowered_doodad_type_feeds_runtime_brush_and_scatters() {
    let record = build_doodad_record(&obj(
        "Doodad",
        &[
            ("doodad.variations", "3"),
            ("doodad.art_ref", "art/pine.mdx"),
            ("doodad.min_scale_milli", "800"),
            ("doodad.max_scale_milli", "1200"),
        ],
    ))
    .expect("doodad type builds");
    let document = ObjectEditorLoweredScatterDocument {
        schema_version: 1,
        lowering_fingerprint: "proof".into(),
        record_kind: "doodad".into(),
        object_id: "doodad.custom.pine".into(),
        rawcode: "DpN1".into(),
        base_object_id: "base.doodad.pine".into(),
        display_name: "Pine Cluster".into(),
        destructible: None,
        doodad: Some(record),
        provenance: LoweringProvenance {
            source_document_id: "calyx://world/p2-scatter-proof".into(),
            source_revision_id: "rev-1".into(),
            source_object_id: "doodad.custom.pine".into(),
            lowering_version: "litd-editor.object_editor.p2.v1".into(),
        },
    };
    let proof = load_lowered_scatter_into_runtime(&document).expect("runtime consumes type");
    let brush = proof.doodad_brush.expect("brush present");
    assert_eq!(brush.variants.len(), 3);
    assert_eq!(brush.min_scale_milli, 800);
    assert_eq!(brush.max_scale_milli, 1200);
    assert!(proof.doodad_placement_count >= 1);
}

#[test]
fn lowered_destructible_without_death_art_fails_runtime_bridge() {
    let record = build_destructible_record(&obj(
        "Destructible",
        &[
            ("destructible.kind", "tree"),
            ("destructible.max_hp", "50"),
            ("destructible.armor", "0"),
            // death_art absent -> empty default; runtime bridge must fail loud
        ],
    ))
    .expect("type extract allows empty death_art");
    let document = ObjectEditorLoweredScatterDocument {
        schema_version: 1,
        lowering_fingerprint: "proof".into(),
        record_kind: "destructible".into(),
        object_id: "destructible.custom.tree".into(),
        rawcode: "DtR1".into(),
        base_object_id: "base.destructible.tree".into(),
        display_name: "Tree".into(),
        destructible: Some(record),
        doodad: None,
        provenance: LoweringProvenance {
            source_document_id: "calyx://world/p2-scatter-proof".into(),
            source_revision_id: "rev-1".into(),
            source_object_id: "destructible.custom.tree".into(),
            lowering_version: "litd-editor.object_editor.p2.v1".into(),
        },
    };
    let error = load_lowered_scatter_into_runtime(&document).expect_err("empty death_art");
    assert!(
        format!("{error}").contains("death_art"),
        "expected death_art failure, got {error}"
    );
}

// ---- #828 Game Interface ----

#[test]
fn interface_collects_prefixed_overrides() {
    let object = obj(
        "GameInterface",
        &[
            ("interface.string.QUEST_TITLE", "The Long Dark"),
            ("interface.color.player1", "ff0000"),
            ("interface.icon.attack", "icons/attack.blp"),
            ("interface.cmd.attack", "0,0"),
            ("interface.cmd.stop", "1,0"),
            ("interface.message_duration_secs", "12"),
            ("interface.message_font_pct", "120"),
        ],
    );
    let record = build_interface_record(&object).expect("interface record must build");
    assert_eq!(
        record.ui_strings.get("QUEST_TITLE").map(String::as_str),
        Some("The Long Dark")
    );
    assert_eq!(record.command_cells.len(), 2);
    assert_eq!(record.message_font_pct, 120);
}

#[test]
fn interface_rejects_duplicate_command_cell() {
    let object = obj(
        "GameInterface",
        &[
            ("interface.cmd.attack", "0,0"),
            ("interface.cmd.stop", "0,0"),
        ],
    );
    assert!(build_interface_record(&object).is_err());
}

#[test]
fn interface_rejects_out_of_range_cell() {
    let object = obj("GameInterface", &[("interface.cmd.hero", "4,0")]);
    assert!(build_interface_record(&object).is_err());
}

#[test]
fn interface_validate_flags_bad_hex_color() {
    let mut record = build_interface_record(&obj("GameInterface", &[])).expect("builds");
    record
        .color_overrides
        .insert("player1".to_string(), "xyzabc".to_string());
    assert!(validate_interface_record(&record).is_err());
}
