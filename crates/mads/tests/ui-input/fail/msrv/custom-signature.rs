use mads::prelude::*;

fn wrong_argument(_: u32) -> ValidationResult { Ok(()) }
fn wrong_result(_: &str) -> bool { true }
fn wrong_whole_argument(_: u32) -> ValidationResult { Ok(()) }
fn wrong_whole_result(_: &WholeResult) -> bool { true }
#[derive(Input)]
struct Argument { #[validate(custom = wrong_argument)] value: String }
#[derive(Input)]
struct Return { #[validate(custom = wrong_result)] value: String }
#[derive(Input)]
#[validate(custom = wrong_whole_argument)]
struct WholeArgument { value: String }
#[derive(Input)]
#[validate(custom = wrong_whole_result)]
struct WholeResult { value: String }
fn main() {}
