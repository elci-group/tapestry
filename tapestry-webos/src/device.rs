use tapestry_core::identity::DeviceIdentity;

/// A validated view of a webOS presentation endpoint.
///
/// `DeviceIdentity` and its capabilities must come from the native platform
/// probe. This wrapper validates the platform label; it does not invent TV
/// capabilities when the webOS bridge is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebOsEndpoint {
    identity: DeviceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceError {
    WrongPlatform(String),
    NotPresentable,
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongPlatform(platform) => {
                write!(f, "expected a webos identity, got {platform:?}")
            }
            Self::NotPresentable => write!(f, "device lacks a display or user input capability"),
        }
    }
}

impl std::error::Error for DeviceError {}

impl WebOsEndpoint {
    pub fn new(identity: DeviceIdentity) -> Result<Self, DeviceError> {
        if !identity.platform.eq_ignore_ascii_case("webos") {
            return Err(DeviceError::WrongPlatform(identity.platform));
        }
        Ok(Self { identity })
    }

    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    pub fn can_present(&self) -> bool {
        self.identity.can_present()
    }

    pub fn require_presentable(&self) -> Result<(), DeviceError> {
        if self.can_present() {
            Ok(())
        } else {
            Err(DeviceError::NotPresentable)
        }
    }
}
