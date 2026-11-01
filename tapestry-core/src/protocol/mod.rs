//! TAP/1 — the Tapestry Artefact Protocol schema (directive §9).
//!
//! Framed, multiplexed, resumable. This module defines the wire *schema*:
//! frames, channels, chunks and negotiation messages. Byte-level framing
//! and encryption belong to the transport swarm.

pub mod chunk;
pub mod frame;
pub mod negotiate;

/// Protocol name and version constant.
pub const PROTOCOL_NAME: &str = "TAP";
pub const PROTOCOL_VERSION: u32 = 1;

/// Logical channels multiplexed over one session connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Channel {
    Control,
    Event,
    Artefact,
    Telemetry,
    Review,
    Media,
}

use serde::{Deserialize, Serialize};
