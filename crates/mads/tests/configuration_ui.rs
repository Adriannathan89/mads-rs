//! Compile contracts for typed configuration declarations.

#[test]
fn configuration_accepts_supported_shapes() {
    trybuild::TestCases::new().pass("tests/ui-configuration/pass/*.rs");
}

#[test]
fn configuration_rejects_invalid_declarations() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui-configuration/fail/*.rs");
    let output = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .unwrap();
    if String::from_utf8_lossy(&output.stdout).starts_with("rustc 1.85.") {
        cases.compile_fail("tests/ui-configuration/fail/msrv/*.rs");
    } else {
        cases.compile_fail("tests/ui-configuration/fail/stable/*.rs");
    }
}
