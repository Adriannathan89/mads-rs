use mads::core::Configuration;
#[derive(Configuration)]
struct Contradiction { #[config(validate(nonempty, length(max = 0)))] value: String }
fn main() {}
