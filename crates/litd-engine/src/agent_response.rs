//! The contract for what a language model is allowed to say to this engine (#1548).
//!
//! # Why this module exists
//!
//! The prompt is content *we* authored. The response is **untrusted input from a third-party
//! system**, and it was previously lowered into authoritative sim orders with nothing standing
//! between the two — OWASP **LLM05, Improper Output Handling**.
//!
//! The defense is **structural, not statistical**. A model cannot emit a [`SimOrder`]. It can
//! only emit an [`EngineAgentProposal`]: a *proposal* over a **closed verb enum** whose every
//! numeric field carries a **hard range** enforced by a real JSON Schema validator, and whose
//! only free-text field terminates in a ledger row. There is deliberately no
//! `impl From<EngineAgentProposal> for SimOrder`; the sole mapping lives in a private `lower()`
//! inside [`crate::agent_response_gate`], behind the admission choke point.
//!
//! That is the load-bearing property, and it is worth stating precisely: **a fully jailbroken
//! model's worst case is a legal, in-envelope order.** It cannot express an illegal one, because
//! an illegal one is not a value of this type. Detectors (Ward's cosine novelty scoring) are a
//! later, weaker layer — see [`crate::agent_response_gate`].
//!
//! # Two ceilings, and why they are different
//!
//! * The **schema ceilings** in this module are CODE. They cap the blast radius of the type
//!   itself and change only with a recompile.
//! * The **envelope** ([`crate::agent_response_gate::EngineAgentOrderEnvelope`]) is DATA. It is
//!   per-deployment, strictly tighter, and tunable by designers.
//!
//! A value can therefore be schema-valid and still envelope-refused. Both layers are real and
//! both are exercised by the FSV.
//!
//! [`SimOrder`]: crate::sim_order::SimOrder

use std::sync::OnceLock;

use litd_hash::sha256_hex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Schema-level ceiling on any single velocity/acceleration axis, in milli-units per tick.
///
/// CODE, not policy. `8_000` milli = 8 units/tick on one axis. The runtime envelope is far
/// tighter; this only bounds what the *type* can represent.
pub const PROPOSAL_MAX_AXIS_MILLI: i32 = 8_000;
/// Schema-level ceiling on proposed attack damage, in milli-units.
pub const PROPOSAL_MAX_DAMAGE_MILLI: u32 = 5_000;
/// Schema-level ceiling on any body index a proposal may name.
pub const PROPOSAL_MAX_BODY_INDEX: u32 = 255;
/// Schema-level ceiling on the length of the single free-text field.
pub const PROPOSAL_MAX_RATIONALE_CHARS: u32 = 480;

/// The JSON Schema dialect the proposal schema is generated in and validated against.
///
/// Pinned explicitly. Auto-detection of a dialect from an untrusted document is exactly the kind
/// of implicit behaviour this module exists to eliminate.
pub const PROPOSAL_SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The one thing a model is permitted to produce.
///
/// A **proposal**, never a command. Note what is *absent*: no tick, no order source, no sequence,
/// no file path, no script, no entity selector, no free-form numeric vector. Everything about
/// *when*, *by whom*, and *to what* an order applies is authored — see
/// [`EngineAgentBinding`]. The model contributes a bounded [`EngineAgentVerb`] and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct EngineAgentProposal {
    /// Must EQUAL the binding's agent id. This is an assertion the model restates, not a
    /// selector it chooses: a mismatch is refused at the binding stage. The model has zero
    /// addressing power.
    pub agent_id: String,
    /// Must EQUAL the binding's target body. Same reasoning as `agent_id` — an agent may never
    /// aim an order at a body it was not bound to.
    #[schemars(range(min = 0, max = 255))]
    pub target_body: u32,
    /// The closed verb set. Externally tagged, so an unknown variant name is a hard serde error
    /// rather than a silently-dropped field.
    pub verb: EngineAgentVerb,
    /// The ONLY free-text field, and therefore the only place attacker-controlled bytes can
    /// survive the gate. It is what Ward scores. It terminates at a ledger row and a dialogue
    /// widget: it never reaches a path, an id, a query, an eval, or any numeric field.
    ///
    /// It exists on purpose. A model with nowhere to reason in prose smuggles reasoning into the
    /// constrained fields, which is precisely where it must not go.
    #[schemars(length(min = 1, max = 480))]
    pub rationale: String,
}

/// The closed verb vocabulary. Adding a variant is a deliberate, reviewable, one-PR act — that
/// cheapness is the pressure-release valve that stops anyone widening the schema instead.
///
/// Externally tagged with a struct payload per variant: serde rejects an unknown variant name
/// outright, and `deny_unknown_fields` is well-defined on each payload (it is a documented no-op
/// on internally-tagged and flattened representations, so neither is used here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum EngineAgentVerb {
    /// Do nothing. A first-class, in-vocabulary answer so the model never has to fake inaction
    /// by emitting a zero vector. Lowers to no order at all.
    Hold(EngineAgentHold),
    /// Replace the body's velocity.
    SetVelocity(EngineAgentVelocity),
    /// Add to the body's velocity.
    AddVelocity(EngineAgentVelocity),
    /// Set the body's acceleration.
    SetAcceleration(EngineAgentVelocity),
    /// Apply damage from one body to the bound body.
    Attack(EngineAgentAttack),
}

/// Payload for [`EngineAgentVerb::Hold`]. Empty by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineAgentHold {}

/// A bounded, integer-milli motion vector.
///
/// Milli-integers, never floats. The sim is Q32.32 fixed-point; a JSON float would be an
/// arbitrary-precision value that does not round-trip, which is a determinism/desync bug waiting
/// to happen. `milli / 1000` converts to `Fixed` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineAgentVelocity {
    /// X axis, milli-units per tick.
    #[schemars(range(min = -8000, max = 8000))]
    pub x_milli: i32,
    /// Y axis, milli-units per tick.
    #[schemars(range(min = -8000, max = 8000))]
    pub y_milli: i32,
    /// Z axis, milli-units per tick.
    #[schemars(range(min = -8000, max = 8000))]
    pub z_milli: i32,
}

/// A bounded attack proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineAgentAttack {
    /// The attacking body index.
    #[schemars(range(min = 0, max = 255))]
    pub attacker_body: u32,
    /// Damage, in milli-units.
    #[schemars(range(min = 0, max = 5000))]
    pub damage_milli: u32,
}

/// A stable tag for a verb, used by the envelope allowlist and the forensic ledger.
///
/// Not part of the wire contract — the model never sees or sends one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineAgentVerbTag {
    /// [`EngineAgentVerb::Hold`]
    Hold,
    /// [`EngineAgentVerb::SetVelocity`]
    SetVelocity,
    /// [`EngineAgentVerb::AddVelocity`]
    AddVelocity,
    /// [`EngineAgentVerb::SetAcceleration`]
    SetAcceleration,
    /// [`EngineAgentVerb::Attack`]
    Attack,
}

impl EngineAgentVerbTag {
    /// The stable wire/ledger spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::SetVelocity => "set_velocity",
            Self::AddVelocity => "add_velocity",
            Self::SetAcceleration => "set_acceleration",
            Self::Attack => "attack",
        }
    }
}

impl EngineAgentVerb {
    /// The verb's stable tag.
    pub const fn tag(&self) -> EngineAgentVerbTag {
        match self {
            Self::Hold(_) => EngineAgentVerbTag::Hold,
            Self::SetVelocity(_) => EngineAgentVerbTag::SetVelocity,
            Self::AddVelocity(_) => EngineAgentVerbTag::AddVelocity,
            Self::SetAcceleration(_) => EngineAgentVerbTag::SetAcceleration,
            Self::Attack(_) => EngineAgentVerbTag::Attack,
        }
    }

    /// Movement units per tick this verb would command, **rounded up**.
    ///
    /// Rounding up is deliberate. [`crate::calyx_ward_gate_support::movement_units`] truncates
    /// (1.9 units reports as 1), which is the conservative direction for *authored* content whose
    /// goldens depend on it, but the wrong direction for an *untrusted* proposal: a model asking
    /// for 1.999 units must not be measured as 1. Truncating here would hand the model a free
    /// sub-unit of movement above the envelope.
    pub const fn movement_units_ceil(&self) -> u32 {
        match self {
            Self::Hold(_) | Self::Attack(_) => 0,
            Self::SetVelocity(vector)
            | Self::AddVelocity(vector)
            | Self::SetAcceleration(vector) => vector.movement_units_ceil(),
        }
    }
}

impl EngineAgentVelocity {
    /// The largest axis magnitude, in whole units, rounded up.
    pub const fn movement_units_ceil(&self) -> u32 {
        let max_milli = max3(
            self.x_milli.unsigned_abs(),
            self.y_milli.unsigned_abs(),
            self.z_milli.unsigned_abs(),
        );
        max_milli.div_ceil(1_000)
    }
}

const fn max3(a: u32, b: u32, c: u32) -> u32 {
    let ab = if a > b { a } else { b };
    if ab > c { ab } else { c }
}

/// A single JSON Schema violation, with the JSON Pointer that located it.
///
/// Recorded in full in the admission ledger: a refusal must be diagnosable from the stored row
/// alone, without re-running the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAgentSchemaError {
    /// JSON Pointer into the instance, e.g. `/verb/set_velocity/x_milli`.
    pub instance_path: String,
    /// JSON Pointer into the schema, e.g. `/properties/verb/.../maximum`.
    pub schema_path: String,
    /// The validator's message.
    pub message: String,
}

/// The proposal's JSON Schema, generated once from the Rust type.
///
/// ONE source of truth: this same document is embedded in the prompt sent to the provider **and**
/// is the document the response is validated against. The type cannot drift from the contract,
/// because the contract is derived from the type.
pub fn proposal_schema() -> &'static serde_json::Value {
    static SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::to_value(schemars::schema_for!(EngineAgentProposal))
            .expect("proposal schema serializes")
    })
}

/// The compiled validator for [`proposal_schema`].
///
/// Draft 2020-12 is named explicitly rather than auto-detected. The `jsonschema` crate is built
/// with `default-features = false`: its default `resolve-http` feature would let a `$ref` inside
/// a schema become an outbound HTTP request, which is an SSRF surface inside the very component
/// whose job is to distrust remote input.
pub fn proposal_validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        jsonschema::draft202012::new(proposal_schema()).expect("proposal schema compiles")
    })
}

/// The proposal schema as canonical JSON text — the exact bytes embedded in the prompt.
pub fn proposal_schema_json() -> String {
    serde_json::to_string_pretty(proposal_schema()).expect("proposal schema renders")
}

/// SHA-256 of [`proposal_schema_json`].
///
/// Stamped into every admission ledger row so a forensic pass can tell "the model changed"
/// apart from "the contract changed".
pub fn proposal_schema_sha256() -> String {
    sha256_hex(proposal_schema_json().as_bytes())
}

/// Validate raw response text against the schema, then against the closed Rust types.
///
/// Two independent gates, in this order, and the order matters:
///
/// 1. **JSON Schema** — catches what serde structurally cannot. `x_milli: 250000` deserializes
///    happily into an `i32`; only `"maximum": 8000` rejects it. This is the entire reason the
///    `jsonschema` dependency exists.
/// 2. **serde** — produces the typed value. Closed enums and `deny_unknown_fields` mean nothing
///    beyond the contract survives. No `serde_json::Value` ever leaves this module.
///
/// All schema violations are collected, not just the first: a refusal row must show every way
/// the response was wrong.
pub fn validate_proposal_body(
    content: &str,
) -> Result<EngineAgentProposal, Vec<EngineAgentSchemaError>> {
    let instance = serde_json::from_str::<serde_json::Value>(content).map_err(|error| {
        vec![EngineAgentSchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("response body is not valid JSON: {error}"),
        }]
    })?;
    let violations = proposal_validator()
        .iter_errors(&instance)
        .map(|error| EngineAgentSchemaError {
            instance_path: error.instance_path().to_string(),
            schema_path: error.schema_path().to_string(),
            message: error.to_string(),
        })
        .collect::<Vec<_>>();
    if !violations.is_empty() {
        return Err(refine_verb_violations(&instance, violations));
    }
    serde_json::from_value::<EngineAgentProposal>(instance).map_err(|error| {
        vec![EngineAgentSchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("response body did not satisfy the closed Rust contract: {error}"),
        }]
    })
}

/// Turn the `oneOf` gate's opaque verdict into an exact one.
///
/// An externally-tagged enum compiles to `oneOf`, and a failing `oneOf` reports one error at the
/// parent — *"is not valid under any of the schemas listed in the 'oneOf' keyword"* — which names
/// neither the offending field nor the offending value. A refusal nobody can diagnose is a refusal
/// that gets silenced, so this re-validates the payload against the branch the instance actually
/// selected (identified by its single tag key) and reports the real violation with a real JSON
/// Pointer, e.g. `/verb/set_velocity/x_milli`.
///
/// The branch is looked up **from the schema itself**, never from a hardcoded variant list — so
/// adding a verb cannot make this drift. This runs only on the refusal path.
///
/// It never suppresses a violation: if the payload cannot be located or turns out to satisfy its
/// branch (i.e. the `oneOf` failed for some other reason), the original coarse errors are returned
/// unchanged. The admit/refuse decision was already made by the full-schema pass; this only
/// improves the diagnosis.
fn refine_verb_violations(
    instance: &serde_json::Value,
    coarse: Vec<EngineAgentSchemaError>,
) -> Vec<EngineAgentSchemaError> {
    let Some(verb) = instance.get("verb").and_then(serde_json::Value::as_object) else {
        return coarse;
    };
    let Some(defs) = proposal_schema().get("$defs") else {
        return coarse;
    };
    let branches = defs
        .pointer("/EngineAgentVerb/oneOf")
        .and_then(serde_json::Value::as_array);
    let Some(branches) = branches else {
        return coarse;
    };
    if verb.len() != 1 {
        return vec![EngineAgentSchemaError {
            instance_path: "/verb".to_string(),
            schema_path: "/$defs/EngineAgentVerb/oneOf".to_string(),
            message: format!(
                "a verb must name exactly one action, this named {}; the closed vocabulary is {:?}",
                verb.len(),
                verb_names(branches)
            ),
        }];
    }
    let (tag, payload) = verb.iter().next().expect("verb has exactly one key");
    let Some(branch) = branches
        .iter()
        .find(|branch| branch_tag(branch) == Some(tag.as_str()))
    else {
        return vec![EngineAgentSchemaError {
            instance_path: "/verb".to_string(),
            schema_path: "/$defs/EngineAgentVerb/oneOf".to_string(),
            message: format!(
                "verb {tag:?} is not in the closed vocabulary; the only verbs this engine accepts are {:?}",
                verb_names(branches)
            ),
        }];
    };
    let Some(payload_schema) = branch.pointer(&format!("/properties/{tag}")) else {
        return coarse;
    };
    // Re-root the branch's payload schema as its own document, carrying `$defs` so its internal
    // `$ref` still resolves.
    let mut document = payload_schema.clone();
    let Some(object) = document.as_object_mut() else {
        return coarse;
    };
    object.insert("$defs".to_string(), defs.clone());
    let Ok(validator) = jsonschema::draft202012::new(&document) else {
        return coarse;
    };
    let refined = validator
        .iter_errors(payload)
        .map(|error| EngineAgentSchemaError {
            instance_path: format!("/verb/{tag}{}", error.instance_path()),
            schema_path: error.schema_path().to_string(),
            message: error.to_string(),
        })
        .collect::<Vec<_>>();
    if refined.is_empty() { coarse } else { refined }
}

/// The tag key of a `oneOf` branch, read from its `required` list.
fn branch_tag(branch: &serde_json::Value) -> Option<&str> {
    branch
        .get("required")
        .and_then(serde_json::Value::as_array)
        .and_then(|required| required.first())
        .and_then(serde_json::Value::as_str)
}

/// Every verb the closed vocabulary accepts — read from the schema, so it cannot go stale.
fn verb_names(branches: &[serde_json::Value]) -> Vec<&str> {
    branches.iter().filter_map(branch_tag).collect()
}
