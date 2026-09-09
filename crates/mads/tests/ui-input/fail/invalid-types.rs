use mads::Input;

#[derive(Input)]
struct Email { #[validate(email)] value: u32 }
#[derive(Input)]
struct Negative { #[validate(negative)] value: u64 }
#[derive(Input)]
struct Required { #[validate(required)] value: String }
#[derive(Input)]
struct Range { #[validate(range(min = 0))] value: bool }
#[derive(Input)]
struct Map { #[validate(nested)] value: std::collections::HashMap<u32, String> }
#[derive(Input)]
struct Collection { #[validate(length(min = 1))] value: std::collections::HashSet<String> }
fn main() {}
