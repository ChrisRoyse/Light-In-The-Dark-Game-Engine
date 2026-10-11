use serde::{Deserialize, Serialize};

pub const MODERATION_SAFETY_TOOL: &str = "litd.editor.project.manage_moderation_safety";
pub const MODERATION_SAFETY_STATE_SCHEMA: &str = "litd.ecosystem.moderation_safety.state.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModerationSafetyAction {
    SubmitReport,
    AssignServerRole,
    ApplyAdminAction,
    ResolveReport,
    SubmitAppeal,
    DecideAppeal,
    SetAccessRule,
    EvaluateConnection,
    RecordHostControl,
    SetRoleplayPolicy,
    EvaluateRoleplayPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    Harassment,
    Hate,
    Cheating,
    Spam,
    Exploit,
    InappropriateContent,
    UnsafeWorld,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Open,
    Actioned,
    Dismissed,
    Appealed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerRole {
    Moderator,
    Admin,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminActionKind {
    Warn,
    Mute,
    Kick,
    Ban,
    LockWorld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppealStatus {
    Submitted,
    Upheld,
    Lifted,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessRuleKind {
    Allow,
    Ban,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostControlKind {
    Kick,
    Mute,
    Ban,
    LockWorld,
    UnlockWorld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayPolicyMode {
    Casual,
    Standard,
    Hardcore,
    PrivilegedHardcore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleplayPolicyVerdict {
    Reward,
    Neutral,
    Punish,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationSafetyArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub moderation_state_path: String,
    pub moderation_state_id: String,
    pub social_state_path: String,
    pub catalog_path: String,
    pub multiplayer_state_path: String,
    pub action: ModerationSafetyAction,
    #[serde(default)]
    pub lobby_id: Option<String>,
    #[serde(default)]
    pub report_id: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub rule_id: Option<String>,
    #[serde(default)]
    pub decision_id: Option<String>,
    #[serde(default)]
    pub control_id: Option<String>,
    #[serde(default)]
    pub policy_id: Option<String>,
    #[serde(default)]
    pub evaluation_id: Option<String>,
    #[serde(default)]
    pub supersedes_policy_id: Option<String>,
    #[serde(default)]
    pub assignment_id: Option<String>,
    #[serde(default)]
    pub appeal_id: Option<String>,
    #[serde(default)]
    pub actor_account_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub target_account_id: Option<String>,
    #[serde(default)]
    pub reporter_account_id: Option<String>,
    #[serde(default)]
    pub reported_account_id: Option<String>,
    #[serde(default)]
    pub content_address: Option<String>,
    #[serde(default)]
    pub reason: Option<ReportReason>,
    #[serde(default)]
    pub severity: Option<u8>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub evidence_sha256: Option<String>,
    #[serde(default)]
    pub role: Option<ServerRole>,
    #[serde(default)]
    pub admin_action_kind: Option<AdminActionKind>,
    #[serde(default)]
    pub duration_minutes: Option<u32>,
    #[serde(default)]
    pub resolution_status: Option<ReportStatus>,
    #[serde(default)]
    pub resolution_note: Option<String>,
    #[serde(default)]
    pub appeal_text: Option<String>,
    #[serde(default)]
    pub appeal_status: Option<AppealStatus>,
    #[serde(default)]
    pub access_rule_kind: Option<AccessRuleKind>,
    #[serde(default)]
    pub host_control_kind: Option<HostControlKind>,
    #[serde(default)]
    pub roleplay_policy_mode: Option<RoleplayPolicyMode>,
    #[serde(default)]
    pub strictness_milli: Option<u16>,
    #[serde(default)]
    pub reward_multiplier_milli: Option<u16>,
    #[serde(default)]
    pub punishment_multiplier_milli: Option<u16>,
    #[serde(default)]
    pub min_evidence_refs: Option<u8>,
    #[serde(default)]
    pub rp_fit_score_milli: Option<u16>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub decision_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentReportRecord {
    pub report_id: String,
    pub lobby_id: String,
    pub reporter_account_id: String,
    pub reported_account_id: Option<String>,
    pub content_address: Option<String>,
    pub reason: ReportReason,
    pub severity: u8,
    pub description: String,
    pub evidence_sha256: String,
    pub status: ReportStatus,
    pub resolution_note: Option<String>,
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRoleAssignmentRecord {
    pub assignment_id: String,
    pub lobby_id: String,
    pub account_id: String,
    pub role: ServerRole,
    pub granted_by_account_id: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminActionRecord {
    pub action_id: String,
    pub report_id: Option<String>,
    pub lobby_id: String,
    pub actor_account_id: String,
    pub target_account_id: Option<String>,
    pub content_address: Option<String>,
    pub action_kind: AdminActionKind,
    pub reason: String,
    pub duration_minutes: Option<u32>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationAppealRecord {
    pub appeal_id: String,
    pub action_id: String,
    pub submitted_by_account_id: String,
    pub status: AppealStatus,
    pub appeal_text: String,
    pub decided_by_account_id: Option<String>,
    pub decision_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRuleRecord {
    pub rule_id: String,
    pub lobby_id: String,
    pub account_id: String,
    pub rule_kind: AccessRuleKind,
    pub reason: String,
    pub set_by_account_id: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionDecisionRecord {
    pub decision_id: String,
    pub lobby_id: String,
    pub account_id: String,
    pub evaluated_by_account_id: String,
    pub allowed: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostControlRecord {
    pub control_id: String,
    pub lobby_id: String,
    pub actor_account_id: String,
    pub target_account_id: Option<String>,
    pub control_kind: HostControlKind,
    pub reason: String,
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPolicyRecord {
    pub policy_id: String,
    pub lobby_id: String,
    pub set_by_account_id: String,
    pub mode: RoleplayPolicyMode,
    pub strictness_milli: u16,
    pub reward_threshold_milli: u16,
    pub punishment_threshold_milli: u16,
    pub reward_multiplier_milli: u16,
    pub punishment_multiplier_milli: u16,
    pub min_evidence_refs: u8,
    pub supersedes_policy_id: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayPolicyEvaluationRecord {
    pub evaluation_id: String,
    pub policy_id: String,
    pub lobby_id: String,
    pub account_id: String,
    pub evaluated_by_account_id: String,
    pub rp_fit_score_milli: u16,
    pub verdict: RoleplayPolicyVerdict,
    pub reward_delta_milli: u16,
    pub punishment_delta_milli: u16,
    pub evidence_sha256: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationSafetyState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub social_state_path: String,
    pub catalog_path: String,
    pub multiplayer_state_path: String,
    pub report_count: usize,
    pub role_assignment_count: usize,
    pub admin_action_count: usize,
    pub appeal_count: usize,
    pub access_rule_count: usize,
    pub connection_decision_count: usize,
    pub host_control_count: usize,
    pub roleplay_policy_count: usize,
    pub roleplay_evaluation_count: usize,
    pub open_report_count: usize,
    pub active_restriction_count: usize,
    pub allow_rule_count: usize,
    pub ban_rule_count: usize,
    pub active_roleplay_policy_count: usize,
    pub reward_verdict_count: usize,
    pub punishment_verdict_count: usize,
    pub reports: Vec<ContentReportRecord>,
    pub role_assignments: Vec<ServerRoleAssignmentRecord>,
    pub admin_actions: Vec<AdminActionRecord>,
    pub appeals: Vec<ModerationAppealRecord>,
    pub access_rules: Vec<AccessRuleRecord>,
    pub connection_decisions: Vec<ConnectionDecisionRecord>,
    pub host_controls: Vec<HostControlRecord>,
    pub roleplay_policies: Vec<RoleplayPolicyRecord>,
    pub roleplay_evaluations: Vec<RoleplayPolicyEvaluationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationSafetyStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: ModerationSafetyState,
}
