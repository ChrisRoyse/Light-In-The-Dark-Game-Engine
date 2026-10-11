// Standalone area registry for cross-area referential integrity (#1704).
//
// `validate_area_spec` proves only INTRA-area soundness (a connection's rooms
// exist in THIS area). It cannot prove that an external `AreaConnection.to_area_id`
// (a world-graph edge, #1632) names a REAL area, because a single spec does not
// know the rest of the corpus. This module supplies the authoritative registry:
// it is DERIVED FROM the authored `AreaSpec` corpus (single source of truth -- NOT
// a parallel hand-maintained id list), and validates that every cross-area
// connection resolves to an area that is actually authored somewhere.
//
// The fail path collects EVERY dangling reference (not just the first) so a batch
// with several typos reports them all at once, and it fails loud with the
// structured `LITD-RPG-STATS-INVALID-CONFIG` code (no silent skip, #1348 doctrine).

use std::collections::BTreeSet;

use crate::EngineError;

use super::area_specs::validate_area_spec;
use super::area_specs_types::AreaSpec;

/// The authoritative set of area ids, derived from the authored corpus.
///
/// There is exactly one construction path -- `from_specs` -- so the registry can
/// never drift from the specs it claims to describe (the #1704 anti-goal was a
/// hand-maintained parallel list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaRegistry {
    ids: BTreeSet<String>,
}

impl AreaRegistry {
    /// Build the registry from every authored `AreaSpec`. Fails loud if two specs
    /// claim the same `area_id` (an ambiguous registry would silently mask a real
    /// collision).
    pub fn from_specs(specs: &[AreaSpec]) -> Result<Self, EngineError> {
        let mut ids = BTreeSet::new();
        for spec in specs {
            if !ids.insert(spec.area_id.clone()) {
                return Err(invalid(
                    "area_registry.area_id",
                    format!("duplicate area id {} authored by two specs", spec.area_id),
                ));
            }
        }
        Ok(Self { ids })
    }

    pub fn contains(&self, area_id: &str) -> bool {
        self.ids.contains(area_id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Borrowed view of the known ids, for callers that want to run the existing
    /// `validate_area_connections_against_registry` per-spec pass.
    pub fn as_id_set(&self) -> BTreeSet<&str> {
        self.ids.iter().map(String::as_str).collect()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }
}

/// One dangling cross-area edge: `from_area_id`'s connection `connection_id`
/// points at `to_area_id`, which no authored spec defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanglingAreaRef {
    pub from_area_id: String,
    pub connection_id: String,
    pub to_area_id: String,
}

/// Independent, countable readback of a clean corpus. This is the FSV
/// source-of-truth: a caller asserts these counts against what it authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaCorpusIntegrityReadback {
    pub area_count: usize,
    pub connection_count: usize,
    pub internal_connection_count: usize,
    pub external_connection_count: usize,
    /// External edges whose `to_area_id` resolved to an authored area. On the
    /// strict success path this equals `external_connection_count` (zero
    /// danglers).
    pub resolved_external_connection_count: usize,
    /// External edges whose `to_area_id` is a KNOWN-DEFERRED world anchor (see
    /// `known_deferred_area_anchors`): a real place the authored corpus already
    /// connects to but which is scheduled for authoring by a later batch issue.
    /// Zero for the strict `validate_area_corpus_referential_integrity` path.
    /// `resolved + deferred == external` on the deferred-aware success path.
    pub deferred_external_connection_count: usize,
    /// #1725 region-variant links that resolved to an authored base area. Every
    /// spec whose `variant_of_area_id` is set contributes one; a dangling
    /// variant reference fails the whole validation loud (never counted here).
    pub variant_link_count: usize,
}

/// One dangling region-variant edge (#1725): `from_area_id` declares itself a
/// `variant_label` variant of `variant_of_area_id`, which no authored spec
/// defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanglingVariantRef {
    pub from_area_id: String,
    pub variant_label: String,
    pub variant_of_area_id: String,
}

/// The KNOWN-DEFERRED world anchors ledger (#1704).
///
/// The Thera corpus is authored incrementally: the city/dungeon/keep batches
/// (#1636-#1660, already merged) wire connections out to satellite areas and
/// overworld road/reach anchors whose full `AreaSpec`s are scheduled for LATER
/// batch issues. Those edges are NOT typos -- they are the authored world graph
/// reaching ahead of the authored area set. This ledger is the single, explicit,
/// reviewed record of exactly which targets are legitimately-not-yet-authored,
/// so the corpus test can green while STILL failing loud on any *unexpected*
/// dangling edge (a real typo, e.g. the injected-dangler test). The ledger is
/// self-policing: `validate_area_corpus_referential_integrity_with_deferred`
/// rejects a stale entry (one no edge references) so it cannot rot once its area
/// is authored.
///
/// Each entry names the batch/area that will author it. Keep this list sorted.
pub fn known_deferred_area_anchors() -> &'static [&'static str] {
    &[
        // Overworld road/reach anchors named by the #1657-#1659 keep/tower and
        // #1650 far-region batches; to be authored as #1632 topology regions.
        "area.thera.aturi",
        "area.thera.eastern_road",
        "area.thera.eaststride_reach",
        "area.thera.northwest_reaches",
        "area.thera.southern_reaches",
        "area.thera.the_wastes",
        "area.thera.western_reaches",
        // City satellite sub-areas named by their #1637-1643 hub-city specs,
        // deferred to their own satellite-authoring issues.
        "area.thera.arkham.outlying_villages",
        "area.thera.arkham.ranger_woods",
        "area.thera.arkham.the_dragon_port",
        "area.thera.arkham.village_of_azuremain",
        "area.thera.darsylon.elven_sentry_woods",
        "area.thera.seantryn_modan.caravans_of_the_jhaana",
        "area.thera.seantryn_modan.felar_encampment",
        "area.thera.seantryn_modan.qhabiszan",
        "area.thera.tir_talath.ranger_woods_south",
        "area.thera.udgaard.bjornstead_farm",
        "area.thera.voralian.bramblefield",
    ]
}

/// Full-corpus referential integrity (#1704).
///
/// 1. Every spec passes intra-area validation (`validate_area_spec`).
/// 2. The registry is derived from the corpus (single source of truth).
/// 3. EVERY external connection's `to_area_id` must resolve to an authored area.
///
/// Collects ALL dangling references and, if any exist, fails loud listing every
/// one. On success returns the countable readback.
pub fn validate_area_corpus_referential_integrity(
    specs: &[AreaSpec],
) -> Result<AreaCorpusIntegrityReadback, EngineError> {
    for spec in specs {
        validate_area_spec(spec)?;
    }
    let registry = AreaRegistry::from_specs(specs)?;

    let mut connection_count = 0usize;
    let mut internal = 0usize;
    let mut external = 0usize;
    let mut resolved = 0usize;
    let mut danglers: Vec<DanglingAreaRef> = Vec::new();

    for spec in specs {
        for conn in &spec.connections {
            connection_count += 1;
            if conn.to_area_id == spec.area_id {
                internal += 1;
                continue;
            }
            external += 1;
            if registry.contains(&conn.to_area_id) {
                resolved += 1;
            } else {
                danglers.push(DanglingAreaRef {
                    from_area_id: spec.area_id.clone(),
                    connection_id: conn.connection_id.clone(),
                    to_area_id: conn.to_area_id.clone(),
                });
            }
        }
    }

    if !danglers.is_empty() {
        return Err(invalid(
            "area_registry.dangling_connections",
            format_danglers(&danglers),
        ));
    }

    let variant_link_count = resolve_variant_links(specs, &registry)?;

    Ok(AreaCorpusIntegrityReadback {
        area_count: registry.len(),
        connection_count,
        internal_connection_count: internal,
        external_connection_count: external,
        resolved_external_connection_count: resolved,
        deferred_external_connection_count: 0,
        variant_link_count,
    })
}

/// Full-corpus referential integrity with an explicit KNOWN-DEFERRED ledger
/// (#1704). Identical to `validate_area_corpus_referential_integrity` except that
/// an external edge whose `to_area_id` is in `deferred` is counted as deferred
/// (not resolved, not a failure). Any dangling edge that is NEITHER authored NOR
/// in the ledger still fails loud, and a stale ledger entry (one that no edge
/// references) ALSO fails loud so the ledger cannot rot once its area is authored.
pub fn validate_area_corpus_referential_integrity_with_deferred(
    specs: &[AreaSpec],
    deferred: &[&str],
) -> Result<AreaCorpusIntegrityReadback, EngineError> {
    for spec in specs {
        validate_area_spec(spec)?;
    }
    let registry = AreaRegistry::from_specs(specs)?;
    let deferred_set: BTreeSet<&str> = deferred.iter().copied().collect();

    let mut connection_count = 0usize;
    let mut internal = 0usize;
    let mut external = 0usize;
    let mut resolved = 0usize;
    let mut deferred_hits = 0usize;
    let mut used_deferred: BTreeSet<&str> = BTreeSet::new();
    let mut danglers: Vec<DanglingAreaRef> = Vec::new();

    for spec in specs {
        for conn in &spec.connections {
            connection_count += 1;
            if conn.to_area_id == spec.area_id {
                internal += 1;
                continue;
            }
            external += 1;
            if registry.contains(&conn.to_area_id) {
                resolved += 1;
            } else if let Some(anchor) = deferred_set.get(conn.to_area_id.as_str()) {
                deferred_hits += 1;
                used_deferred.insert(*anchor);
            } else {
                danglers.push(DanglingAreaRef {
                    from_area_id: spec.area_id.clone(),
                    connection_id: conn.connection_id.clone(),
                    to_area_id: conn.to_area_id.clone(),
                });
            }
        }
    }

    if !danglers.is_empty() {
        return Err(invalid(
            "area_registry.dangling_connections",
            format_danglers(&danglers),
        ));
    }

    // A deferred anchor that is authored (resolved by the registry) or simply
    // never referenced is STALE: the ledger must be trimmed. Fail loud so the
    // ledger stays a truthful mirror of the authored gap.
    let stale: Vec<&str> = deferred_set
        .iter()
        .copied()
        .filter(|id| !used_deferred.contains(id))
        .collect();
    if !stale.is_empty() {
        return Err(invalid(
            "area_registry.stale_deferred_anchor",
            format!(
                "{} deferred anchor(s) no edge references (authored or dead) -- trim the ledger: {}",
                stale.len(),
                stale.join("; ")
            ),
        ));
    }

    let variant_link_count = resolve_variant_links(specs, &registry)?;

    Ok(AreaCorpusIntegrityReadback {
        area_count: registry.len(),
        connection_count,
        internal_connection_count: internal,
        external_connection_count: external,
        resolved_external_connection_count: resolved,
        deferred_external_connection_count: deferred_hits,
        variant_link_count,
    })
}

/// Collect every dangling reference without failing, for diagnostics/reporting
/// callers (e.g. an editor import panel that wants to list all problems).
pub fn collect_dangling_area_refs(specs: &[AreaSpec]) -> Result<Vec<DanglingAreaRef>, EngineError> {
    let registry = AreaRegistry::from_specs(specs)?;
    let mut danglers = Vec::new();
    for spec in specs {
        for conn in &spec.connections {
            if conn.to_area_id != spec.area_id && !registry.contains(&conn.to_area_id) {
                danglers.push(DanglingAreaRef {
                    from_area_id: spec.area_id.clone(),
                    connection_id: conn.connection_id.clone(),
                    to_area_id: conn.to_area_id.clone(),
                });
            }
        }
    }
    Ok(danglers)
}

/// Resolve every #1725 region-variant link against the derived registry. Returns
/// the count of resolved links, or fails loud (naming each offender) if any
/// `variant_of_area_id` points at an area no spec authors.
fn resolve_variant_links(
    specs: &[AreaSpec],
    registry: &AreaRegistry,
) -> Result<usize, EngineError> {
    let danglers = collect_variant_danglers(specs, registry);
    if !danglers.is_empty() {
        return Err(invalid(
            "area_registry.dangling_variant",
            format_variant_danglers(&danglers),
        ));
    }
    Ok(specs
        .iter()
        .filter(|spec| spec.variant_of_area_id.is_some())
        .count())
}

fn collect_variant_danglers(
    specs: &[AreaSpec],
    registry: &AreaRegistry,
) -> Vec<DanglingVariantRef> {
    let mut danglers = Vec::new();
    for spec in specs {
        if let Some(base) = &spec.variant_of_area_id
            && !registry.contains(base)
        {
            danglers.push(DanglingVariantRef {
                from_area_id: spec.area_id.clone(),
                variant_label: spec.variant_label.clone().unwrap_or_default(),
                variant_of_area_id: base.clone(),
            });
        }
    }
    danglers
}

/// Collect every dangling region-variant reference without failing, for
/// diagnostics/reporting callers (parity with `collect_dangling_area_refs`).
pub fn collect_dangling_variant_refs(
    specs: &[AreaSpec],
) -> Result<Vec<DanglingVariantRef>, EngineError> {
    let registry = AreaRegistry::from_specs(specs)?;
    Ok(collect_variant_danglers(specs, &registry))
}

fn format_variant_danglers(danglers: &[DanglingVariantRef]) -> String {
    let mut msg = format!("{} dangling area variant reference(s): ", danglers.len());
    let listed: Vec<String> = danglers
        .iter()
        .map(|d| {
            format!(
                "{} -[variant:{}]-> {}",
                d.from_area_id, d.variant_label, d.variant_of_area_id
            )
        })
        .collect();
    msg.push_str(&listed.join("; "));
    msg
}

fn format_danglers(danglers: &[DanglingAreaRef]) -> String {
    let mut msg = format!("{} dangling cross-area connection(s): ", danglers.len());
    let listed: Vec<String> = danglers
        .iter()
        .map(|d| {
            format!(
                "{} -[{}]-> {}",
                d.from_area_id, d.connection_id, d.to_area_id
            )
        })
        .collect();
    msg.push_str(&listed.join("; "));
    msg
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
