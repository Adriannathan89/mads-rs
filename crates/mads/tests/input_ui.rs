//! Compile-contract matrix for Input helper grammar, field shapes, literals,
//! nested bounds, callbacks, renamed dependencies, and minimal generic bounds.
#![cfg(feature = "http")]

#[test]
fn input_accepts_supported_shapes_and_minimal_generic_bounds() {
    trybuild::TestCases::new().pass("tests/ui-input/pass/*.rs");
}

#[test]
fn input_rejects_helper_grammar_shapes_literals_and_callback_contracts() {
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
