use mads::prelude::*;

fn wrong_argument(_: u32) -> ValidationResult { Ok(()) }
fn wrong_result(_: &str) -> bool { true }
#[derive(Input)]
struct Argument { #[validate(custom = wrong_argument)] value: String }
#[derive(Input)]
struct Return { #[validate(custom = wrong_result)] value: String }
fn main() {}
