use mads::core::Configuration;

fn wrong_argument(_: u32) -> Result<u16, ()> { Ok(1) }
fn wrong_result(_: &str) -> u16 { 1 }

#[derive(Configuration)]
struct Argument { #[config(parse_with = wrong_argument)] value: u16 }
#[derive(Configuration)]
struct Output { #[config(parse_with = wrong_result)] value: u16 }

fn main() {}
