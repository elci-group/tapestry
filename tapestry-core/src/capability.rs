//! Codec capability descriptors and negotiation (directives §18, §40).
//!
//! Rule: codec selection happens only after explicit capability probing.
//! H.265 is never assumed from platform identity.

use serde::{Deserialize, Serialize};

/// Codecs Tapestry reasons about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Codec {
    H264,
    H265,
}

impl Codec {
    /// Human/wire label.
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::H265 => "h265",
        }
    }
}

/// What a device reports after actual platform probing (not model lookup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecCapabilities {
    /// Codecs confirmed by a platform query, in probe order.
    pub probed: Vec<Codec>,
}

impl CodecCapabilities {
    pub fn from_probe(probed: Vec<Codec>) -> Self {
        Self { probed }
    }

    /// Empty capability set: the honest result of "we could not probe".
    /// Negotiating against this never yields H.265.
    pub fn unknown() -> Self {
        Self { probed: Vec::new() }
    }

    pub fn supports(&self, codec: Codec) -> bool {
        self.probed.contains(&codec)
    }

    /// Select the best playback codec for HEVC-class content given what was
    /// actually probed. Returns `None` when the device supports neither
    /// codec — callers must then fall back to still-image or remote-render
    /// strategies, never to guessing.
    pub fn select_for_hevc_content(&self) -> Option<Codec> {
        if self.supports(Codec::H265) {
            Some(Codec::H265)
        } else if self.supports(Codec::H264) {
            Some(Codec::H264)
        } else {
            None
        }
    }
}

/// The negotiation exchange: presenter proposes, device answers with what
/// it can actually decode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NegotiationMessage {
    /// Presenter proposes preferred codecs, most preferred first.
    Propose { codecs: Vec<Codec> },
    /// Device answers with its probed subset.
    Answer { capabilities: CodecCapabilities },
    /// Presenter commits to one codec after negotiation.
    Confirm { codec: Codec },
}

/// Negotiate a codec for HEVC-class content: propose → answer → confirm.
/// Returns `Err` when the device can decode neither proposed codec.
pub fn negotiate_hevc(device_answer: &CodecCapabilities) -> crate::Result<Codec> {
    device_answer.select_for_hevc_content().ok_or_else(|| {
        crate::TapestryError::IllegalTransition(
            "device probed neither h265 nor h264; refusing to assume codec support".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_h265_falls_back_to_h264() {
        assert_eq!(
            CodecCapabilities::from_probe(vec![Codec::H264, Codec::H265]).select_for_hevc_content(),
            Some(Codec::H265)
        );
        assert_eq!(
            CodecCapabilities::from_probe(vec![Codec::H264]).select_for_hevc_content(),
            Some(Codec::H264)
        );
    }

    #[test]
    fn unprobed_device_never_gets_h265() {
        // The directive's non-negotiable: no probing, no H.265.
        assert_eq!(CodecCapabilities::unknown().select_for_hevc_content(), None);
        assert!(negotiate_hevc(&CodecCapabilities::unknown()).is_err());
    }

    #[test]
    fn negotiation_message_roundtrip() {
        let msg = NegotiationMessage::Answer {
            capabilities: CodecCapabilities::from_probe(vec![Codec::H264]),
        };
        let back: NegotiationMessage =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(msg, back);
    }
}
