use mads::core::Configuration;

#[derive(Configuration)]
struct Email { #[config(validate(email))] value: u32 }
#[derive(Configuration)]
struct Negative { #[config(validate(negative))] value: u32 }
#[derive(Configuration)]
struct Zero { #[config(validate(multiple_of = 0))] value: i64 }
#[derive(Configuration)]
struct Range { #[config(validate(range(min = 9, max = 1)))] value: i64 }
#[derive(Configuration)]
struct Length { #[config(validate(length(exact = 1, max = 2)))] value: String }
#[derive(Configuration)]
struct Duplicate { #[config(validate(email, email))] value: String }
#[derive(Configuration)]
struct Unknown { #[config(validate(magic))] value: String }

fn main() {}
