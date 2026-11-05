//! TAP/1 — the Tapestry Artefact Protocol schema.

pub mod chunk;
pub mod frame;
pub mod negotiate;

/// Protocol name and version constant.
pub const PROTOCOL_NAME: &str = "TAP";
pub const PROTOCOL_VERSION: u32 = 1;

/// Compatibility label used by the WebOS client.
#[allow(non_upper_case_globals)]
pub const ProtocolVersion: &str = "TAP/1";

/// Logical channels multiplexed over one session connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Channel {
    Control,
    Event,
    Artefact,
    Telemetry,
    Review,
    Media,
}
