//! Codec negotiation: H.265 only after explicit probing, H.264 fallback,
//! honest refusal when nothing was probed.

use tapestry_core::capability::NegotiationMessage;
use tapestry_core::capability::{negotiate_hevc, Codec, CodecCapabilities};

#[test]
fn probed_h265_device_gets_h265() {
    let caps = CodecCapabilities::from_probe(vec![Codec::H264, Codec::H265]);
    assert_eq!(negotiate_hevc(&caps).unwrap(), Codec::H265);
}

#[test]
fn h265_capable_but_h265_only_after_probe_order_respected() {
    // Probed order does not matter: capability, not preference order.
    let caps = CodecCapabilities::from_probe(vec![Codec::H265, Codec::H264]);
    assert_eq!(negotiate_hevc(&caps).unwrap(), Codec::H265);
}

#[test]
fn h264_only_device_falls_back() {
    let caps = CodecCapabilities::from_probe(vec![Codec::H264]);
    assert_eq!(negotiate_hevc(&caps).unwrap(), Codec::H264);
}

#[test]
fn unprobed_device_refuses_hevc_content() {
    // Fire TV platform label must not imply codec support (directive §40).
    let caps = CodecCapabilities::unknown();
    assert!(negotiate_hevc(&caps).is_err());
    assert_eq!(caps.select_for_hevc_content(), None);
}

#[test]
fn negotiation_message_types_are_wire_stable() {
    let propose = NegotiationMessage::Propose {
        codecs: vec![Codec::H265, Codec::H264],
    };
    let json = serde_json::to_string(&propose).unwrap();
    assert!(json.contains("\"propose\""));
    let back: NegotiationMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(propose, back);
}

#[test]
fn negotiate_returns_error_type_not_panic() {
    let caps = CodecCapabilities::from_probe(vec![]);
    let result = negotiate_hevc(&caps);
    let err = result.unwrap_err();
    assert!(err.to_string().contains("refusing to assume codec support"));
}
