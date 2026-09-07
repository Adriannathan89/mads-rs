//! Compile-time contracts for explicit database-to-HTTP delivery mapping.
#![cfg(all(feature = "http", feature = "database"))]

#[test]
fn database_http_mapping_requires_an_explicit_method_call() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui-database-http/pass/*.rs");
    if rustc_is_msrv() {
        cases.compile_fail("tests/ui-database-http/fail/msrv/*.rs");
    } else {
        cases.compile_fail("tests/ui-database-http/fail/implicit.rs");
    }
}

fn rustc_is_msrv() -> bool {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).starts_with("rustc 1.85."))
        .unwrap_or(false)
}
