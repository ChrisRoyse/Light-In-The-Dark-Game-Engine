use std::{collections::BTreeMap, path::PathBuf};

use litd_engine::wasm_script::WasmScriptArtifact;
use serde::{Deserialize, Serialize};

use crate::sound_editor::{
    NamedSoundVariable, SoundCategory, SoundEditorRecord, TriggerSoundBinding,
};

/// WC3 GUI-parity variable value types. `UnitId`/`RegionId` are the unit-ref and
/// region-ref handle types; `SoundVariable` predates this and is retained so old
/// persisted rows keep parsing. `UnitGroup`, `Point`, `Timer`, and `Player` were
/// added for #833 to complete WC3 parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerValueType {
    Integer,
    Real,
    Boolean,
    String,
    UnitId,
    RegionId,
    SoundVariable,
    UnitGroup,
    Point,
    Timer,
    Player,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerVariableScope {
    Global,
    Local,
}

/// A typed WC3-style trigger variable. A scalar carries at most `initial_value`;
/// a fixed-size array carries `array_size = Some(n>0)` and either an empty
/// `array_initial_values` (every element default-initialized) or exactly `n`
/// per-element initial values. Every initial value is type-checked at authoring
/// time (#833) so a type mismatch is a typed error, never a runtime surprise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerVariable {
    pub name: String,
    pub value_type: TriggerValueType,
    pub scope: TriggerVariableScope,
    pub is_array: bool,
    pub initial_value: Option<String>,
    #[serde(default)]
    pub array_size: Option<u32>,
    #[serde(default)]
    pub array_initial_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcaGraph {
    pub trigger_id: String,
    pub display_name: String,
    /// Runtime-editable enable/disable flag (WC3 "Enable/Disable Trigger").
    pub enabled: bool,
    /// Slash-delimited folder/category path in the trigger tree (#835).
    pub folder_path: Option<String>,
    /// WC3 "initially on": whether the trigger is armed at map start. Distinct
    /// from `enabled`, which a running trigger can toggle. Defaults to `true`
    /// so pre-#835 persisted graphs keep their WC3-default arming.
    #[serde(default = "default_true")]
    pub initially_on: bool,
    /// Free-form author comment attached to the trigger (#835).
    #[serde(default)]
    pub comment: Option<String>,
    pub events: Vec<EcaNode>,
    pub conditions: Vec<EcaNode>,
    pub actions: Vec<EcaNode>,
    pub wasm_escape: Option<WasmEscape>,
}

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcaNode {
    pub node_id: String,
    pub operation: String,
    pub order: u32,
    pub args: BTreeMap<String, String>,
    pub variable_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmEscape {
    pub script_id: String,
    pub wat_source: String,
    pub wat_source_sha256: Option<String>,
    pub entry_point: String,
    pub fuel_budget: u64,
    pub approved_reason: String,
    pub gui_metadata_json: Option<String>,
    pub artifact: Option<WasmScriptArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeCueKind {
    Timer,
    Camera,
    Dialog,
    Fog,
    Effect,
    Sound,
    Quest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCue {
    pub cue_id: String,
    pub trigger_id: String,
    pub kind: RuntimeCueKind,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertVariableArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub variable: TriggerVariable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertEcaGraphArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub graph: EcaGraph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertRuntimeCueArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub cue: RuntimeCue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadTriggerRuntimeArgs {
    pub state_path: PathBuf,
}

/// #835: mutate only the organization fields of an existing trigger graph
/// (folder/category, enable/disable, initially-on, comment) without re-authoring
/// its events/conditions/actions. Every supplied field replaces the current value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetTriggerOrganizationArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub trigger_id: String,
    pub enabled: bool,
    pub initially_on: bool,
    #[serde(default)]
    pub folder_path: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

/// #835/#736: which direction of the Object Manager cross-reference to answer.
/// Both are computed from the REAL persisted trigger state, never a side index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrossRefQuery {
    /// List every trigger that references the given object.
    TriggersReferencingObject {
        object_kind: CrossRefObjectKind,
        object_id: String,
    },
    /// List every object the given trigger references.
    ObjectsReferencedByTrigger { trigger_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossRefObjectKind {
    Variable,
    Unit,
    UnitType,
    Region,
    Player,
    Timer,
    UnitGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCrossRefArgs {
    pub state_path: PathBuf,
    pub query: CrossRefQuery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportSoundArgs {
    pub record_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_record: Option<SoundEditorRecord>,
    pub source_path: PathBuf,
    pub category: SoundCategory,
    pub asset_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertSoundVariableArgs {
    pub record_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_record: Option<SoundEditorRecord>,
    pub sound: NamedSoundVariable,
    pub trigger_binding: Option<TriggerSoundBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewSoundArgs {
    pub record_path: PathBuf,
    pub runtime_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub variable_name: String,
    pub cursor_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadSoundEditorArgs {
    pub record_path: PathBuf,
    pub runtime_path: Option<PathBuf>,
}
