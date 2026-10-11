//! Shared hashing primitives for package, editor, and MCP integrity checks.

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
pub const FNV1A64_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv1a64 {
    state: u64,
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a64 {
    pub const fn new() -> Self {
        Self {
            state: FNV1A64_OFFSET,
        }
    }

    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }

    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV1A64_PRIME);
        }
        self
    }

    pub fn update_u8(&mut self, value: u8) -> &mut Self {
        self.update(&[value])
    }

    pub fn update_u64_le(&mut self, value: u64) -> &mut Self {
        self.update(&value.to_le_bytes())
    }

    pub fn update_i64_le(&mut self, value: i64) -> &mut Self {
        self.update_u64_le(value as u64)
    }

    pub const fn finish(&self) -> u64 {
        self.state
    }

    pub fn finish_hex(&self) -> String {
        format!("{:016x}", self.finish())
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish()
}

pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Returns true iff `value` is a canonical lowercase hex SHA-256 digest
/// (exactly 64 characters, each `0-9`/`a-f`). Uppercase is intentionally
/// rejected — `sha256_hex` above always emits lowercase, so callers compare
/// against that canonical form. Shared here to replace ~9 identical private
/// copies across litd-engine/litd-editor (#1491).
pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn to_stable_ron<T: Serialize>(value: &T) -> Result<String, ron::Error> {
    ron::ser::to_string_pretty(
        value,
        ron::ser::PrettyConfig::default()
            .new_line("\n".to_string())
            .indentor("  ".to_string())
            .compact_arrays(true),
    )
}

pub fn stable_fingerprint<T: Serialize>(value: &T) -> Result<String, ron::Error> {
    Ok(fnv1a64_hex(to_stable_ron(value)?.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Fixture<'a> {
        name: &'a str,
        values: &'a [u32],
    }

    #[test]
    fn stable_fingerprint_uses_shared_ron_and_fnv1a64() {
        let value = Fixture {
            name: "ashen",
            values: &[1, 3, 5],
        };
        let ron = to_stable_ron(&value).unwrap();
        assert_eq!(
            stable_fingerprint(&value).unwrap(),
            fnv1a64_hex(ron.as_bytes())
        );
        let mut hasher = Fnv1a64::new();
        hasher.update(b"li").update(b"td");
        assert_eq!(hasher.finish_hex(), fnv1a64_hex(b"litd"));
        assert_eq!(fnv1a64_hex(b"litd"), "bf6d98ad696c2956");
    }

    #[test]
    fn sha256_hex_is_lowercase_64_chars() {
        let digest = sha256_hex(b"litd");
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            digest,
            "0b27a6025921f1219f98238d8e0c3644789e8fe2d8d8271ed3e0fe607032eaad"
        );
    }

    #[test]
    fn is_sha256_hex_accepts_only_canonical_lowercase_64() {
        // A real digest produced by sha256_hex validates true (round-trip).
        let digest = sha256_hex(b"litd");
        assert!(is_sha256_hex(&digest));
        // Uppercased form of the same digest is rejected (canonical is lowercase).
        assert!(!is_sha256_hex(&digest.to_ascii_uppercase()));
        // Wrong length (63 and 65) rejected.
        assert!(!is_sha256_hex(&digest[..63]));
        assert!(!is_sha256_hex(&format!("{digest}0")));
        // Non-hex character in an otherwise-64-char string rejected.
        assert!(!is_sha256_hex(&("g".to_string() + &digest[1..])));
        // Empty rejected.
        assert!(!is_sha256_hex(""));
    }
}
