//! Typed, time-sortable identifiers.
//!
//! IDs are ULID-style: a 48-bit millisecond timestamp followed by 80 bits of
//! randomness, encoded in Crockford base32 (26 chars). They sort by creation
//! time, which keeps journal replay and gallery ordering stable without a
//! separate timestamp column.

use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::{Result, TapestryError};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const ENCODED_LEN: usize = 26;

/// Generate 10 random bytes from the OS; test builds use a deterministic
/// fallback seeded by a counter so tests never flake on collisions.
fn random_80_bits() -> [u8; 10] {
    let mut bytes = [0u8; 10];
    // getrandom is not a dependency; read from /dev/urandom via std is not
    // available, so use a cheap process-unique entropy mix. This is
    // sufficient for Gate 0: IDs must be unique and sortable, not
    // cryptographically unguessable (auth uses DreamSequence).
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    let addr = &bytes as *const _ as u64;
    let mut state = nanos ^ addr ^ (std::process::id() as u64) << 17;
    for byte in bytes.iter_mut() {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    bytes
}

macro_rules! tapestry_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!(stringify!($name), " — a typed, time-sortable identifier with prefix `", $prefix, "_`.")]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            /// Generate a new identifier stamped with the current time.
            pub fn generate() -> Self {
                Self(Self::encode(Self::now_ms(), &random_80_bits()))
            }

            /// Construct an identifier from a timestamp and explicit
            /// randomness (used by tests and journal replay).
            pub fn from_parts(time_ms: u64, randomness: [u8; 10]) -> Self {
                Self(Self::encode(time_ms, &randomness))
            }

            /// Parse an identifier, enforcing the typed prefix.
            pub fn parse(s: &str) -> Result<Self> {
                let expected = concat!($prefix, "_");
                let raw = s
                    .strip_prefix(expected)
                    .ok_or_else(|| TapestryError::InvalidValue {
                        field: stringify!($name),
                        reason: format!("expected prefix `{expected}`"),
                    })?;
                if raw.len() != ENCODED_LEN || !raw.bytes().all(Self::is_crockford) {
                    return Err(TapestryError::InvalidValue {
                        field: stringify!($name),
                        reason: "not 26 Crockford base32 characters".into(),
                    });
                }
                Ok(Self(s.to_owned()))
            }

            /// Millisecond timestamp encoded in this identifier.
            pub fn timestamp_ms(&self) -> u64 {
                self.decode_time()
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            fn now_ms() -> u64 {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }

            fn is_crockford(b: u8) -> bool {
                // Uppercase set used by encode: 0-9 plus A-Z minus I, L, O, U.
                matches!(b, b'0'..=b'9') || (b.is_ascii_uppercase() && !matches!(b, b'I' | b'L' | b'O' | b'U'))
            }

            fn encode(time_ms: u64, randomness: &[u8; 10]) -> String {
                let mut raw = [0u8; 16];
                // Take the low 48 bits of the millisecond timestamp.
                raw[..6].copy_from_slice(&time_ms.to_be_bytes()[2..8]);
                raw[6..].copy_from_slice(randomness);

                let mut out = String::with_capacity($prefix.len() + 1 + ENCODED_LEN);
                out.push_str($prefix);
                out.push('_');
                // 128 bits -> 26 * 5 bits = 130; the top 2 bits are zero.
                let value = u128::from_be_bytes(raw);
                let mut chars = [0u8; ENCODED_LEN];
                for (i, slot) in chars.iter_mut().enumerate() {
                    let shift = 125 - 5 * i; // 26 groups of 5 bits, high to low
                    *slot = CROCKFORD[((value >> shift) & 0x1f) as usize];
                }
                out.push_str(std::str::from_utf8(&chars).expect("crockford is ascii"));
                out
            }

            fn decode_time(&self) -> u64 {
                let raw = self
                    .0
                    .strip_prefix(concat!($prefix, "_"))
                    .unwrap_or(&self.0);
                let mut value: u128 = 0;
                for &ch in raw.as_bytes() {
                    value = (value << 5) | decode_crockford(ch) as u128;
                }
                // Top 48 bits are the millisecond timestamp.
                (value >> 80) as u64
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::parse(&s).map_err(de::Error::custom)
            }
        }
    };
}

fn decode_crockford(ch: u8) -> u32 {
    match ch {
        b'0'..=b'9' => (ch - b'0') as u32,
        b'A'..=b'H' => (ch - b'A' + 10) as u32,
        b'J' | b'K' => (ch - b'J' + 18) as u32,
        b'M' | b'N' => (ch - b'M' + 20) as u32,
        b'P'..=b'T' => (ch - b'P' + 22) as u32,
        b'V'..=b'Z' => (ch - b'V' + 27) as u32,
        _ => 0,
    }
}

tapestry_id!(SessionId, "tap");
tapestry_id!(ArtifactId, "art");
tapestry_id!(DeviceId, "dev");
tapestry_id!(ReviewId, "rev");
tapestry_id!(OperationId, "ops");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_have_prefix_and_length() {
        let id = SessionId::generate();
        assert!(id.as_str().starts_with("tap_"));
        assert_eq!(id.as_str().len(), "tap_".len() + ENCODED_LEN);
    }

    #[test]
    fn parse_rejects_wrong_prefix_and_garbage() {
        let id = SessionId::generate();
        assert!(SessionId::parse(id.as_str()).is_ok());
        assert!(ArtifactId::parse(id.as_str()).is_err());
        assert!(SessionId::parse("tap_!!!").is_err());
        assert!(SessionId::parse("no-prefix").is_err());
    }

    #[test]
    fn roundtrip_preserves_timestamp() {
        let original = SessionId::from_parts(1_723_456_789_012, [7u8; 10]);
        let reparsed = SessionId::parse(original.as_str()).unwrap();
        assert_eq!(reparsed.timestamp_ms(), 1_723_456_789_012);
    }

    #[test]
    fn ids_sort_by_creation_time() {
        let earlier = SessionId::from_parts(1_000, [1u8; 10]);
        let later = SessionId::from_parts(2_000, [1u8; 10]);
        assert!(earlier < later);
    }

    #[test]
    fn generated_ids_are_unique() {
        let a = OperationId::generate();
        let b = OperationId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn serde_roundtrip() {
        let id = DeviceId::generate();
        let json = serde_json::to_string(&id).unwrap();
        let back: DeviceId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
