use mads::core::Configuration;
#[derive(Configuration)]
struct IntegerOverflow { #[config(validate(range(max = 256)))] value: u8 }
fn main() {}
