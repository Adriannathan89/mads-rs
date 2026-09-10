use mads::core::Configuration;
#[derive(Configuration)]
struct FloatOverflow { #[config(validate(range(max = 1e100)))] value: f32 }
fn main() {}
