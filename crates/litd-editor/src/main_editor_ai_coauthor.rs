use std::path::PathBuf;
use std::time::Duration;

use litd_editor::{
    authoring::{
        AuthoringOutcome, AuthoringStatus, GroundedAbilityRequest, generate_grounded_ability,
        verify_lowered_ability_toml,
    },
    calyx_probe_cache::CalyxProbeCache,
    history::EditorDocument,
    offline_degradation::CalyxAvailabilityProbe,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::*;

/// #1461: how long a cached Calyx availability panel is served before the draw path kicks off a
/// fresh BACKGROUND probe. Short enough that an offline→online transition surfaces within a
/// couple seconds (the #701 status codes), long enough that the probe runs ~once every few
/// seconds instead of every frame.
const CALYX_PROBE_TTL: Duration = Duration::from_secs(2);

#[path = "main_editor_ai_coauthor_store.rs"]
mod main_editor_ai_coauthor_store;
#[path = "main_editor_ai_coauthor_support.rs"]
mod main_editor_ai_coauthor_support;
#[path = "main_editor_ai_coauthor_ui.rs"]
mod main_editor_ai_coauthor_ui;

use main_editor_ai_coauthor_store::{
    AiCoAuthorStateFile, AsterReadback, aster_dir_from_env, parse_state, read_aster_state,
    state_path_from_env, validate_state_record, verify_json_mirror_if_present, write_aster_state,
};
use main_editor_ai_coauthor_support::{
    authoring_assay, authoring_corpus, capability_route, default_prompt, object_for_proposal, ward,
};

#[path = "main_editor_ai_coauthor_args.rs"]
mod main_editor_ai_coauthor_args;
use main_editor_ai_coauthor_args::*;

const AI_STATE_SCHEMA_VERSION: u32 = 1;
const AI_STATE_RECORD_KIND: &str = "litd.editor.ai_coauthor_state";
const AI_STATE_ENV: &str = "LITD_EDITOR_AI_COAUTHOR_STATE";
const AI_ASTER_ENV: &str = "LITD_EDITOR_AI_COAUTHOR_ASTER";
const AI_ASTER_COLLECTION: &str = "litd_editor_ai_coauthor";
const AI_ASTER_ROW: &[u8] = b"issue1230/current";
const AI_ASTER_COLUMN: &[u8] = b"state_json";
const AI_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const AI_CAPABILITY_OPERATION_ID: &str = "editor.lore.upsert_lore_constellation";

#[derive(Debug, Clone)]
pub(super) struct AiCoAuthorRuntime {
    state_path: PathBuf,
    aster_dir: PathBuf,
    prompt: String,
    document: EditorDocument,
    outcome: Option<AuthoringOutcome>,
    transactions: Vec<AiCoAuthorTransaction>,
    capability_operation_id: String,
    readback_tool_name: String,
    loaded_from_aster: bool,
    save_count: u32,
    last_status: String,
    last_error: String,
    last_aster_seq: u64,
    last_aster_graph_rows: usize,
    last_aster_verified: bool,
    calyx_probe: CalyxAvailabilityProbe,
    calyx_probe_cache: CalyxProbeCache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::main_editor_ui) struct AiCoAuthorEvidence {
    pub(in crate::main_editor_ui) state_summary: String,
    pub(in crate::main_editor_ui) store_summary: String,
    pub(in crate::main_editor_ui) aster_summary: String,
    pub(in crate::main_editor_ui) last_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AiCoAuthorTransaction {
    action: String,
    object_id: Option<String>,
    before_object_count: usize,
    after_object_count: usize,
    admission_code: String,
    note: String,
}

impl AiCoAuthorRuntime {
    pub(super) fn from_env() -> Result<Self, EngineError> {
        let state_path = state_path_from_env()?;
        let aster_dir = aster_dir_from_env()?;
        Self::new(state_path, aster_dir)
    }

    fn new(state_path: PathBuf, aster_dir: PathBuf) -> Result<Self, EngineError> {
        let (operation_id, readback_tool) = capability_route()?;
        let calyx_probe = CalyxAvailabilityProbe::from_env("editor.ai_coauthor");
        let calyx_probe_cache =
            CalyxProbeCache::new("ai_coauthor", "AI co-author", &calyx_probe, CALYX_PROBE_TTL)?;
        if let Some(readback) = read_aster_state(&aster_dir)? {
            let record = parse_state(&readback.bytes)?;
            validate_state_record(&record, &operation_id, &readback_tool)?;
            verify_json_mirror_if_present(&state_path, &readback.bytes)?;
            return Ok(Self::from_record(FromRecordArgs {
                state_path,
                aster_dir,
                record,
                loaded_from_aster: true,
                save_count: 0,
                readback,
                calyx_probe,
                calyx_probe_cache,
            }));
        }
        if state_path.exists() {
            return invalid_ai(
                "state.load",
                format!(
                    "{} exists but Aster source row {}/{} is absent",
                    state_path.display(),
                    AI_ASTER_COLLECTION,
                    String::from_utf8_lossy(AI_ASTER_ROW)
                ),
            );
        }
        let mut runtime = Self {
            state_path,
            aster_dir,
            prompt: default_prompt(),
            document: EditorDocument::new("calyx.editor.ai_coauthor.issue1230", "rev-0001")?,
            outcome: None,
            transactions: Vec::new(),
            capability_operation_id: operation_id,
            readback_tool_name: readback_tool,
            loaded_from_aster: false,
            save_count: 0,
            last_status: "ready".to_string(),
            last_error: String::new(),
            last_aster_seq: 0,
            last_aster_graph_rows: 0,
            last_aster_verified: false,
            calyx_probe,
            calyx_probe_cache,
        };
        runtime.save_state()?;
        Ok(runtime)
    }

    fn from_record(args: FromRecordArgs) -> Self {
        let FromRecordArgs {
            state_path,
            aster_dir,
            record,
            loaded_from_aster,
            save_count,
            readback,
            calyx_probe,
            calyx_probe_cache,
        } = args;
        Self {
            state_path,
            aster_dir,
            prompt: record.prompt,
            document: record.document,
            outcome: record.outcome,
            transactions: record.transactions,
            capability_operation_id: record.capability_operation_id,
            readback_tool_name: record.readback_tool_name,
            loaded_from_aster,
            save_count,
            last_status: record.last_status,
            last_error: record.last_error,
            last_aster_seq: readback.seq,
            last_aster_graph_rows: readback.graph_rows,
            last_aster_verified: true,
            calyx_probe,
            calyx_probe_cache,
        }
    }

    pub(super) fn prompt_mut(&mut self) -> &mut String {
        &mut self.prompt
    }

    pub(super) fn progress_fraction(&self) -> f32 {
        match self.outcome.as_ref().map(|outcome| outcome.status) {
            Some(AuthoringStatus::Admitted) => 1.0,
            Some(AuthoringStatus::Provisional) => 0.56,
            Some(AuthoringStatus::Refused) => 0.22,
            None => 0.05,
        }
    }

    pub(super) fn progress_label(&self) -> String {
        match &self.outcome {
            Some(outcome) => outcome.admission_code.clone(),
            None => "Awaiting grounded prompt".to_string(),
        }
    }

    /// Draw-path refresh (#1461): NON-BLOCKING. Picks up any finished background probe and, if the
    /// cached panel is stale, kicks off a fresh background probe. Never touches the network on the
    /// UI thread — replaces the per-frame blocking probe that stalled the editor up to the probe
    /// timeout every frame while Calyx was unreachable.
    pub(super) fn refresh_calyx_feature(&mut self) -> Result<(), EngineError> {
        if let Some(message) = self.calyx_probe_cache.poll(&self.calyx_probe) {
            self.last_error = message;
        }
        Ok(())
    }

    pub(super) fn calyx_feature_enabled(&self) -> bool {
        self.calyx_probe_cache.panel().enabled
    }

    pub(super) fn calyx_disabled_reason(&self) -> &str {
        &self.calyx_probe_cache.panel().disabled_reason
    }

    pub(super) fn generate_proposal(&mut self) -> Result<(), EngineError> {
        self.require_calyx_feature()?;
        let request = GroundedAbilityRequest {
            request_id: "issue1230-ai-coauthor-request".to_string(),
            prompt: self.prompt.clone(),
            seed: 424242,
            requested_by: "litd-editor-ai-coauthor".to_string(),
        };
        let outcome =
            generate_grounded_ability(&request, &authoring_corpus(), &authoring_assay(), &ward())?;
        self.last_error.clear();
        self.last_status = format!("proposal_{:?}_{}", outcome.status, outcome.admission_code);
        info!(
            admission_code = %outcome.admission_code,
            status = ?outcome.status,
            "ai co-author generated grounded proposal"
        );
        self.outcome = Some(outcome);
        self.save_state()
    }

    pub(super) fn apply_to_world(&mut self) -> Result<(), EngineError> {
        self.require_calyx_feature()?;
        let outcome = self
            .outcome
            .as_ref()
            .ok_or_else(|| ai_error("apply", "generate an admitted proposal before apply"))?;
        if outcome.status != AuthoringStatus::Admitted {
            return Err(ai_error(
                "apply",
                format!(
                    "proposal status {:?} cannot be applied; admission_code={}",
                    outcome.status, outcome.admission_code
                ),
            ));
        }
        let proposal = outcome
            .proposal
            .as_ref()
            .ok_or_else(|| ai_error("apply.proposal", "admitted outcome has no proposal"))?;
        let lowered = outcome
            .lowered
            .as_ref()
            .ok_or_else(|| ai_error("apply.lowered", "admitted outcome has no lowered TOML"))?;
        let lowered_doc =
            verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
        let object = object_for_proposal(proposal, outcome, &lowered.fingerprint)?;
        let before = self.document.objects.len();
        let mut next = self.document.clone();
        next.upsert_object(object.clone())?;
        next.validate()?;
        self.document = next;
        self.transactions.push(AiCoAuthorTransaction {
            action: "apply_to_world".to_string(),
            object_id: Some(object.id.clone()),
            before_object_count: before,
            after_object_count: self.document.objects.len(),
            admission_code: outcome.admission_code.clone(),
            note: format!(
                "ability={} damage={} cooldown={}",
                lowered_doc.ability.id,
                lowered_doc.ability.damage,
                lowered_doc.ability.cooldown_seconds
            ),
        });
        self.last_status = format!("applied_object={}", object.id);
        self.last_error.clear();
        self.save_state()
    }

    pub(super) fn reject_proposal(&mut self) -> Result<(), EngineError> {
        let before = self.document.objects.len();
        let admission = self
            .outcome
            .as_ref()
            .map(|outcome| outcome.admission_code.clone())
            .unwrap_or_else(|| "none".to_string());
        self.outcome = None;
        self.transactions.push(AiCoAuthorTransaction {
            action: "reject_proposal".to_string(),
            object_id: None,
            before_object_count: before,
            after_object_count: self.document.objects.len(),
            admission_code: admission,
            note: "cleared proposal without mutating document".to_string(),
        });
        self.last_status = "proposal_rejected".to_string();
        self.last_error.clear();
        self.save_state()
    }

    pub(super) fn undo_last_apply(&mut self) -> Result<(), EngineError> {
        let object_id = self
            .last_live_object_id()
            .ok_or_else(|| ai_error("undo", "no applied AI object exists to undo"))?;
        let before = self.document.objects.len();
        let mut next = self.document.clone();
        let removed = next.remove_object(&object_id)?;
        next.validate()?;
        self.document = next;
        self.transactions.push(AiCoAuthorTransaction {
            action: "undo_apply".to_string(),
            object_id: Some(object_id.clone()),
            before_object_count: before,
            after_object_count: self.document.objects.len(),
            admission_code: "undo".to_string(),
            note: format!("removed {}", removed.kind),
        });
        self.last_status = format!("undone_object={object_id}");
        self.last_error.clear();
        self.save_state()
    }

    pub(super) fn hand_edit_last_object(&mut self, label: &str) -> Result<(), EngineError> {
        if label.trim().is_empty() {
            return Err(ai_error(
                "hand_edit.label",
                "hand edit label must be non-empty",
            ));
        }
        let object_id = self
            .last_live_object_id()
            .ok_or_else(|| ai_error("hand_edit", "no applied AI object exists to edit"))?;
        let before = self.document.objects.len();
        let mut next = self.document.clone();
        let object = next.objects.get_mut(&object_id).ok_or_else(|| {
            ai_error(
                "hand_edit.object",
                format!("applied object {object_id} disappeared before edit"),
            )
        })?;
        // Absent revision = a fresh object (start at 0). A PRESENT but unparseable
        // value is data corruption: fail loud instead of silently resetting the
        // counter to 0 and losing edit-revision continuity (#1489, #800 §4).
        let revision = match object.properties.get("hand_edit_revision") {
            None => 0,
            Some(value) => value.parse::<u32>().map_err(|error| {
                ai_error(
                    "hand_edit.revision",
                    format!("hand_edit_revision property is corrupt ('{value}'): {error}"),
                )
            })?,
        }
        .saturating_add(1);
        object
            .properties
            .insert("display_name".to_string(), label.trim().to_string());
        object
            .properties
            .insert("hand_edit_revision".to_string(), revision.to_string());
        next.validate()?;
        self.document = next;
        self.transactions.push(AiCoAuthorTransaction {
            action: "hand_edit".to_string(),
            object_id: Some(object_id.clone()),
            before_object_count: before,
            after_object_count: self.document.objects.len(),
            admission_code: "manual_edit".to_string(),
            note: format!("display_name={}", label.trim()),
        });
        self.last_status = format!("hand_edited_object={object_id};revision={revision}");
        self.last_error.clear();
        self.save_state()
    }

    pub(super) fn evidence(&self) -> AiCoAuthorEvidence {
        let admission = self
            .outcome
            .as_ref()
            .map(|outcome| outcome.admission_code.as_str())
            .unwrap_or("none");
        let status = self
            .outcome
            .as_ref()
            .map(|outcome| format!("{:?}", outcome.status))
            .unwrap_or_else(|| "None".to_string());
        AiCoAuthorEvidence {
            state_summary: format!(
                "status={status};admission={admission};objects={};last_object={};transactions={};last_status={};calyx={}",
                self.document.objects.len(),
                self.last_live_object_id()
                    .unwrap_or_else(|| "none".to_string()),
                self.transactions.len(),
                self.last_status,
                self.calyx_probe_cache.panel().status
            ),
            store_summary: format!(
                "legacy_json_path={};loaded_from_aster={};save_count={};operation={};readback_tool={}",
                self.state_path.display(),
                self.loaded_from_aster,
                self.save_count,
                self.capability_operation_id,
                self.readback_tool_name
            ),
            aster_summary: format!(
                "dir={};collection={};row={};seq={};graph_rows={};verified={}",
                self.aster_dir.display(),
                AI_ASTER_COLLECTION,
                String::from_utf8_lossy(AI_ASTER_ROW),
                self.last_aster_seq,
                self.last_aster_graph_rows,
                self.last_aster_verified
            ),
            last_error: self.last_error.clone(),
        }
    }
}

#[path = "main_editor_ai_coauthor_runtime_tail.rs"]
mod runtime_tail;

fn invalid_ai<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(ai_error(field, reason))
}

#[cfg(test)]
#[path = "main_editor_ai_coauthor_tests.rs"]
mod tests;
