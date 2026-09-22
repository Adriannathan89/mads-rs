//! Compile tests for the public MADS attribute macros.

#[test]
fn core_attributes_accept_supported_shapes() {
    trybuild::TestCases::new().pass("tests/ui/pass/*.rs");
}

#[test]
fn core_attributes_reject_unsupported_shapes() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/fail/*.rs");
    if rustc_is_msrv() {
        tests.compile_fail("tests/ui/fail/msrv/*.rs");
    } else {
        tests.compile_fail("tests/ui/fail/stable/*.rs");
    }
}

fn rustc_is_msrv() -> bool {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).starts_with("rustc 1.94."))
        .unwrap_or(false)
}
