use mads::core::{Configuration, Secret};

#[derive(Configuration)]
struct Optional { #[config(default = 1)] value: Option<u16> }
#[derive(Configuration)]
struct SecretDefault { #[config(default = "secret")] value: Secret<String> }
#[derive(Configuration)]
struct Wrong { #[config(default = "wrong")] value: u16 }
#[derive(Configuration)]
struct Expression { #[config(default = 1 + 2)] value: u16 }

fn main() {}
