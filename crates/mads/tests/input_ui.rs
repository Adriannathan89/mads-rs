//! Compile contracts for input validation declarations.
#![cfg(feature = "http")]

#[test]
fn input_accepts_supported_shapes() {
    trybuild::TestCases::new().pass("tests/ui-input/pass/*.rs");
}

#[test]
fn input_rejects_invalid_declarations() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui-input/fail/*.rs");
    let output = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .unwrap();
    if String::from_utf8_lossy(&output.stdout).starts_with("rustc 1.85.") {
        cases.compile_fail("tests/ui-input/fail/msrv/*.rs");
    } else {
        cases.compile_fail("tests/ui-input/fail/stable/*.rs");
    }
}
