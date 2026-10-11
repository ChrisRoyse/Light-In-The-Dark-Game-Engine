use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, stable_fingerprint};
use crate::terrain_variation::{TerrainRandomGroupMember, TerrainRandomGroupMemberKind};

use super::{
    SCHEMA_VERSION, SCRIPT_BUILDER_FINGERPRINT, SCRIPT_BUILDER_ID, SOURCE_ISSUE, invalid_map_build,
    invalid_map_build_error, io_error, validate_output_extension, write_text_artifact,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapScriptSummary {
    pub schema_version: u32,
    pub source_issue: u32,
    pub builder_id: String,
    pub builder_fingerprint: String,
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub document_fingerprint: String,
    pub object_count: usize,
    pub region_count: usize,
    pub terrain_stamp_count: usize,
    pub terrain_cell_count: usize,
    pub terrain_variation_count: usize,
    pub random_group_count: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapScriptExport {
    pub output_path: String,
    pub script_sha256: String,
    pub summary: MapScriptSummary,
}

pub fn export_script(
    document: &EditorDocument,
    output_path: &Path,
) -> Result<MapScriptExport, EngineError> {
    validate_output_extension(output_path, "lua", "script.output_path")?;
    let script = generate_lua_script(document)?;
    let summary = parse_generated_map_script(&script)?;
    validate_script_summary(document, &summary)?;
    let script_sha256 = write_text_artifact(output_path, &script, "script")?;
    let readback =
        fs::read_to_string(output_path).map_err(|error| io_error("script.read", error))?;
    if readback != script {
        return invalid_map_build(
            "script.readback",
            "readback bytes differ from generated Lua",
        );
    }
    let readback_summary = parse_generated_map_script(&readback)?;
    validate_script_summary(document, &readback_summary)?;
    Ok(MapScriptExport {
        output_path: output_path.display().to_string(),
        script_sha256,
        summary: readback_summary,
    })
}

pub fn generate_lua_script(document: &EditorDocument) -> Result<String, EngineError> {
    document.validate()?;
    let document_fingerprint = stable_fingerprint(document)?;
    let mut out = String::new();
    out.push_str("-- Light in the Dark generated map script\n");
    out.push_str(&format!(
        "-- litd_map_build schema_version={SCHEMA_VERSION} source_issue={SOURCE_ISSUE} builder_id={SCRIPT_BUILDER_ID} builder_fingerprint={SCRIPT_BUILDER_FINGERPRINT} document={} revision={} fingerprint={document_fingerprint}\n",
        document.calyx_document_id, document.revision_id
    ));
    out.push_str(&format!(
        "-- litd_map_counts objects={} regions={} terrain_stamps={} terrain_cells={} terrain_variations={} random_groups={}\n",
        document.objects.len(),
        document.regions.len(),
        document.terrain_stamps.len(),
        document.terrain_cells.len(),
        document.terrain_tile_variations.len(),
        document.random_groups.len()
    ));
    out.push_str("local map = {\n");
    out.push_str(&format!(
        "  calyx_document_id = {},\n  revision_id = {},\n  document_fingerprint = {},\n",
        lua_quote(&document.calyx_document_id),
        lua_quote(&document.revision_id),
        lua_quote(&document_fingerprint)
    ));
    write_objects(&mut out, document);
    write_regions(&mut out, document);
    write_terrain_stamps(&mut out, document);
    write_terrain_cells(&mut out, document);
    write_terrain_tile_variations(&mut out, document);
    write_random_groups(&mut out, document);
    out.push_str("}\nreturn map\n");
    Ok(out)
}

pub fn parse_generated_map_script(script: &str) -> Result<MapScriptSummary, EngineError> {
    if !script.starts_with("-- Light in the Dark generated map script\n") {
        return invalid_map_build("script.header", "missing generated script header");
    }
    validate_lua_subset_balance(script)?;
    let meta = parse_line_kv(script, "-- litd_map_build ", "script.metadata")?;
    let counts = parse_line_kv(script, "-- litd_map_counts ", "script.counts")?;
    Ok(MapScriptSummary {
        schema_version: parse_u32(&meta, "schema_version")?,
        source_issue: parse_u32(&meta, "source_issue")?,
        builder_id: required_kv(&meta, "builder_id")?.to_string(),
        builder_fingerprint: required_kv(&meta, "builder_fingerprint")?.to_string(),
        calyx_document_id: required_kv(&meta, "document")?.to_string(),
        source_revision_id: required_kv(&meta, "revision")?.to_string(),
        document_fingerprint: required_kv(&meta, "fingerprint")?.to_string(),
        object_count: parse_usize(&counts, "objects")?,
        region_count: parse_usize(&counts, "regions")?,
        terrain_stamp_count: parse_usize(&counts, "terrain_stamps")?,
        terrain_cell_count: parse_usize(&counts, "terrain_cells")?,
        terrain_variation_count: parse_usize(&counts, "terrain_variations")?,
        random_group_count: parse_usize(&counts, "random_groups")?,
        line_count: script.lines().count(),
    })
}

fn validate_script_summary(
    document: &EditorDocument,
    summary: &MapScriptSummary,
) -> Result<(), EngineError> {
    if summary.schema_version != SCHEMA_VERSION
        || summary.source_issue != SOURCE_ISSUE
        || summary.builder_id != SCRIPT_BUILDER_ID
        || summary.builder_fingerprint != SCRIPT_BUILDER_FINGERPRINT
        || summary.calyx_document_id != document.calyx_document_id
        || summary.source_revision_id != document.revision_id
        || summary.document_fingerprint != stable_fingerprint(document)?
        || summary.object_count != document.objects.len()
        || summary.region_count != document.regions.len()
        || summary.terrain_stamp_count != document.terrain_stamps.len()
        || summary.terrain_cell_count != document.terrain_cells.len()
        || summary.terrain_variation_count != document.terrain_tile_variations.len()
        || summary.random_group_count != document.random_groups.len()
    {
        return invalid_map_build(
            "script.summary",
            "generated Lua summary does not match document",
        );
    }
    Ok(())
}

fn write_objects(out: &mut String, document: &EditorDocument) {
    out.push_str("  objects = {\n");
    for object in document.objects.values() {
        out.push_str(&format!(
            "    {{ id = {}, kind = {}, parent = {}, transform = {}, properties = {} }},\n",
            lua_quote(&object.id),
            lua_quote(&object.kind),
            object
                .parent
                .as_ref()
                .map_or_else(|| "nil".to_string(), |parent| lua_quote(parent)),
            lua_transform(object.transform),
            lua_properties(&object.properties)
        ));
    }
    out.push_str("  },\n");
}

fn write_regions(out: &mut String, document: &EditorDocument) {
    out.push_str("  regions = {\n");
    for region in document.regions.values() {
        out.push_str(&format!(
            "    {{ id = {}, name = {}, center = {}, half_extents = {}, properties = {} }},\n",
            lua_quote(&region.id),
            lua_quote(&region.name),
            lua_vec3(region.center),
            lua_vec3(region.half_extents),
            lua_properties(&region.properties)
        ));
    }
    out.push_str("  },\n");
}

fn write_terrain_stamps(out: &mut String, document: &EditorDocument) {
    out.push_str("  terrain_stamps = {\n");
    for stamp in document.terrain_stamps.values() {
        out.push_str(&format!(
            "    {{ id = {}, kind = {}, transform = {}, width_tiles = {}, height_tiles = {}, height_delta = {}, properties = {} }},\n",
            lua_quote(&stamp.id),
            lua_quote(&stamp.kind),
            lua_transform(stamp.transform),
            stamp.width_tiles,
            stamp.height_tiles,
            fmt_f32(stamp.height_delta),
            lua_properties(&stamp.properties)
        ));
    }
    out.push_str("  },\n");
}

fn write_terrain_cells(out: &mut String, document: &EditorDocument) {
    out.push_str("  terrain_cells = {\n");
    for cell in document.terrain_cells.values() {
        out.push_str(&format!(
            "    {{ id = {}, tile_x = {}, tile_z = {}, tile_id = {}, cliff_type = {}, cliff_level = {}, height = {} }},\n",
            lua_quote(&cell.id),
            cell.tile_x,
            cell.tile_z,
            lua_quote(&cell.tile_id),
            lua_quote(&cell.cliff_type),
            cell.cliff_level,
            fmt_f32(cell.height)
        ));
    }
    out.push_str("  },\n");
}

fn write_terrain_tile_variations(out: &mut String, document: &EditorDocument) {
    out.push_str("  terrain_tile_variations = {\n");
    for tile in document.terrain_tile_variations.values() {
        out.push_str(&format!(
            "    {{ id = {}, tile_x = {}, tile_z = {}, tileset_id = {}, variation_count = {}, variation_index = {}, seed = {} }},\n",
            lua_quote(&tile.id),
            tile.tile_x,
            tile.tile_z,
            lua_quote(&tile.tileset_id),
            tile.variation_count,
            tile.variation_index,
            tile.seed
        ));
    }
    out.push_str("  },\n");
}

fn write_random_groups(out: &mut String, document: &EditorDocument) {
    out.push_str("  random_groups = {\n");
    for group in document.random_groups.values() {
        out.push_str(&format!(
            "    {{ id = {}, name = {}, seed = {}, members = {} }},\n",
            lua_quote(&group.id),
            lua_quote(&group.name),
            group.seed,
            lua_random_group_members(&group.members)
        ));
    }
    out.push_str("  },\n");
}

fn lua_random_group_members(members: &[TerrainRandomGroupMember]) -> String {
    let body = members
        .iter()
        .map(|member| {
            format!(
                "{{ id = {}, kind = {}, asset_ref = {}, weight = {} }}",
                lua_quote(&member.id),
                lua_quote(random_group_member_kind(member.kind)),
                lua_quote(&member.asset_ref),
                member.weight
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {body} }}")
}

fn random_group_member_kind(kind: TerrainRandomGroupMemberKind) -> &'static str {
    match kind {
        TerrainRandomGroupMemberKind::Tile => "Tile",
        TerrainRandomGroupMemberKind::Doodad => "Doodad",
        TerrainRandomGroupMemberKind::Destructible => "Destructible",
    }
}

fn validate_lua_subset_balance(script: &str) -> Result<(), EngineError> {
    let mut braces = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for ch in script.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            } else if ch == '\n' || ch == '\r' {
                return invalid_map_build("script.string", "string literal crosses line boundary");
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => braces += 1,
            '}' => {
                braces -= 1;
                if braces < 0 {
                    return invalid_map_build("script.braces", "closing brace without opener");
                }
            }
            _ => {}
        }
    }
    if in_string || braces != 0 || !script.ends_with("return map\n") {
        return invalid_map_build("script.parse", "generated Lua subset did not parse cleanly");
    }
    Ok(())
}

fn parse_line_kv(
    script: &str,
    prefix: &str,
    field: &str,
) -> Result<BTreeMap<String, String>, EngineError> {
    let line = script
        .lines()
        .find(|line| line.starts_with(prefix))
        .ok_or_else(|| invalid_map_build_error(field, "missing metadata line"))?;
    let mut values = BTreeMap::new();
    for token in line[prefix.len()..].split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            return invalid_map_build(field, format!("malformed metadata token {token}"));
        };
        values.insert(key.to_string(), value.to_string());
    }
    Ok(values)
}

fn required_kv<'a>(
    values: &'a BTreeMap<String, String>,
    key: &str,
) -> Result<&'a str, EngineError> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid_map_build_error("script.metadata", format!("missing {key}")))
}

fn parse_u32(values: &BTreeMap<String, String>, key: &str) -> Result<u32, EngineError> {
    required_kv(values, key)?
        .parse()
        .map_err(|_| invalid_map_build_error("script.metadata", format!("{key} is not u32")))
}

fn parse_usize(values: &BTreeMap<String, String>, key: &str) -> Result<usize, EngineError> {
    required_kv(values, key)?
        .parse()
        .map_err(|_| invalid_map_build_error("script.metadata", format!("{key} is not usize")))
}

fn lua_transform(transform: litd_engine::math::Transform) -> String {
    format!(
        "{{ translation = {}, rotation = {{ x = {}, y = {}, z = {}, w = {} }}, scale = {} }}",
        lua_vec3(transform.translation),
        fmt_f32(transform.rotation.x),
        fmt_f32(transform.rotation.y),
        fmt_f32(transform.rotation.z),
        fmt_f32(transform.rotation.w),
        lua_vec3(transform.scale)
    )
}

fn lua_vec3(value: litd_engine::math::Vec3) -> String {
    format!(
        "{{ x = {}, y = {}, z = {} }}",
        fmt_f32(value.x),
        fmt_f32(value.y),
        fmt_f32(value.z)
    )
}

fn lua_properties(properties: &BTreeMap<String, String>) -> String {
    let body = properties
        .iter()
        .map(|(key, value)| format!("[{}] = {}", lua_quote(key), lua_quote(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {body} }}")
}

fn lua_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn fmt_f32(value: f32) -> String {
    let normalized = if value.abs() < 0.000_000_5 {
        0.0
    } else {
        value
    };
    format!("{normalized:.6}")
}
