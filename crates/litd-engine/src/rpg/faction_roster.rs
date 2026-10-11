//! Durable, exclusive faction-role reservations (#1755).
//!
//! The policy is referenced by a stable ClassSpec id. Roster state and audit
//! rows live in Calyx; every admission/release/transfer reads and writes while
//! holding the database's cross-process exclusive transaction lock.

use serde::{Deserialize, Serialize};

use crate::calyx_persistence::{
    EngineCalyxDatabase, EngineCalyxPersistenceError, EngineCalyxRecord, EngineCalyxRecordKind,
};

pub const FACTION_ROSTER_SCHEMA: &str = "litd.rpg.faction_roster.v1";
pub const BERSERKER_CLAN_CHIEF_POLICY_ID: &str = "policy.faction.berserker_clan_chief.v1";
pub const BERSERKER_CLAN_CHIEF_RANK: u16 = 51;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRosterPolicy {
    pub schema: String,
    pub policy_id: String,
    pub class_spec_id: String,
    pub role_id: String,
    pub rank: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRosterHolder {
    pub candidate_id: String,
    pub class_spec_id: String,
    pub rank: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRosterState {
    pub schema: String,
    pub clan_id: String,
    pub policy_id: String,
    pub revision: u64,
    pub holder: Option<FactionRosterHolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRosterAudit {
    pub schema: String,
    pub event_id: String,
    pub action: String,
    pub clan_id: String,
    pub candidate_id: String,
    pub previous_holder_id: Option<String>,
    pub new_holder_id: Option<String>,
    pub outcome: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRosterDecision {
    pub action: String,
    pub clan_id: String,
    pub candidate_id: String,
    pub accepted: bool,
    pub state: FactionRosterState,
    pub audit: FactionRosterAudit,
}

#[derive(Debug, thiserror::Error)]
pub enum FactionRosterError {
    #[error("faction roster persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("faction roster request is invalid: {0}")]
    Invalid(String),
    #[error("faction roster request refused: {0}")]
    Refused(String),
}

pub fn persist_faction_roster_policy(
    database: &EngineCalyxDatabase,
    policy: &FactionRosterPolicy,
) -> Result<(), FactionRosterError> {
    validate_policy(policy)?;
    let payload = serde_json::to_value(policy)
        .map_err(|error| FactionRosterError::Invalid(error.to_string()))?;
    database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::FactionRosterPolicy,
        &policy.policy_id,
        payload,
        serde_json::json!({"source": "ClassSpec", "class_spec_id": policy.class_spec_id}),
    ))?;
    Ok(())
}

pub fn read_faction_roster_state(
    database: &EngineCalyxDatabase,
    clan_id: &str,
    policy_id: &str,
) -> Result<FactionRosterState, FactionRosterError> {
    validate_id("clan_id", clan_id)?;
    validate_id("policy_id", policy_id)?;
    match database.get_record(EngineCalyxRecordKind::FactionRosterState, clan_id) {
        Ok(row) => decode(row.payload),
        Err(EngineCalyxPersistenceError::MissingRecord { .. }) => {
            Ok(empty_state(clan_id, policy_id))
        }
        Err(error) => Err(error.into()),
    }
}

pub fn admit_clan_chief(
    database: &EngineCalyxDatabase,
    policy_id: &str,
    clan_id: &str,
    candidate_id: &str,
    action_id: &str,
) -> Result<FactionRosterDecision, FactionRosterError> {
    mutate(
        database,
        "admit",
        policy_id,
        clan_id,
        candidate_id,
        action_id,
        |state, policy| {
            if state.holder.is_some() {
                return refusal("clan already has a rank-51 holder");
            }
            state.revision += 1;
            state.holder = Some(FactionRosterHolder {
                candidate_id: candidate_id.to_string(),
                class_spec_id: policy.class_spec_id.clone(),
                rank: policy.rank,
            });
            Ok(None)
        },
    )
}

pub fn release_clan_chief(
    database: &EngineCalyxDatabase,
    policy_id: &str,
    clan_id: &str,
    candidate_id: &str,
    action_id: &str,
) -> Result<FactionRosterDecision, FactionRosterError> {
    mutate(
        database,
        "release",
        policy_id,
        clan_id,
        candidate_id,
        action_id,
        |state, _| {
            if state
                .holder
                .as_ref()
                .map(|holder| holder.candidate_id.as_str())
                != Some(candidate_id)
            {
                return refusal("release requires the current holder id");
            }
            state.revision += 1;
            state.holder = None;
            Ok(None)
        },
    )
}

pub fn transfer_clan_chief(
    database: &EngineCalyxDatabase,
    policy_id: &str,
    clan_id: &str,
    current_holder_id: &str,
    next_holder_id: &str,
    action_id: &str,
) -> Result<FactionRosterDecision, FactionRosterError> {
    if next_holder_id.trim().is_empty() || current_holder_id == next_holder_id {
        return Err(FactionRosterError::Invalid(
            "transfer holder ids must be distinct and nonempty".to_string(),
        ));
    }
    mutate(
        database,
        "transfer",
        policy_id,
        clan_id,
        next_holder_id,
        action_id,
        |state, policy| {
            if state
                .holder
                .as_ref()
                .map(|holder| holder.candidate_id.as_str())
                != Some(current_holder_id)
            {
                return refusal("transfer requires the current holder id");
            }
            state.revision += 1;
            state.holder = Some(FactionRosterHolder {
                candidate_id: next_holder_id.to_string(),
                class_spec_id: policy.class_spec_id.clone(),
                rank: policy.rank,
            });
            Ok(None)
        },
    )
}

fn mutate<F>(
    database: &EngineCalyxDatabase,
    action: &str,
    policy_id: &str,
    clan_id: &str,
    candidate_id: &str,
    action_id: &str,
    change: F,
) -> Result<FactionRosterDecision, FactionRosterError>
where
    F: FnOnce(
        &mut FactionRosterState,
        &FactionRosterPolicy,
    ) -> Result<Option<String>, FactionRosterError>,
{
    validate_id("policy_id", policy_id)?;
    validate_id("clan_id", clan_id)?;
    validate_id("candidate_id", candidate_id)?;
    validate_id("action_id", action_id)?;
    database
        .with_exclusive_transaction("faction_roster", |database| {
            let policy: FactionRosterPolicy = decode(
                database
                    .get_record(EngineCalyxRecordKind::FactionRosterPolicy, policy_id)?
                    .payload,
            )?;
            validate_policy(&policy)?;
            let before = read_faction_roster_state(database, clan_id, policy_id)?;
            let previous_holder_id = before
                .holder
                .as_ref()
                .map(|holder| holder.candidate_id.clone());
            let mut after = before.clone();
            let reason = match change(&mut after, &policy) {
                Ok(reason) => reason,
                Err(FactionRosterError::Refused(reason)) => Some(reason),
                Err(error) => return Err(error),
            };
            let accepted = reason.is_none();
            if accepted {
                database.put_record(&EngineCalyxRecord::new(
                    EngineCalyxRecordKind::FactionRosterState,
                    clan_id,
                    serde_json::to_value(&after).map_err(|error| {
                        EngineCalyxPersistenceError::Store {
                            op: "serialize_roster_state".to_string(),
                            reason: error.to_string(),
                        }
                    })?,
                    serde_json::json!({"source": "faction_roster_transaction"}),
                ))?;
            }
            let audit = FactionRosterAudit {
                schema: FACTION_ROSTER_SCHEMA.to_string(),
                event_id: action_id.to_string(),
                action: action.to_string(),
                clan_id: clan_id.to_string(),
                candidate_id: candidate_id.to_string(),
                previous_holder_id,
                new_holder_id: after
                    .holder
                    .as_ref()
                    .map(|holder| holder.candidate_id.clone()),
                outcome: if accepted { "accepted" } else { "refused" }.to_string(),
                reason,
            };
            database.put_record(&EngineCalyxRecord::new(
                EngineCalyxRecordKind::FactionRosterAudit,
                action_id,
                serde_json::to_value(&audit).map_err(|error| {
                    EngineCalyxPersistenceError::Store {
                        op: "serialize_roster_audit".to_string(),
                        reason: error.to_string(),
                    }
                })?,
                serde_json::json!({"source": "faction_roster_transaction"}),
            ))?;
            Ok(FactionRosterDecision {
                action: action.to_string(),
                clan_id: clan_id.to_string(),
                candidate_id: candidate_id.to_string(),
                accepted,
                state: after,
                audit,
            })
        })
}

fn refusal(reason: &str) -> Result<Option<String>, FactionRosterError> {
    Ok(Some(reason.to_string()))
}
fn empty_state(clan_id: &str, policy_id: &str) -> FactionRosterState {
    FactionRosterState {
        schema: FACTION_ROSTER_SCHEMA.to_string(),
        clan_id: clan_id.to_string(),
        policy_id: policy_id.to_string(),
        revision: 0,
        holder: None,
    }
}
fn decode<T: for<'de> Deserialize<'de>>(value: serde_json::Value) -> Result<T, FactionRosterError> {
    serde_json::from_value(value)
        .map_err(|error| FactionRosterError::Invalid(format!("decode faction roster row: {error}")))
}
fn validate_id(field: &str, value: &str) -> Result<(), FactionRosterError> {
    if value.trim().is_empty() {
        Err(FactionRosterError::Invalid(format!(
            "{field} must be nonempty"
        )))
    } else {
        Ok(())
    }
}
fn validate_policy(policy: &FactionRosterPolicy) -> Result<(), FactionRosterError> {
    validate_id("policy_id", &policy.policy_id)?;
    validate_id("class_spec_id", &policy.class_spec_id)?;
    if policy.rank != BERSERKER_CLAN_CHIEF_RANK || policy.role_id != "role.berserker.clan_chief" {
        return Err(FactionRosterError::Invalid(
            "policy is not the rank-51 Berserker Clan-Chief policy".to_string(),
        ));
    }
    Ok(())
}
