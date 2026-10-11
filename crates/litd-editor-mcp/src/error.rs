use thiserror::Error;

#[derive(Debug, Error)]
pub enum EditorMcpError {
    #[error("MCP manifest config field {field} is invalid: {reason}")]
    InvalidConfig { field: String, reason: String },

    #[error("{format} serialization failed: {reason}")]
    SerializationFailed {
        format: &'static str,
        reason: String,
    },
}
