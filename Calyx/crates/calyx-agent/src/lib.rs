//! `calyx-agent` — the domain-neutral, fail-closed LLM transport **atom**.
//!
//! This is the workspace's ONE LLM call site (issue #1454). Everything above it — the
//! co-writer, NPC minds, the Immortal director — composes on top of this atom rather than
//! reimplementing HTTP, auth, or provider validation. Per `docs2/BUILDING_ON_CALYX.md`
//! (atoms -> associations -> bits -> kernel -> compose), an atom is *irreducible* and
//! *independently measurable*: this one does exactly one thing — turn a validated prompt +
//! validated credentials into a validated completion — and it knows nothing about games,
//! NPCs, markets, or any other domain.
//!
//! # The three parts
//!
//! * [`AgentSecrets`] — the Infisical-bound secret contract. The vault coordinates are
//!   [`InfisicalSource`] *parameters*, so any project can bind its own; [`AgentSecrets::gameeditor`]
//!   binds GameEditor's. The key lives in `Zeroizing` memory and is never returned, logged,
//!   serialized, or `Debug`-printed — [`AgentSecrets::metadata`] exposes only non-secret shape.
//! * [`AgentTransport`] — the chat-completions call, with the provider's wire contract enforced
//!   clause by clause.
//! * [`AgentError`] — a stable machine-readable [`code`][AgentError::code] plus a
//!   secret-redacted message.
//!
//! # Doctrine
//!
//! Fail closed, always. There is no fallback provider, no stub, no canned or degraded local
//! text, no silent retry, and no coercion of an unsupported model to a supported one. An agent
//! call either returns a completion that satisfied every clause of the contract, or it returns
//! an [`AgentError`] naming exactly which clause failed. A plausible-looking wrong answer is
//! strictly worse than a loud failure.
//!
//! # Secrets
//!
//! The key is read from the process environment ONLY, and the process is expected to have been
//! launched under Infisical injection:
//!
//! ```text
//! infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>
//! ```
//!
//! Routine `cargo test` neither requires the key nor touches the network: the unit tests cover
//! validation only. The real provider is exercised by the FSV example, behind the `fsv` feature:
//!
//! ```text
//! cargo run --features fsv --example issue1454_deepseek_agent_fsv
//! ```
//!
//! # Example
//!
//! ```no_run
//! use calyx_agent::{AgentModel, AgentRequest, AgentSecrets, AgentTransport};
//!
//! # fn main() -> Result<(), calyx_agent::AgentError> {
//! let transport = AgentTransport::new(AgentSecrets::gameeditor()?);
//! let request = AgentRequest::new(
//!     AgentModel::Flash,
//!     "You are a strict JSON generator. Reply with JSON only.",
//!     "Return {\"sum\": <the sum of 2 and 2>}.",
//! )
//! .with_max_tokens(64);
//!
//! let completion = transport.complete(&request)?;
//! println!("request id {} | content {}", completion.id, completion.content);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod model;
pub mod secrets;
pub mod transport;
pub mod wire;

// Exercises the REAL transport HTTP path (`AgentTransport::complete`) end-to-end against a local
// loopback `TcpListener` serving captured DeepSeek response/error JSON. Kept out of the product
// build; compiled only under `cargo test`.
#[cfg(test)]
mod transport_http_tests;

pub use error::{AgentError, AgentErrorKind, Result, codes, redact_secret};
pub use model::{AgentModel, MODEL_FLASH, MODEL_PRO};
pub use secrets::{
    AgentSecretMetadata, AgentSecrets, DEEPSEEK_BASE_URL, GAMEEDITOR_API_KEY_NAME,
    GAMEEDITOR_ENVIRONMENT, GAMEEDITOR_PROJECT_ID, GAMEEDITOR_SECRET_PATH, InfisicalSource,
};
pub use transport::{
    AgentRawResponse, AgentRequest, AgentTransport, DEFAULT_MAX_TOKENS, DEFAULT_TEMPERATURE,
    DEFAULT_TIMEOUT_SECS,
};
pub use wire::{AgentCompletion, AgentUsage};
