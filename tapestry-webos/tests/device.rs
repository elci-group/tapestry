use tapestry_webos::{
    CapabilitySet, DeviceError, DeviceId, DeviceIdentity, DisplayCharacteristics,
    InputCharacteristic, TapVersion, WebOsEndpoint,
};

fn identity(platform: &str, has_display: bool) -> DeviceIdentity {
    DeviceIdentity {
        device_id: DeviceId::generate(),
        friendly_name: "test-tv".into(),
        platform: platform.into(),
        tap_version: TapVersion::default(),
        capabilities: CapabilitySet {
            display: has_display.then_some(DisplayCharacteristics {
                width: 1920,
                height: 1080,
                refresh_hz: 60,
                hdr: false,
            }),
            audio: None,
            input: vec![InputCharacteristic::Dpad],
            storage: None,
            codecs: vec![],
        },
        available: true,
    }
}

#[test]
fn endpoint_reuses_shared_identity_and_checks_platform() {
    let endpoint = WebOsEndpoint::new(identity("webos", true)).unwrap();
    assert!(endpoint.can_present());
    assert_eq!(endpoint.identity().platform, "webos");
    assert!(WebOsEndpoint::new(identity("android-tv", true)).is_err());
}

#[test]
fn endpoint_does_not_claim_presentation_without_a_display() {
    let endpoint = WebOsEndpoint::new(identity("webos", false)).unwrap();
    assert!(!endpoint.can_present());
    assert_eq!(
        endpoint.require_presentable(),
        Err(DeviceError::NotPresentable)
    );
}
