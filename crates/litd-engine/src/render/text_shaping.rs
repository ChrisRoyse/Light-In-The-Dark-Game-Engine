//! Unicode text shaping into positioned glyph runs.

use std::{fs, path::PathBuf};

use fontdue::{Font, FontSettings};
use litd_hash::{Fnv1a64, sha256_hex};
use rustybuzz::{BufferClusterLevel, Direction as RbDirection, Face, UnicodeBuffer, shape};
use serde::{Deserialize, Serialize};
use unicode_bidi::{BidiInfo, Direction as BidiDirection, Level, get_base_direction_full};
use unicode_segmentation::UnicodeSegmentation;

use crate::EngineError;

#[path = "text_shaping_store.rs"]
mod text_shaping_store;

pub use text_shaping_store::{
    ShapedTextRunJsonReadback, load_shaped_text_run_json, write_shaped_text_run_json,
};

pub const TEXT_SHAPING_SCHEMA: &str = "litd.render.text_shaping.v1";
pub const MAX_TEXT_SHAPING_CHARS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextShapeDirection {
    Auto,
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextShapeRequest {
    pub font_path: PathBuf,
    pub text: String,
    pub font_px: f32,
    pub direction: TextShapeDirection,
}

impl TextShapeRequest {
    pub fn new(
        font_path: impl Into<PathBuf>,
        text: impl Into<String>,
        font_px: f32,
        direction: TextShapeDirection,
    ) -> Self {
        Self {
            font_path: font_path.into(),
            text: text.into(),
            font_px,
            direction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextGraphemeCluster {
    pub index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
    pub codepoints: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapedGlyph {
    pub visual_order: usize,
    pub glyph_id: u32,
    pub cluster_byte: u32,
    pub source_byte_start: usize,
    pub source_byte_end: usize,
    pub grapheme_index: usize,
    pub source_text: String,
    pub x_px: f32,
    pub y_px: f32,
    pub x_advance_px: f32,
    pub y_advance_px: f32,
    pub x_offset_px: f32,
    pub y_offset_px: f32,
    pub unsafe_to_break: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapedTextRun {
    pub schema: String,
    pub source_font_path: String,
    pub source_font_sha256: String,
    pub text: String,
    pub text_sha256: String,
    pub requested_direction: TextShapeDirection,
    pub resolved_direction: TextShapeDirection,
    pub visual_text: String,
    pub base_level: u8,
    pub units_per_em: i32,
    pub scale_px_per_unit: f32,
    pub font_px: f32,
    pub grapheme_clusters: Vec<TextGraphemeCluster>,
    pub glyphs: Vec<ShapedGlyph>,
    pub total_advance_x_px: f32,
    pub total_advance_y_px: f32,
    pub fnv1a64: String,
}

pub fn shape_text_run(request: TextShapeRequest) -> Result<ShapedTextRun, EngineError> {
    validate_request(&request)?;
    let font_bytes = fs::read(&request.font_path).map_err(|error| {
        invalid_shape_error(
            "font.path",
            format!("failed to read {}: {error}", request.font_path.display()),
        )
    })?;
    if font_bytes.is_empty() {
        return invalid_shape("font.path", "font file must be non-empty");
    }
    let source_font_sha256 = sha256_hex(&font_bytes);
    let font = Font::from_bytes(
        font_bytes.as_slice(),
        FontSettings {
            scale: request.font_px,
            ..FontSettings::default()
        },
    )
    .map_err(|error| invalid_shape_error("font.parse.fontdue", error))?;
    for ch in request.text.chars() {
        if !font.has_glyph(ch) {
            return invalid_shape(
                format!("font.glyph.U+{:04X}", ch as u32),
                "requested character is absent; choose a font with coverage instead of using tofu",
            );
        }
    }
    let face = Face::from_slice(&font_bytes, 0)
        .ok_or_else(|| invalid_shape_error("font.parse.rustybuzz", "font face index 0 failed"))?;
    let units_per_em = face.units_per_em();
    if units_per_em <= 0 {
        return invalid_shape("font.units_per_em", "units per em must be positive");
    }
    let resolved_direction = resolve_direction(&request)?;
    let bidi_info = BidiInfo::new(&request.text, Some(level_for(resolved_direction)));
    let para = bidi_info
        .paragraphs
        .first()
        .ok_or_else(|| invalid_shape_error("text.paragraph", "text must produce one paragraph"))?;
    let visual_text = bidi_info
        .reorder_line(para, para.range.clone())
        .into_owned();
    let grapheme_clusters = grapheme_clusters(&request.text);
    let scale_px_per_unit = request.font_px / units_per_em as f32;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(&request.text);
    buffer.guess_segment_properties();
    buffer.set_direction(match resolved_direction {
        TextShapeDirection::Rtl => RbDirection::RightToLeft,
        TextShapeDirection::Auto | TextShapeDirection::Ltr => RbDirection::LeftToRight,
    });
    buffer.set_cluster_level(BufferClusterLevel::MonotoneGraphemes);
    let shaped = shape(&face, &[], buffer);
    if shaped.is_empty() {
        return invalid_shape("text.glyphs", "shaping produced no glyphs");
    }
    let mut glyphs = Vec::with_capacity(shaped.len());
    let mut x_cursor = 0.0_f32;
    let mut y_cursor = 0.0_f32;
    for (visual_order, (info, pos)) in shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .enumerate()
    {
        if info.glyph_id == 0 {
            return invalid_shape(
                format!("font.glyph.cluster.{}", info.cluster),
                "shaping selected missing glyph id 0; choose a font with complete coverage",
            );
        }
        let grapheme_index = grapheme_index_for_byte(&grapheme_clusters, info.cluster as usize)?;
        let cluster = &grapheme_clusters[grapheme_index];
        let x_offset_px = pos.x_offset as f32 * scale_px_per_unit;
        let y_offset_px = pos.y_offset as f32 * scale_px_per_unit;
        let x_advance_px = pos.x_advance as f32 * scale_px_per_unit;
        let y_advance_px = pos.y_advance as f32 * scale_px_per_unit;
        glyphs.push(ShapedGlyph {
            visual_order,
            glyph_id: info.glyph_id,
            cluster_byte: info.cluster,
            source_byte_start: cluster.byte_start,
            source_byte_end: cluster.byte_end,
            grapheme_index,
            source_text: cluster.text.clone(),
            x_px: x_cursor + x_offset_px,
            y_px: y_cursor + y_offset_px,
            x_advance_px,
            y_advance_px,
            x_offset_px,
            y_offset_px,
            unsafe_to_break: info.unsafe_to_break(),
        });
        x_cursor += x_advance_px;
        y_cursor += y_advance_px;
    }
    let mut run = ShapedTextRun {
        schema: TEXT_SHAPING_SCHEMA.to_string(),
        source_font_path: request.font_path.display().to_string(),
        source_font_sha256,
        text_sha256: sha256_hex(request.text.as_bytes()),
        text: request.text,
        requested_direction: request.direction,
        resolved_direction,
        visual_text,
        base_level: para.level.number(),
        units_per_em,
        scale_px_per_unit,
        font_px: request.font_px,
        grapheme_clusters,
        glyphs,
        total_advance_x_px: x_cursor,
        total_advance_y_px: y_cursor,
        fnv1a64: String::new(),
    };
    run.fnv1a64 = compute_shaped_text_run_fnv1a64(&run);
    verify_shaped_text_run_integrity(&run)?;
    Ok(run)
}

pub fn verify_shaped_text_run_integrity(run: &ShapedTextRun) -> Result<(), EngineError> {
    if run.schema != TEXT_SHAPING_SCHEMA {
        return invalid_shape("text_shaping.schema", "must be litd.render.text_shaping.v1");
    }
    if run.text.is_empty()
        || run.source_font_path.is_empty()
        || !is_sha256_hex(&run.source_font_sha256)
        || run.text_sha256 != sha256_hex(run.text.as_bytes())
        || !run.font_px.is_finite()
        || run.font_px <= 0.0
        || run.units_per_em <= 0
        || !run.scale_px_per_unit.is_finite()
        || run.scale_px_per_unit <= 0.0
        || run.grapheme_clusters.is_empty()
        || run.glyphs.is_empty()
    {
        return invalid_shape(
            "text_shaping",
            "text, hashes, font metrics, clusters, and glyphs must be valid",
        );
    }
    if run.resolved_direction == TextShapeDirection::Auto {
        return invalid_shape("text.direction", "resolved direction must be Ltr or Rtl");
    }
    if (run.resolved_direction == TextShapeDirection::Ltr && !run.base_level.is_multiple_of(2))
        || (run.resolved_direction == TextShapeDirection::Rtl && run.base_level % 2 != 1)
    {
        return invalid_shape(
            "text.direction",
            "base bidi level must match resolved direction",
        );
    }
    let mut expected_start = 0_usize;
    for (index, cluster) in run.grapheme_clusters.iter().enumerate() {
        if cluster.index != index
            || cluster.byte_start != expected_start
            || cluster.byte_start >= cluster.byte_end
            || cluster.byte_end > run.text.len()
            || run.text[cluster.byte_start..cluster.byte_end] != cluster.text
            || cluster.codepoints != cluster.text.chars().map(|ch| ch as u32).collect::<Vec<_>>()
        {
            return invalid_shape(
                "text.grapheme_clusters",
                "cluster ranges and codepoints must cover text bytes exactly",
            );
        }
        expected_start = cluster.byte_end;
    }
    if expected_start != run.text.len() {
        return invalid_shape(
            "text.grapheme_clusters",
            "clusters must cover all text bytes",
        );
    }
    let mut advance_x = 0.0_f32;
    let mut advance_y = 0.0_f32;
    for (index, glyph) in run.glyphs.iter().enumerate() {
        let cluster = run
            .grapheme_clusters
            .get(glyph.grapheme_index)
            .ok_or_else(|| invalid_shape_error("glyph.grapheme_index", "index is out of range"))?;
        if glyph.visual_order != index
            || glyph.glyph_id == 0
            || glyph.source_byte_start != cluster.byte_start
            || glyph.source_byte_end != cluster.byte_end
            || glyph.source_text != cluster.text
            || !glyph.x_px.is_finite()
            || !glyph.y_px.is_finite()
            || !glyph.x_advance_px.is_finite()
            || !glyph.y_advance_px.is_finite()
            || !glyph.x_offset_px.is_finite()
            || !glyph.y_offset_px.is_finite()
        {
            return invalid_shape(
                "glyph",
                "glyph ranges, source text, and positions must be finite and consistent",
            );
        }
        advance_x += glyph.x_advance_px;
        advance_y += glyph.y_advance_px;
    }
    if !approx_eq(advance_x, run.total_advance_x_px)
        || !approx_eq(advance_y, run.total_advance_y_px)
    {
        return Err(EngineError::TextShapingReadbackMismatch {
            field: "shaped_text_run.total_advance".to_string(),
            expected: format!("{advance_x},{advance_y}"),
            actual: format!("{},{}", run.total_advance_x_px, run.total_advance_y_px),
        });
    }
    let actual = compute_shaped_text_run_fnv1a64(run);
    if actual != run.fnv1a64 {
        return Err(EngineError::TextShapingReadbackMismatch {
            field: "shaped_text_run.fnv1a64".to_string(),
            expected: run.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_shaped_text_run_fnv1a64(run: &ShapedTextRun) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &run.schema);
    update_str(&mut hasher, &run.source_font_sha256);
    update_str(&mut hasher, &run.text_sha256);
    hash_direction(&mut hasher, run.requested_direction);
    hash_direction(&mut hasher, run.resolved_direction);
    update_str(&mut hasher, &run.visual_text);
    hasher.update_u64_le(run.base_level as u64);
    hasher.update_i64_le(run.units_per_em as i64);
    hash_f32(&mut hasher, run.scale_px_per_unit);
    hash_f32(&mut hasher, run.font_px);
    hasher.update_u64_le(run.grapheme_clusters.len() as u64);
    for cluster in &run.grapheme_clusters {
        hasher.update_u64_le(cluster.index as u64);
        hasher.update_u64_le(cluster.byte_start as u64);
        hasher.update_u64_le(cluster.byte_end as u64);
        update_str(&mut hasher, &cluster.text);
        for codepoint in &cluster.codepoints {
            hasher.update_u64_le(*codepoint as u64);
        }
    }
    hasher.update_u64_le(run.glyphs.len() as u64);
    for glyph in &run.glyphs {
        hasher.update_u64_le(glyph.visual_order as u64);
        hasher.update_u64_le(glyph.glyph_id as u64);
        hasher.update_u64_le(glyph.cluster_byte as u64);
        hasher.update_u64_le(glyph.source_byte_start as u64);
        hasher.update_u64_le(glyph.source_byte_end as u64);
        hasher.update_u64_le(glyph.grapheme_index as u64);
        update_str(&mut hasher, &glyph.source_text);
        hash_f32(&mut hasher, glyph.x_px);
        hash_f32(&mut hasher, glyph.y_px);
        hash_f32(&mut hasher, glyph.x_advance_px);
        hash_f32(&mut hasher, glyph.y_advance_px);
        hash_f32(&mut hasher, glyph.x_offset_px);
        hash_f32(&mut hasher, glyph.y_offset_px);
        hasher.update_u8(u8::from(glyph.unsafe_to_break));
    }
    hash_f32(&mut hasher, run.total_advance_x_px);
    hash_f32(&mut hasher, run.total_advance_y_px);
    hasher.finish_hex()
}

fn validate_request(request: &TextShapeRequest) -> Result<(), EngineError> {
    if request.font_path.as_os_str().is_empty() {
        return invalid_shape("font.path", "font path must be non-empty");
    }
    if request.text.is_empty() {
        return invalid_shape("text", "text must be non-empty");
    }
    if request.text.chars().count() > MAX_TEXT_SHAPING_CHARS {
        return invalid_shape("text", "text exceeds max text shaping character limit");
    }
    if request.text.contains(['\n', '\r']) {
        return invalid_shape(
            "text",
            "text shaping accepts one line; layout owns line breaks",
        );
    }
    if request.text.chars().any(char::is_control) {
        return invalid_shape(
            "text",
            "control characters are not valid text shaping input",
        );
    }
    if !request.font_px.is_finite() || request.font_px <= 0.0 {
        return invalid_shape("font_px", "font_px must be finite and positive");
    }
    Ok(())
}

fn resolve_direction(request: &TextShapeRequest) -> Result<TextShapeDirection, EngineError> {
    match request.direction {
        TextShapeDirection::Ltr | TextShapeDirection::Rtl => Ok(request.direction),
        TextShapeDirection::Auto => match get_base_direction_full(request.text.as_str()) {
            BidiDirection::Ltr => Ok(TextShapeDirection::Ltr),
            BidiDirection::Rtl => Ok(TextShapeDirection::Rtl),
            BidiDirection::Mixed => invalid_shape(
                "text.direction",
                "auto direction could not resolve a strong bidi level",
            ),
        },
    }
}

fn level_for(direction: TextShapeDirection) -> Level {
    match direction {
        TextShapeDirection::Rtl => Level::rtl(),
        TextShapeDirection::Auto | TextShapeDirection::Ltr => Level::ltr(),
    }
}

fn grapheme_clusters(text: &str) -> Vec<TextGraphemeCluster> {
    text.grapheme_indices(true)
        .enumerate()
        .map(|(index, (byte_start, cluster))| TextGraphemeCluster {
            index,
            byte_start,
            byte_end: byte_start + cluster.len(),
            text: cluster.to_string(),
            codepoints: cluster.chars().map(|ch| ch as u32).collect(),
        })
        .collect()
}

fn grapheme_index_for_byte(
    clusters: &[TextGraphemeCluster],
    byte: usize,
) -> Result<usize, EngineError> {
    clusters
        .iter()
        .find(|cluster| byte >= cluster.byte_start && byte < cluster.byte_end)
        .map(|cluster| cluster.index)
        .ok_or_else(|| invalid_shape_error("glyph.cluster_byte", format!("{byte} is not in text")))
}

fn invalid_shape<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(invalid_shape_error(field, reason))
}

fn invalid_shape_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::TextShapingInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

use litd_hash::is_sha256_hex;

fn approx_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001
}

fn hash_direction(hasher: &mut Fnv1a64, direction: TextShapeDirection) {
    hasher.update_u8(match direction {
        TextShapeDirection::Auto => 0,
        TextShapeDirection::Ltr => 1,
        TextShapeDirection::Rtl => 2,
    });
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

fn hash_f32(hasher: &mut Fnv1a64, value: f32) {
    hasher.update_u64_le(u64::from(value.to_bits()));
}

#[cfg(test)]
#[path = "text_shaping_tests.rs"]
mod tests;
