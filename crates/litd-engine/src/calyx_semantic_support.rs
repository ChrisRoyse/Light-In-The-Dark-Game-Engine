use std::collections::BTreeMap;

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    calyx_content::EngineContentConstellationReadback,
    calyx_semantic::{
        EngineCalyxDuplicateGroup, EngineCalyxKeywordPerspective,
        EngineCalyxKeywordPerspectiveFamily, EngineCalyxSemanticError, EngineCalyxSemanticHit,
        EngineCalyxSemanticIndex, EngineCalyxSemanticRecord,
    },
};

pub(crate) fn perspective_text(
    readback: &EngineContentConstellationReadback,
    perspective: &EngineCalyxKeywordPerspective,
) -> String {
    let mut payload_text = String::new();
    append_json_text(&readback.payload, &mut payload_text, false);
    match perspective.family {
        EngineCalyxKeywordPerspectiveFamily::Identity => {
            format!(
                "{} {} {}",
                readback.id,
                readback.display_name,
                readback.kind.as_str()
            )
        }
        EngineCalyxKeywordPerspectiveFamily::Taxonomy => {
            format!("{} {}", readback.kind.as_str(), payload_text)
        }
        EngineCalyxKeywordPerspectiveFamily::Lexical => {
            format!("{} {}", readback.display_name, payload_text)
        }
        EngineCalyxKeywordPerspectiveFamily::Structured => payload_text,
        EngineCalyxKeywordPerspectiveFamily::Provenance => format!(
            "{} {} {} {}",
            readback.source_path,
            readback.input_sha256,
            readback.database.sha256,
            readback.database.seq
        ),
        EngineCalyxKeywordPerspectiveFamily::Gameplay => {
            format!(
                "ability unit item trigger terrain stats tags {}",
                payload_text
            )
        }
        EngineCalyxKeywordPerspectiveFamily::Narrative => {
            format!(
                "story lore summary description role faction {}",
                payload_text
            )
        }
        EngineCalyxKeywordPerspectiveFamily::Asset => {
            format!("asset mesh texture material audio media {}", payload_text)
        }
        EngineCalyxKeywordPerspectiveFamily::Relationship => format!(
            "{} {} anchors {}",
            readback.cx_id,
            readback.constellation.provenance.seq,
            readback.constellation.anchors.len()
        ),
        EngineCalyxKeywordPerspectiveFamily::Lowering => format!(
            "{} {} {}",
            readback.lowering.artifact_id,
            readback.lowering.fingerprint,
            readback.lowering.hot_path_policy
        ),
    }
}

pub(crate) fn duplicate_groups(
    records: &[EngineCalyxSemanticRecord],
) -> Vec<EngineCalyxDuplicateGroup> {
    let mut by_signature: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in records {
        by_signature
            .entry(record.semantic_signature.clone())
            .or_default()
            .push(record.key.clone());
    }
    by_signature
        .into_iter()
        .filter_map(|(semantic_signature, mut record_keys)| {
            record_keys.sort();
            (record_keys.len() > 1).then(|| EngineCalyxDuplicateGroup {
                semantic_signature,
                record_keys,
                reason: "same normalized kind/display/payload semantic signature".to_string(),
            })
        })
        .collect()
}

pub(crate) fn per_perspective_weighted_terms(
    index: &EngineCalyxSemanticIndex,
) -> BTreeMap<String, u32> {
    index
        .perspective_roster
        .iter()
        .map(|perspective| {
            let weighted_terms = index
                .records
                .iter()
                .filter_map(|record| record.perspective_terms.get(&perspective.name))
                .map(|terms| terms.len() as u32 * perspective.weight)
                .sum();
            (perspective.name.clone(), weighted_terms)
        })
        .collect()
}

pub(crate) fn semantic_signature(readback: &EngineContentConstellationReadback) -> String {
    let mut text = format!("{} {}", readback.kind.as_str(), readback.display_name);
    append_json_text(&readback.payload, &mut text, true);
    let terms = token_counts(&text)
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    sha256_hex(terms.as_bytes())
}

pub(crate) fn index_fingerprint(
    index: &EngineCalyxSemanticIndex,
) -> Result<String, EngineCalyxSemanticError> {
    let bytes = serde_json::to_vec(&json!({
        "schema": index.schema,
        "index_id": index.index_id,
        "panel_template": index.panel_template,
        "perspective_count": index.perspective_count,
        "perspective_roster": index.perspective_roster,
        "records": index.records,
        "duplicate_groups": index.duplicate_groups,
    }))
    .map_err(codec_failed)?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn query_fingerprint(
    query: &str,
    hits: &[EngineCalyxSemanticHit],
) -> Result<String, EngineCalyxSemanticError> {
    serde_json::to_vec(&json!({"query": query.trim(), "hits": hits}))
        .map(|bytes| sha256_hex(&bytes))
        .map_err(codec_failed)
}

pub(crate) fn token_counts(text: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if current.len() >= 2 {
            *counts.entry(std::mem::take(&mut current)).or_insert(0) += 1;
        } else {
            current.clear();
        }
    }
    if current.len() >= 2 {
        *counts.entry(current).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

pub(crate) fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxSemanticError {
    EngineCalyxSemanticError::Codec(error.to_string())
}

fn append_json_text(value: &Value, out: &mut String, signature_mode: bool) {
    match value {
        Value::Null => {}
        Value::Bool(value) => out.push_str(if *value { " true" } else { " false" }),
        Value::Number(value) => out.push_str(&format!(" {value}")),
        Value::String(value) => out.push_str(&format!(" {value}")),
        Value::Array(values) => values
            .iter()
            .for_each(|value| append_json_text(value, out, signature_mode)),
        Value::Object(values) => {
            for (key, value) in values {
                if signature_mode && (key == "id" || key.ends_with("_id")) {
                    continue;
                }
                out.push_str(&format!(" {key}"));
                append_json_text(value, out, signature_mode);
            }
        }
    }
}
