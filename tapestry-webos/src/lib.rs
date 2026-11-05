//! Host-testable TAP endpoint adapter for a future LG webOS application.
//!
//! This crate deliberately does not claim to open a webOS socket or render a
//! TV surface. The native app must provide measured platform capabilities,
//! establish an authenticated channel, and pass received TAP frames here.

pub mod device;
pub mod transport;

pub use device::{DeviceError, WebOsEndpoint};
pub use tapestry_core::artifact::{Artifact, ArtifactKind};
pub use tapestry_core::capability::{Codec, CodecCapabilities, NegotiationMessage};
pub use tapestry_core::identity::{
    AudioCharacteristics, CapabilitySet, DeviceIdentity, DisplayCharacteristics,
    InputCharacteristic, StorageCharacteristic, TapVersion,
};
pub use tapestry_core::ids::{ArtifactId, DeviceId, SessionId};
pub use tapestry_core::protocol::frame::Frame;
pub use tapestry_core::protocol::{Channel, ProtocolVersion, PROTOCOL_VERSION};
pub use tapestry_core::review::{ReviewDecision, ReviewRecord};
pub use tapestry_core::session::{Session, SessionState};
pub use transport::{AcceptedChunk, Transport, TransportError, TransportState};
