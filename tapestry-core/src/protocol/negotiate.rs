//! Capability negotiation for TAP sessions (directive §10).
//!
//! Negotiation precedes presentation: a receiver answers a proposal with
//! what it *probed*, not what its platform name suggests.

use serde::{Deserialize, Serialize};

use crate::capability::CodecCapabilities;
use crate::identity::TapVersion;

/// Session handshake: sender declares protocol version and intent;
/// receiver answers with its own version and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Handshake {
    /// Sender → receiver.
    Hello { tap_version: TapVersion },
    /// Receiver → sender, after probing its real capabilities.
    Welcome {
        tap_version: TapVersion,
        capabilities: CodecCapabilities,
    },
    /// Sender rejects the negotiated version; session closes.
    Reject { reason: String },
}

impl Handshake {
    /// Whether a Hello/Welcome pair is version-compatible (same major).
    pub fn versions_compatible(a: &TapVersion, b: &TapVersion) -> bool {
        a.major == b.major
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Codec;

    #[test]
    fn handshake_roundtrip() {
        let hs = Handshake::Welcome {
            tap_version: TapVersion::new(1, 0),
            capabilities: CodecCapabilities::from_probe(vec![Codec::H264, Codec::H265]),
        };
        let back: Handshake = serde_json::from_str(&serde_json::to_string(&hs).unwrap()).unwrap();
        assert_eq!(hs, back);
    }

    #[test]
    fn same_major_compatible_different_major_not() {
        assert!(Handshake::versions_compatible(
            &TapVersion::new(1, 0),
            &TapVersion::new(1, 3)
        ));
        assert!(!Handshake::versions_compatible(
            &TapVersion::new(1, 0),
            &TapVersion::new(2, 0)
        ));
    }
}
