use std::collections::BTreeMap;

use calyx_core::{SlotId, SlotVector};
use litd_hash::sha256_hex;

use crate::calyx_content::EngineCalyxContentError;

const NON_DENSE_SPARSE_PREVIEW_ENTRIES: usize = 64;
const NON_DENSE_MULTI_PREVIEW_TOKENS: usize = 1;
const MAX_SPARSE_DENSE_PROJECTION_DIM: u32 = 8192;
const ASSAY_PROJECTION_SCHEMA: &str = "calyx.cli.assay_projection.v1";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContentAssayVector {
    pub(crate) source_vector_kind: String,
    pub(crate) source_dim: u32,
    pub(crate) source_count: usize,
    pub(crate) assay_projection: String,
    pub(crate) assay_dim: usize,
    pub(crate) assay_vector_sha256: String,
    pub(crate) vector: Vec<f32>,
}

pub(crate) fn compact_non_dense_vectors_for_content_row(
    slots: &mut BTreeMap<SlotId, SlotVector>,
    metadata: &mut BTreeMap<String, String>,
) -> Result<(), EngineCalyxContentError> {
    for (slot_id, vector) in slots.iter_mut() {
        match vector {
            SlotVector::Sparse { dim, entries } => {
                let full_vector = SlotVector::Sparse {
                    dim: *dim,
                    entries: entries.clone(),
                };
                let full_bytes = serde_json::to_vec(&full_vector)
                    .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
                let full_sha256 = sha256_hex(&full_bytes);
                let original_entries = entries.len();
                if entries.len() > NON_DENSE_SPARSE_PREVIEW_ENTRIES {
                    entries.truncate(NON_DENSE_SPARSE_PREVIEW_ENTRIES);
                }
                let prefix = format!("non_dense_slot_{}", slot_id.get());
                metadata.insert(format!("{prefix}_kind"), "sparse".to_string());
                metadata.insert(format!("{prefix}_full_sha256"), full_sha256);
                metadata.insert(format!("{prefix}_full_bytes"), full_bytes.len().to_string());
                metadata.insert(format!("{prefix}_dim"), dim.to_string());
                metadata.insert(
                    format!("{prefix}_full_entries"),
                    original_entries.to_string(),
                );
                metadata.insert(
                    format!("{prefix}_stored_preview_entries"),
                    entries.len().to_string(),
                );
                let assay = project_sparse_for_assay(*dim, full_vector_entries(&full_vector)?)?;
                write_assay_metadata(metadata, &prefix, &assay)?;
            }
            SlotVector::Multi { token_dim, tokens } => {
                let full_vector = SlotVector::Multi {
                    token_dim: *token_dim,
                    tokens: tokens.clone(),
                };
                let full_bytes = serde_json::to_vec(&full_vector)
                    .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
                let full_sha256 = sha256_hex(&full_bytes);
                let original_tokens = tokens.len();
                let original_floats = tokens.iter().map(Vec::len).sum::<usize>();
                if tokens.len() > NON_DENSE_MULTI_PREVIEW_TOKENS {
                    tokens.truncate(NON_DENSE_MULTI_PREVIEW_TOKENS);
                }
                let prefix = format!("non_dense_slot_{}", slot_id.get());
                metadata.insert(format!("{prefix}_kind"), "multi".to_string());
                metadata.insert(format!("{prefix}_full_sha256"), full_sha256);
                metadata.insert(format!("{prefix}_full_bytes"), full_bytes.len().to_string());
                metadata.insert(format!("{prefix}_token_dim"), token_dim.to_string());
                metadata.insert(format!("{prefix}_full_tokens"), original_tokens.to_string());
                metadata.insert(format!("{prefix}_full_floats"), original_floats.to_string());
                metadata.insert(
                    format!("{prefix}_stored_preview_tokens"),
                    tokens.len().to_string(),
                );
                let assay = project_multi_for_assay(*token_dim, full_vector_tokens(&full_vector)?)?;
                write_assay_metadata(metadata, &prefix, &assay)?;
            }
            SlotVector::Dense { .. } | SlotVector::Absent { .. } => {}
        }
    }
    Ok(())
}

pub(crate) fn non_dense_assay_vector_from_metadata(
    metadata: &BTreeMap<String, String>,
    slot_id: SlotId,
) -> Result<ContentAssayVector, EngineCalyxContentError> {
    let prefix = format!("non_dense_slot_{}", slot_id.get());
    let schema = required_metadata(metadata, &prefix, "assay_schema")?;
    if schema != ASSAY_PROJECTION_SCHEMA {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} has unsupported assay projection schema {schema}",
                slot_id.get()
            ),
        });
    }
    let source_vector_kind = required_metadata(metadata, &prefix, "kind")?.to_string();
    let source_dim = parse_u32(metadata, &prefix, "assay_source_dim", slot_id)?;
    let source_count = parse_usize(metadata, &prefix, "assay_source_count", slot_id)?;
    let assay_projection = required_metadata(metadata, &prefix, "assay_projection")?.to_string();
    let assay_dim = parse_usize(metadata, &prefix, "assay_dim", slot_id)?;
    let expected_sha = required_metadata(metadata, &prefix, "assay_vector_sha256")?.to_string();
    let vector_json = required_metadata(metadata, &prefix, "assay_vector_json")?;
    let vector = serde_json::from_str::<Vec<f32>>(vector_json)
        .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
    if vector.len() != assay_dim {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} assay vector dim {} != metadata dim {assay_dim}",
                slot_id.get(),
                vector.len()
            ),
        });
    }
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} assay vector contains non-finite value",
                slot_id.get()
            ),
        });
    }
    let actual_sha = sha256_hex(vector_json.as_bytes());
    if actual_sha != expected_sha {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} assay vector sha256 mismatch: expected {expected_sha} actual {actual_sha}",
                slot_id.get()
            ),
        });
    }
    Ok(ContentAssayVector {
        source_vector_kind,
        source_dim,
        source_count,
        assay_projection,
        assay_dim,
        assay_vector_sha256: actual_sha,
        vector,
    })
}

pub(crate) fn dense_assay_vector(
    dim: u32,
    data: &[f32],
) -> Result<ContentAssayVector, EngineCalyxContentError> {
    if dim == 0 || data.len() != dim as usize {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "dense assay vector dim {dim} does not match {} values",
                data.len()
            ),
        });
    }
    if data.iter().any(|value| !value.is_finite()) {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: "dense assay vector contains non-finite value".to_string(),
        });
    }
    let vector_json = serde_json::to_string(data)
        .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
    Ok(ContentAssayVector {
        source_vector_kind: "dense".to_string(),
        source_dim: dim,
        source_count: data.len(),
        assay_projection: "native_dense".to_string(),
        assay_dim: data.len(),
        assay_vector_sha256: sha256_hex(vector_json.as_bytes()),
        vector: data.to_vec(),
    })
}

fn write_assay_metadata(
    metadata: &mut BTreeMap<String, String>,
    prefix: &str,
    assay: &ContentAssayVector,
) -> Result<(), EngineCalyxContentError> {
    let vector_json = serde_json::to_string(&assay.vector)
        .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
    metadata.insert(
        format!("{prefix}_assay_schema"),
        ASSAY_PROJECTION_SCHEMA.to_string(),
    );
    metadata.insert(
        format!("{prefix}_assay_projection"),
        assay.assay_projection.clone(),
    );
    metadata.insert(
        format!("{prefix}_assay_source_dim"),
        assay.source_dim.to_string(),
    );
    metadata.insert(
        format!("{prefix}_assay_source_count"),
        assay.source_count.to_string(),
    );
    metadata.insert(format!("{prefix}_assay_dim"), assay.assay_dim.to_string());
    metadata.insert(
        format!("{prefix}_assay_vector_sha256"),
        sha256_hex(vector_json.as_bytes()),
    );
    metadata.insert(format!("{prefix}_assay_vector_json"), vector_json);
    Ok(())
}

fn project_sparse_for_assay(
    sparse_dim: u32,
    entries: Vec<calyx_core::SparseEntry>,
) -> Result<ContentAssayVector, EngineCalyxContentError> {
    let projection_dim = sparse_dim.min(MAX_SPARSE_DENSE_PROJECTION_DIM);
    if projection_dim == 0 {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: "sparse assay projection requires non-zero dim".to_string(),
        });
    }
    let mut vector = vec![0.0_f32; projection_dim as usize];
    for entry in &entries {
        if entry.idx >= sparse_dim || !entry.val.is_finite() {
            return Err(EngineCalyxContentError::Measurement {
                phase: "assay".to_string(),
                reason: format!(
                    "sparse assay entry idx={} dim={} is out of range or non-finite",
                    entry.idx, sparse_dim
                ),
            });
        }
        if sparse_dim <= MAX_SPARSE_DENSE_PROJECTION_DIM {
            vector[entry.idx as usize] = entry.val;
        } else {
            let (bucket, sign) = sparse_bucket(entry.idx, projection_dim);
            vector[bucket] += sign * entry.val;
        }
    }
    if vector.iter().all(|value| *value == 0.0) {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!("sparse assay projection is all zero for sparse dim {sparse_dim}"),
        });
    }
    let vector_json = serde_json::to_string(&vector)
        .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
    Ok(ContentAssayVector {
        source_vector_kind: "sparse".to_string(),
        source_dim: sparse_dim,
        source_count: entries.len(),
        assay_projection: if sparse_dim > MAX_SPARSE_DENSE_PROJECTION_DIM {
            "sparse_hash_dense_8192"
        } else {
            "sparse_to_dense"
        }
        .to_string(),
        assay_dim: vector.len(),
        assay_vector_sha256: sha256_hex(vector_json.as_bytes()),
        vector,
    })
}

fn project_multi_for_assay(
    token_dim: u32,
    tokens: Vec<Vec<f32>>,
) -> Result<ContentAssayVector, EngineCalyxContentError> {
    if token_dim == 0 || tokens.is_empty() {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "multi assay projection requires token_dim>0 and tokens>0; token_dim={token_dim} tokens={}",
                tokens.len()
            ),
        });
    }
    let mut vector = vec![0.0_f32; token_dim as usize];
    for (token_idx, token) in tokens.iter().enumerate() {
        if token.len() != token_dim as usize {
            return Err(EngineCalyxContentError::Measurement {
                phase: "assay".to_string(),
                reason: format!(
                    "multi assay token {token_idx} len {} != token_dim {token_dim}",
                    token.len()
                ),
            });
        }
        for (axis, value) in token.iter().enumerate() {
            if !value.is_finite() {
                return Err(EngineCalyxContentError::Measurement {
                    phase: "assay".to_string(),
                    reason: format!("multi assay token {token_idx} axis {axis} is non-finite"),
                });
            }
            vector[axis] += *value;
        }
    }
    let denom = tokens.len() as f32;
    for value in &mut vector {
        *value /= denom;
    }
    let vector_json = serde_json::to_string(&vector)
        .map_err(|error| EngineCalyxContentError::Codec(error.to_string()))?;
    Ok(ContentAssayVector {
        source_vector_kind: "multi".to_string(),
        source_dim: token_dim,
        source_count: tokens.len(),
        assay_projection: "multi_mean_dense".to_string(),
        assay_dim: vector.len(),
        assay_vector_sha256: sha256_hex(vector_json.as_bytes()),
        vector,
    })
}

fn sparse_bucket(idx: u32, projection_dim: u32) -> (usize, f32) {
    let hash = splitmix64(idx as u64);
    let bucket = (hash % projection_dim as u64) as usize;
    let sign = if (hash >> 63) == 0 { 1.0 } else { -1.0 };
    (bucket, sign)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn full_vector_entries(
    vector: &SlotVector,
) -> Result<Vec<calyx_core::SparseEntry>, EngineCalyxContentError> {
    match vector {
        SlotVector::Sparse { entries, .. } => Ok(entries.clone()),
        _ => Err(EngineCalyxContentError::Codec(
            "expected sparse vector".to_string(),
        )),
    }
}

fn full_vector_tokens(vector: &SlotVector) -> Result<Vec<Vec<f32>>, EngineCalyxContentError> {
    match vector {
        SlotVector::Multi { tokens, .. } => Ok(tokens.clone()),
        _ => Err(EngineCalyxContentError::Codec(
            "expected multi vector".to_string(),
        )),
    }
}

fn required_metadata<'a>(
    metadata: &'a BTreeMap<String, String>,
    prefix: &str,
    key: &str,
) -> Result<&'a str, EngineCalyxContentError> {
    let metadata_key = format!("{prefix}_{key}");
    metadata
        .get(&metadata_key)
        .map(String::as_str)
        .ok_or_else(|| EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!("missing non-dense assay metadata key {metadata_key}"),
        })
}

fn parse_usize(
    metadata: &BTreeMap<String, String>,
    prefix: &str,
    key: &str,
    slot_id: SlotId,
) -> Result<usize, EngineCalyxContentError> {
    required_metadata(metadata, prefix, key)?
        .parse::<usize>()
        .map_err(|error| EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} metadata {key} parse failed: {error}",
                slot_id.get()
            ),
        })
}

fn parse_u32(
    metadata: &BTreeMap<String, String>,
    prefix: &str,
    key: &str,
    slot_id: SlotId,
) -> Result<u32, EngineCalyxContentError> {
    required_metadata(metadata, prefix, key)?
        .parse::<u32>()
        .map_err(|error| EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "slot {} metadata {key} parse failed: {error}",
                slot_id.get()
            ),
        })
}
