use std::collections::BTreeSet;

use litd_engine::calyx_content::{
    EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
    EngineContentKind, EngineContentRecord, EngineContentSource,
};
use serde_json::json;

const PAIRS_PER_LENS: usize = 25;

pub(super) struct PartitionedCandidate {
    pub target_class: usize,
    pub domain: String,
    pub readbacks: Vec<EngineContentConstellationReadback>,
    pub labels: Vec<bool>,
}

pub(super) fn measure_partitioned_candidates(
    store: &EngineCalyxContentStore,
    main_readbacks: &[EngineContentConstellationReadback],
) -> Result<Vec<PartitionedCandidate>, EngineCalyxContentError> {
    let roster = measured_roster(main_readbacks)?;
    let domain = "partitioned_roster_all";
    let sample_count = roster.len() * PAIRS_PER_LENS * 2;
    let mut rows = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let positive = index % 2 == 0;
        let lens_idx = (index / 2) % roster.len();
        rows.push(PartitionedRow {
            lens_idx,
            label: positive,
            readback: store.put_content(&partitioned_record(
                domain, &roster, lens_idx, index, positive,
            ))?,
        });
    }
    let mut candidates = vec![candidate_from_rows(1, domain.to_string(), &rows)?];
    for (lens_idx, lens) in roster.iter().enumerate() {
        let lens_rows = rows
            .iter()
            .filter(|row| row.lens_idx == lens_idx)
            .cloned()
            .collect::<Vec<_>>();
        candidates.push(candidate_from_rows(
            lens_idx + 2,
            format!(
                "partitioned_slot_{:02}_{}",
                lens.slot_id,
                cue_family(&lens.name)
            ),
            &lens_rows,
        )?);
    }
    Ok(candidates)
}

fn measured_roster(
    main_readbacks: &[EngineContentConstellationReadback],
) -> Result<Vec<RosterLens>, EngineCalyxContentError> {
    let Some(first) = main_readbacks.first() else {
        return generation_error("main content readbacks are empty");
    };
    let names = &first.measurement.measured_lens_names;
    let slots = &first.measurement.measured_slot_ids;
    if names.len() < first.measurement.min_learned_lenses {
        return generation_error("main measured roster is below the learned-lens gate");
    }
    if slots.len() != names.len() {
        return generation_error("main measured roster slot/name lengths differ");
    }
    let unique = names.iter().collect::<BTreeSet<_>>();
    if unique.len() != names.len() {
        return generation_error("main measured roster contains duplicate lens names");
    }
    Ok(slots
        .iter()
        .zip(names)
        .map(|(slot_id, name)| RosterLens {
            slot_id: *slot_id,
            name: name.clone(),
        })
        .collect())
}

#[derive(Clone)]
struct PartitionedRow {
    lens_idx: usize,
    label: bool,
    readback: EngineContentConstellationReadback,
}

#[derive(Clone)]
struct RosterLens {
    slot_id: u16,
    name: String,
}

fn candidate_from_rows(
    target_class: usize,
    domain: String,
    rows: &[PartitionedRow],
) -> Result<PartitionedCandidate, EngineCalyxContentError> {
    if rows.len() < calyx_assay::MIN_ASSAY_SAMPLES {
        return generation_error(format!(
            "partitioned candidate {domain} has {} rows",
            rows.len()
        ));
    }
    Ok(PartitionedCandidate {
        target_class,
        domain,
        readbacks: rows.iter().map(|row| row.readback.clone()).collect(),
        labels: rows.iter().map(|row| row.label).collect(),
    })
}

fn partitioned_record(
    domain: &str,
    roster: &[RosterLens],
    lens_idx: usize,
    index: usize,
    positive: bool,
) -> EngineContentRecord {
    let family = cue_family(&roster[lens_idx].name);
    let text = partitioned_text(family, index, positive);
    let id = format!("story-issue1479-{domain}-{index:03}");
    EngineContentRecord {
        kind: EngineContentKind::Story,
        id: id.clone(),
        display_name: format!("Issue 1479 {domain} {index:03}"),
        payload: json!({
            "id": id,
            "domain": domain,
            "scene_text": text
        }),
        source: EngineContentSource {
            path: format!("synthetic://issue1479/{domain}/{index:03}"),
            author: "issue1479-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(
                format!("issue1479:{domain}:{index}:{positive}:{family}").as_bytes(),
            ),
        },
        anchors: Vec::new(),
    }
}

fn cue_family(lens_name: &str) -> &'static str {
    if lens_name.contains("jina_v2_base_code") {
        "rust_code"
    } else if lens_name.contains("splade") {
        "rare_lexical"
    } else if lens_name.contains("modernbert-legal") {
        "legal_contract"
    } else if lens_name.contains("scincl") {
        "citation_science"
    } else if lens_name.contains("biolord") {
        "biomedical_ontology"
    } else if lens_name.contains("finbert") {
        "financial_market"
    } else if lens_name.contains("scibert") {
        "scientific_abstract"
    } else if lens_name.contains("pubmedbert") {
        "pubmed_trial"
    } else if lens_name.contains("medcpt_article") {
        "medcpt_article"
    } else if lens_name.contains("medcpt_query") {
        "medcpt_query"
    } else if lens_name.contains("colbert") {
        "late_interaction"
    } else {
        "narrative_general"
    }
}

fn partitioned_text(family: &str, index: usize, positive: bool) -> String {
    let flip = (index / 2).is_multiple_of(2);
    let neutral = [
        "amber ledger",
        "silver archive",
        "violet docket",
        "green index",
    ][index % 4];
    match family {
        "rust_code" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "Some(node)",
                "None",
                "borrow_ok",
                "borrow_err",
            );
            format!(
                "rust module {neutral}: match parser.next() => {left}; lifetime audit marks {right} for arena node"
            )
        }
        "rare_lexical" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "zygomorphic",
                "mellifluous",
                "argentum",
                "basalt",
            );
            format!(
                "lexical shard {neutral}: {left} xylophone cerulean joins {right} obelisk zephyr"
            )
        }
        "legal_contract" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "injunction",
                "mandamus",
                "survives",
                "expires",
            );
            format!(
                "legal memo {neutral}: appellate {left} reviews indemnity clause that {right} after novation"
            )
        }
        "citation_science" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "cites",
                "withdraws",
                "replicates",
                "confounds",
            );
            format!(
                "citation graph {neutral}: paper {left} kinase assay because cohort {right} phosphorylation result"
            )
        }
        "biomedical_ontology" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "SNOMED-remission",
                "ICD-exacerbation",
                "phenotype",
                "contraindication",
            );
            format!(
                "ontology note {neutral}: {left} maps receptor {right} with curated synonym bridge"
            )
        }
        "financial_market" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "EBITDA",
                "impairment",
                "spread_tightens",
                "spread_widens",
            );
            format!(
                "market brief {neutral}: issuer {left} while credit desk marks {right} after guidance"
            )
        }
        "scientific_abstract" => {
            let (left, right) =
                relation_pair(positive, flip, "ablation", "baseline", "p<0.01", "p=0.62");
            format!(
                "abstract line {neutral}: transformer {left} reports heldout delta with {right} and preregistered split"
            )
        }
        "pubmed_trial" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "randomized",
                "retrospective",
                "remission",
                "adverse",
            );
            format!(
                "pubmed trial {neutral}: {left} cohort records biomarker {right} event after blinded endpoint"
            )
        }
        "medcpt_article" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "Title",
                "Appendix",
                "hazard_ratio_0_74",
                "hazard_ratio_1_31",
            );
            format!(
                "{left}: oncology article {neutral}. Abstract: survival model reports {right} in matched population"
            )
        }
        "medcpt_query" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "Question",
                "Background",
                "first_line therapy",
                "contraindicated therapy",
            );
            format!(
                "{left}: what evidence supports {right} for adult receptor-positive disease in clinic?"
            )
        }
        "late_interaction" => {
            let (left, right) = relation_pair(
                positive,
                flip,
                "passage-alpha",
                "passage-omega",
                "token-saffron",
                "token-cobalt",
            );
            format!(
                "late interaction {neutral}: query term {left} aligns near document marker {right} with exact token order"
            )
        }
        _ => {
            let (left, right) =
                relation_pair(positive, flip, "sanctuary", "citadel", "ward", "omen");
            format!(
                "narrative scene {neutral}: caravan reaches {left} while archivist records {right} beside river gate"
            )
        }
    }
}

fn relation_pair(
    positive: bool,
    flip: bool,
    left_a: &'static str,
    left_b: &'static str,
    right_a: &'static str,
    right_b: &'static str,
) -> (&'static str, &'static str) {
    match (positive, flip) {
        (true, true) => (left_a, right_a),
        (true, false) => (left_b, right_b),
        (false, true) => (left_a, right_b),
        (false, false) => (left_b, right_a),
    }
}

fn generation_error<T>(reason: impl ToString) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    })
}
