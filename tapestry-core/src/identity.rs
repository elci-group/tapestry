//! Device identity and advertised capabilities.
//!
//! LAN discovery advertisement (directive §10). Capabilities are *actual*,
//! queried from the platform — never inferred from device model alone.

use serde::{Deserialize, Serialize};

use crate::ids::DeviceId;

/// The TAP protocol version a device speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TapVersion {
    pub major: u32,
    pub minor: u32,
}

impl TapVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl std::fmt::Display for TapVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TAP/{}.{}", self.major, self.minor)
    }
}

impl Default for TapVersion {
    fn default() -> Self {
        Self::new(1, 0)
    }
}

/// Display characteristics advertised by a presentation endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayCharacteristics {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub hdr: bool,
}

/// Audio output characteristics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCharacteristics {
    pub channels: u32,
    pub surround: bool,
}

/// Input peripherals available on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum InputCharacteristic {
    Dpad,
    Keyboard,
    Touch,
    Voice,
}

/// Storage capability advertised by a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageCharacteristic {
    pub capacity_bytes: u64,
    pub available_bytes: u64,
    pub writable: bool,
    /// Removable volumes (USB) the device can address.
    pub removable: bool,
}

/// The full capability set a device advertises during discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySet {
    pub display: Option<DisplayCharacteristics>,
    pub audio: Option<AudioCharacteristics>,
    pub input: Vec<InputCharacteristic>,
    pub storage: Option<StorageCharacteristic>,
    /// Media codecs the platform reports it can decode.
    pub codecs: Vec<crate::capability::Codec>,
}

/// A discovered LAN node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub friendly_name: String,
    /// Platform label, e.g. "fire-tv", "android-tv", "desktop".
    pub platform: String,
    pub tap_version: TapVersion,
    pub capabilities: CapabilitySet,
    /// Whether the node is currently accepting sessions.
    pub available: bool,
}

impl DeviceIdentity {
    /// Whether this device can present artefacts at all (display + some input).
    pub fn can_present(&self) -> bool {
        self.capabilities.display.is_some() && !self.capabilities.input.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeviceIdentity {
        DeviceIdentity {
            device_id: DeviceId::generate(),
            friendly_name: "lounge-fire".into(),
            platform: "fire-tv".into(),
            tap_version: TapVersion::default(),
            capabilities: CapabilitySet {
                display: Some(DisplayCharacteristics {
                    width: 3840,
                    height: 2160,
                    refresh_hz: 60,
                    hdr: true,
                }),
                audio: Some(AudioCharacteristics {
                    channels: 6,
                    surround: true,
                }),
                input: vec![InputCharacteristic::Dpad, InputCharacteristic::Keyboard],
                storage: Some(StorageCharacteristic {
                    capacity_bytes: 8 << 30,
                    available_bytes: 4 << 30,
                    writable: true,
                    removable: true,
                }),
                codecs: vec![
                    crate::capability::Codec::H264,
                    crate::capability::Codec::H265,
                ],
            },
            available: true,
        }
    }

    #[test]
    fn serde_roundtrip() {
        let dev = sample();
        let back: DeviceIdentity =
            serde_json::from_str(&serde_json::to_string(&dev).unwrap()).unwrap();
        assert_eq!(dev, back);
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = r#"{"device_id":"dev_01ARZ3NDEKTSV4RRFFQ69G5FAV","friendly_name":"x","platform":"fire-tv","tap_version":{"major":1,"minor":0},"capabilities":{"display":null,"audio":null,"input":[],"storage":null,"codecs":[]},"available":true,"surprise":1}"#;
        assert!(serde_json::from_str::<DeviceIdentity>(json).is_err());
    }

    #[test]
    fn presentable_device_reports_can_present() {
        assert!(sample().can_present());
    }
}
