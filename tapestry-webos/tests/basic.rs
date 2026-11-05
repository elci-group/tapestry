#[test]
fn test_lib_imports_compile() {
    // This test ensures the lib.rs can be compiled without errors
    // We'll test a few basic functions to ensure imports work
    use tapestry_core::artifact::Artifact;
    use tapestry_core::ids::SessionId;
    let _ = SessionId::generate();
    let _ = Artifact::builder(
        SessionId::generate(),
        tapestry_core::artifact::ArtifactKind::Image,
    );
}
