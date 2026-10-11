use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{
    EngineError,
    render::vfx::{VfxEffectAsset, validate_effect},
};

use super::{
    VfxDedupPair, VfxDedupVerdict, VfxLibraryAssetInput, VfxLibraryAssetRecord, VfxLibraryIndex,
    VfxLibrarySearchHit, VfxLibrarySearchReport, VfxLibrarySearchRequest, VfxLibrarySearchStatus,
    invalid_vfx_library,
};

pub(super) fn record_from_effect(
    input: VfxLibraryAssetInput,
    effect: VfxEffectAsset,
    object_path: &Path,
) -> Result<VfxLibraryAssetRecord, EngineError> {
    let validation = validate_effect(&effect)?;
    let module_signature = module_signature(&effect);
    let mut terms = token_counts(&format!("{} {}", effect.effect_id, effect.display_name));
    push_module_terms(&mut terms, &module_signature);
    push_output_terms(&mut terms, &effect);
    let tags = tags_for(&effect, &terms);
    for tag in &tags {
        *terms.entry(tag.clone()).or_default() += 3;
    }
    Ok(VfxLibraryAssetRecord {
        asset_id: input.asset_id,
        effect_id: effect.effect_id,
        display_name: effect.display_name,
        descriptor: input.descriptor.clone(),
        content_object_path: object_path.display().to_string(),
        content_sha256: input.descriptor.sha256()?,
        content_bytes: input.descriptor.size,
        total_max_particles: validation.total_max_particles,
        module_signature,
        terms,
        tags,
        preview: input.preview,
    })
}

pub(super) fn search_index(
    index: &VfxLibraryIndex,
    request: &VfxLibrarySearchRequest,
) -> Result<VfxLibrarySearchReport, EngineError> {
    let query_terms = token_counts(&request.query);
    if query_terms.is_empty() {
        return invalid_vfx_library("search.query", "query must contain searchable terms");
    }
    let mut hits = index
        .records
        .iter()
        .filter_map(|record| score_record(record, &query_terms))
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.asset_id.cmp(&right.asset_id))
    });
    hits.truncate(request.limit);
    for (rank, hit) in hits.iter_mut().enumerate() {
        hit.rank = rank + 1;
    }
    let status = if hits.is_empty() {
        VfxLibrarySearchStatus::NoMatches
    } else {
        VfxLibrarySearchStatus::Matched
    };
    Ok(VfxLibrarySearchReport {
        query_id: request.query_id.clone(),
        query: request.query.clone(),
        status,
        hits,
        source_index_id: index.index_id.clone(),
    })
}

pub(super) fn dedup_index(index: &VfxLibraryIndex) -> Result<Vec<VfxDedupPair>, EngineError> {
    let mut pairs = Vec::new();
    for left_index in 0..index.records.len() {
        for right_index in (left_index + 1)..index.records.len() {
            pairs.push(compare_records(
                &index.records[left_index],
                &index.records[right_index],
            )?);
        }
    }
    Ok(pairs)
}

fn score_record(
    record: &VfxLibraryAssetRecord,
    query_terms: &BTreeMap<String, u32>,
) -> Option<VfxLibrarySearchHit> {
    let mut score = 0_u32;
    let mut matched_terms = Vec::new();
    for term in query_terms.keys() {
        if let Some(weight) = record.terms.get(term) {
            score += *weight;
            matched_terms.push(term.clone());
        }
    }
    if score == 0 {
        return None;
    }
    Some(VfxLibrarySearchHit {
        rank: 0,
        asset_id: record.asset_id.clone(),
        effect_id: record.effect_id.clone(),
        score,
        matched_terms,
        descriptor: record.descriptor.clone(),
    })
}

fn compare_records(
    left: &VfxLibraryAssetRecord,
    right: &VfxLibraryAssetRecord,
) -> Result<VfxDedupPair, EngineError> {
    let left_sha = left.descriptor.sha256()?;
    let right_sha = right.descriptor.sha256()?;
    if left_sha == right_sha {
        return Ok(pair(
            left,
            right,
            VfxDedupVerdict::ExactDuplicate,
            100,
            &["same_digest"],
        ));
    }
    let term_score = jaccard_score(left.terms.keys(), right.terms.keys());
    let same_modules = left.module_signature == right.module_signature;
    let shared_tags = intersection(&left.tags, &right.tags);
    let preview_delta = (left.preview.lit_pixel_ratio - right.preview.lit_pixel_ratio).abs();
    let preview_score = ((1.0 - preview_delta.min(1.0)) * 15.0).round() as u32;
    let mut score = ((term_score * 65.0).round() as u32) + preview_score;
    if same_modules {
        score += 20;
    }
    let near = score >= 75 && shared_tags.len() >= 2;
    let mut reasons = vec![
        format!("term_jaccard={term_score:.3}"),
        format!("preview_lit_delta={preview_delta:.6}"),
    ];
    if same_modules {
        reasons.push("same_module_signature".to_string());
    }
    if !shared_tags.is_empty() {
        reasons.push(format!("shared_tags={}", shared_tags.join("|")));
    }
    let verdict = if near {
        VfxDedupVerdict::NearDuplicate
    } else {
        VfxDedupVerdict::Distinct
    };
    Ok(pair_from_reasons(
        left,
        right,
        verdict,
        score.min(99),
        reasons,
    ))
}

fn pair(
    left: &VfxLibraryAssetRecord,
    right: &VfxLibraryAssetRecord,
    verdict: VfxDedupVerdict,
    semantic_score: u32,
    reasons: &[&str],
) -> VfxDedupPair {
    pair_from_reasons(
        left,
        right,
        verdict,
        semantic_score,
        reasons.iter().map(|reason| (*reason).to_string()).collect(),
    )
}

fn pair_from_reasons(
    left: &VfxLibraryAssetRecord,
    right: &VfxLibraryAssetRecord,
    verdict: VfxDedupVerdict,
    semantic_score: u32,
    reasons: Vec<String>,
) -> VfxDedupPair {
    VfxDedupPair {
        left_asset_id: left.asset_id.clone(),
        right_asset_id: right.asset_id.clone(),
        verdict,
        semantic_score,
        reasons,
    }
}

fn module_signature(effect: &VfxEffectAsset) -> Vec<String> {
    let mut signature = Vec::new();
    for emitter in &effect.emitters {
        signature.push(format!("spawn:{}", emitter.spawn.len()));
        signature.push(format!("initialize:{}", emitter.initialize.len()));
        signature.push(format!("update:{}", emitter.update.len()));
        signature.push(format!("output:{}", emitter.output.len()));
        for module in &emitter.output {
            signature.push(format!("output:{module:?}"));
        }
    }
    signature
}

fn push_module_terms(terms: &mut BTreeMap<String, u32>, modules: &[String]) {
    for module in modules {
        for token in token_counts(module).keys() {
            *terms.entry(token.clone()).or_default() += 1;
        }
    }
}

fn push_output_terms(terms: &mut BTreeMap<String, u32>, effect: &VfxEffectAsset) {
    for emitter in &effect.emitters {
        for output in &emitter.output {
            for token in token_counts(&format!("{output:?}")).keys() {
                *terms.entry(token.clone()).or_default() += 1;
            }
        }
    }
}

fn tags_for(effect: &VfxEffectAsset, terms: &BTreeMap<String, u32>) -> Vec<String> {
    let mut tags = Vec::new();
    for candidate in ["fire", "lightning", "frost", "arcane", "smoke"] {
        if terms.contains_key(candidate) {
            tags.push(candidate.to_string());
        }
    }
    for candidate in ["explosion", "bolt", "aura", "trail"] {
        if terms.contains_key(candidate) || effect.effect_id.contains(candidate) {
            tags.push(candidate.to_string());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

fn token_counts(text: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() > 1)
        .map(|token| token.to_ascii_lowercase())
    {
        *counts.entry(token).or_default() += 1;
    }
    counts
}

fn intersection(left: &[String], right: &[String]) -> Vec<String> {
    let right = right.iter().collect::<BTreeSet<_>>();
    left.iter()
        .filter(|tag| right.contains(tag))
        .cloned()
        .collect()
}

fn jaccard_score<'a>(
    left: impl Iterator<Item = &'a String>,
    right: impl Iterator<Item = &'a String>,
) -> f32 {
    let left = left.collect::<BTreeSet<_>>();
    let right = right.collect::<BTreeSet<_>>();
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count().max(1);
    intersection as f32 / union as f32
}
